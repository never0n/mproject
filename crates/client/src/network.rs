use crate::{Movement, Player, Terrain, character::AnimatedActor};
use bevy::prelude::*;
use futures_util::{SinkExt, StreamExt};
use mortown_protocol::{
    Actor, Character, ClientMessage, PROTOCOL_VERSION, ServerMessage, WORLD_VERSION,
};
use std::{
    collections::HashSet,
    sync::{Mutex, mpsc},
    time::Duration,
};
use tokio_tungstenite::tungstenite::Message;

async fn send_frame<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    message: Message,
) -> Result<(), &'static str>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    tokio::time::timeout(Duration::from_secs(2), socket.send(message))
        .await
        .map_err(|_| "Соединение не отвечает")?
        .map_err(|_| "Соединение закрыто")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Menu,
    Offline,
    Connecting,
    Online,
    Selection,
}
#[allow(clippy::large_enum_variant)]
enum Event {
    Message(ServerMessage),
    Closed(String),
}
#[derive(Resource)]
pub struct Network {
    pub mode: Mode,
    pub status: String,
    pub character: Option<Character>,
    pub characters: Vec<Character>,
    pub selected: Option<i64>,
    pub pending: bool,
    pub selection_revision: u64,
    pub registered: bool,
    pub authoritative: Option<Actor>,
    pub roster: Vec<Actor>,
    pub residents: Vec<mortown_protocol::admin::NpcState>,
    pub monsters: Vec<mortown_protocol::Monster>,
    pub areas: Vec<mortown_protocol::skills::Area>,
    pub alliance: mortown_protocol::alliance::Reply,
    pub alliance_pending: bool,
    pub party: mortown_protocol::party::Reply,
    pub party_pending: bool,
    pub party_received_at: f64,
    pub chat: crate::chat::Log,
    pub mail: crate::mail::Mailbox,
    pub exchange: Option<mortown_protocol::exchange::View>,
    pub exchange_status: String,
    pub exchange_pending: bool,
    pub address: String,
    tx: Option<tokio::sync::mpsc::Sender<ClientMessage>>,
    rx: Option<Mutex<mpsc::Receiver<Event>>>,
    seq: u64,
    sent_at: f64,
}
impl Default for Network {
    fn default() -> Self {
        Self {
            mode: Mode::Menu,
            status: "Войдите в аккаунт или зарегистрируйтесь".into(),
            character: None,
            characters: vec![],
            selected: None,
            pending: false,
            selection_revision: 0,
            registered: false,
            authoritative: None,
            roster: vec![],
            monsters: vec![],
            residents: vec![],
            areas: vec![],
            alliance: default(),
            alliance_pending: false,
            party: default(),
            party_pending: false,
            party_received_at: 0.,
            chat: default(),
            mail: default(),
            exchange: None,
            exchange_status: String::new(),
            exchange_pending: false,
            address: std::env::var("MORTOWN_SERVER")
                .unwrap_or_else(|_| "ws://127.0.0.1:3000/world".into()),
            tx: None,
            rx: None,
            seq: 0,
            sent_at: -1.,
        }
    }
}
impl Network {
    pub fn begin(&mut self, message: ClientMessage) {
        self.disconnect();
        self.mode = Mode::Connecting;
        self.registered = false;
        self.status = "Соединяемся с миром…".into();
        let (events, rx) = mpsc::sync_channel(64);
        let (commands, mut input) = tokio::sync::mpsc::channel::<ClientMessage>(8);
        self.tx = Some(commands);
        self.rx = Some(Mutex::new(rx));
        self.seq = 0;
        self.sent_at = -1.;
        let address = self.address.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let result=match runtime {
                Ok(runtime) => runtime.block_on(async {
                    let (mut socket,_) = tokio::time::timeout(Duration::from_secs(8), tokio_tungstenite::connect_async(address)).await.map_err(|_| "Сервер не отвечает")?.map_err(|_| "Не удалось подключиться к серверу")?;
                    send_frame(&mut socket, Message::Text(serde_json::to_string(&message).map_err(|_| "Ошибка запроса")?.into())).await?;
                    let mut logout=false;
                    let mut last_received=tokio::time::Instant::now();
                    let mut heartbeat=tokio::time::interval(Duration::from_secs(1));
                    loop { tokio::select! {
                        _=heartbeat.tick()=> { if last_received.elapsed()>Duration::from_secs(15) { return Err("Сервер перестал отвечать"); } }
                        incoming=socket.next() => {
                            last_received=tokio::time::Instant::now();
                            match incoming {
                                Some(Ok(Message::Text(text))) => {
                                    let message: ServerMessage=serde_json::from_str(&text).map_err(|_| "Несовместимый ответ сервера")?;
                                    let saved=matches!(message,ServerMessage::Saved|ServerMessage::Registered);
                                    match events.try_send(Event::Message(message)) {
                                        Ok(()) => {},
                                        Err(mpsc::TrySendError::Disconnected(_)) => break,
                                        Err(mpsc::TrySendError::Full(_)) => { return Err("Клиент не успевает получать состояние мира"); }
                                    }
                                    if saved { break; }
                                }
                                Some(Ok(Message::Ping(p))) => { send_frame(&mut socket, Message::Pong(p)).await?; }
                                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                                _ => {}
                            }
                        }
                        command=input.recv(), if !logout => {
                            let command=command.unwrap_or(ClientMessage::Logout);
                            logout=matches!(command,ClientMessage::Logout);
                            send_frame(&mut socket, Message::Text(serde_json::to_string(&command).map_err(|_| "Ошибка команды")?.into())).await?;
                        }
                    }}
                    let _=tokio::time::timeout(Duration::from_secs(2),socket.close(None)).await;
                    Ok(())
                }),
                Err(_) => Err("Не удалось запустить соединение"),
            };
            let _ = events.send(Event::Closed(
                result.err().unwrap_or("Соединение завершено").into(),
            ));
        });
    }
    pub fn disconnect(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.try_send(ClientMessage::Logout);
        }
        self.mode = Mode::Menu;
        self.authoritative = None;
        self.character = None;
        self.characters.clear();
        self.selected = None;
        self.pending = false;
        self.roster.clear();
        self.monsters.clear();
        self.residents.clear();
        self.areas.clear();
        self.alliance = default();
        self.alliance_pending = false;
        self.party = default();
        self.party_pending = false;
        self.chat = default();
        self.mail = default();
        self.exchange = None;
        self.exchange_status.clear();
        self.exchange_pending = false;
    }
    pub fn alliance_request(&mut self, command: mortown_protocol::alliance::Command) {
        if self.mode != Mode::Online || self.alliance_pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Alliance { command }).is_ok())
        {
            self.alliance_pending = true;
        } else {
            self.alliance.message = "Соединение недоступно".into();
        }
    }
    pub fn party_request(&mut self, command: mortown_protocol::party::Command) {
        if self.mode != Mode::Online || self.party_pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Party { command }).is_ok())
        {
            self.party_pending = true;
        } else {
            self.status = "Соединение недоступно".into();
        }
    }
    pub fn chat_request(&mut self, command: mortown_protocol::chat::Command) {
        if self.mode != Mode::Online || self.chat.pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Chat { command }).is_ok())
        {
            self.chat.pending = true;
            self.chat.status = "Отправляем…".into();
        } else {
            self.chat.status = "Соединение недоступно".into();
        }
    }
    pub fn exchange_request(&mut self, command: mortown_protocol::exchange::Command) {
        if self.mode != Mode::Online || self.exchange_pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Exchange { command }).is_ok())
        {
            self.exchange_pending = true;
            self.exchange_status = "Подождите…".into();
        } else {
            self.exchange_status = "Соединение недоступно".into();
        }
    }
    pub fn mail_request(&mut self, command: mortown_protocol::mail::Command) {
        if self.mode != Mode::Online || self.mail.pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Mail { command }).is_ok())
        {
            self.mail.pending = true;
            self.mail.status = "Подождите…".into();
        } else {
            self.mail.status = "Соединение недоступно".into();
        }
    }
    pub fn request(&mut self, message: ClientMessage) {
        if self.pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(message).is_ok())
        {
            self.pending = true;
            self.status = "Подождите…".into();
        } else {
            self.status = "Соединение недоступно".into();
        }
    }
    pub fn action(&mut self, message: ClientMessage) {
        if self.mode != Mode::Online {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(message).is_ok())
        {
            self.status.clear();
        } else {
            self.status = "Соединение недоступно".into();
        }
    }
    pub fn leave_world(&mut self) {
        if self.mode == Mode::Online {
            self.request(ClientMessage::LeaveWorld);
            if self.pending {
                self.mode = Mode::Selection;
            }
        }
    }
    pub fn intent(&mut self, time: f64, target: [f32; 2], walking: bool) {
        if self.mode != Mode::Online || time - self.sent_at < 0.045 {
            return;
        }
        self.sent_at = time;
        self.seq += 1;
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(ClientMessage::Move {
                seq: self.seq,
                target,
                walking,
            });
        }
    }
    pub fn teleport(&mut self, target: [f32; 2]) {
        if self.mode != Mode::Online {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(ClientMessage::Teleport { target });
        }
    }
    pub fn playing(&self) -> bool {
        matches!(self.mode, Mode::Offline | Mode::Online)
    }
}
#[derive(Component)]
pub struct Remote {
    pub id: i64,
    target: Vec3,
    yaw: f32,
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn receive(
    mut commands: Commands,
    mut network: ResMut<Network>,
    assets: Res<AssetServer>,
    time: Res<Time>,
    mut hero: Single<&mut Transform, (With<Player>, Without<Remote>)>,
    mut remotes: Query<(Entity, &mut Remote, &mut Transform), Without<Player>>,
    mut movement: ResMut<Movement>,
    world: Res<Terrain>,
) {
    let events: Vec<_> = network
        .rx
        .as_ref()
        .map(|rx| rx.lock().unwrap().try_iter().collect())
        .unwrap_or_default();
    for event in events {
        match event {
            Event::Message(ServerMessage::Alliance { mut reply })
                if network.mode == Mode::Online =>
            {
                if reply.response {
                    network.alliance_pending = false;
                }
                if reply.message.is_empty()
                    && reply.alliance.as_ref().map(|a| a.id)
                        == network.alliance.alliance.as_ref().map(|a| a.id)
                {
                    reply.message = network.alliance.message.clone();
                }
                network.alliance = reply;
            }
            Event::Message(ServerMessage::Party { reply }) if network.mode == Mode::Online => {
                network.party_received_at = time.elapsed_secs_f64();
                if !reply.message.is_empty() {
                    network.status = reply.message.clone();
                }
                if reply.response {
                    network.party_pending = false;
                }
                let mut reply = reply;
                if reply.message.is_empty()
                    && reply.group.as_ref().map(|g| g.id)
                        == network.party.group.as_ref().map(|g| g.id)
                {
                    reply.message = network.party.message.clone();
                }
                network.party = reply;
            }
            Event::Message(ServerMessage::Chat { reply }) if network.mode == Mode::Online => {
                if let Some(owner) = network.character.as_ref().map(|c| c.id) {
                    network.chat.receive(reply, owner);
                }
            }
            Event::Message(ServerMessage::Exchange { reply }) if network.mode == Mode::Online => {
                if let Some(update) = reply.update
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| c.id == update.character.id)
                {
                    network.character = Some(update.character);
                    network.authoritative = Some(update.actor);
                }
                network.exchange = reply.view;
                network.exchange_status = reply.message.clone();
                network.status = reply.message;
                network.exchange_pending = false;
            }
            Event::Message(ServerMessage::Mail { reply }) if network.mode == Mode::Online => {
                if let Some(update) = reply.inventory_update()
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| c.id == update.character.id)
                {
                    network.character = Some(update.character.clone());
                    network.authoritative = Some(update.actor.clone());
                }
                network.mail.receive(reply);
            }
            Event::Message(ServerMessage::EconomyUpdated { character, actor }) => {
                if network.mode == Mode::Online
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| c.id == character.id)
                {
                    network.status = actor.reward_status.clone();
                    network.character = Some(character);
                    network.authoritative = Some(actor);
                    network.pending = false;
                }
            }
            Event::Message(ServerMessage::CharacterUpdated { mut character }) => {
                if network.mode == Mode::Online
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| c.id == character.id)
                {
                    if let Some(a) = network.authoritative.as_ref() {
                        if a.progress.kills
                            >= character.stats.get("kills").copied().unwrap_or(0.) as u64
                        {
                            let old_level = character.stats["level"] as u32;
                            mortown_protocol::progression::grow(
                                &mut character.stats,
                                a.progress.level.saturating_sub(old_level),
                            );
                            a.progress.write_stats(&mut character.stats);
                            character.loot = a.loot.clone();
                            mortown_protocol::items::include_loot(
                                &mut character.inventory,
                                &character.loot,
                            );
                        }
                        character.stats.insert("current_hp".into(), a.hp);
                        character.stats.insert("current_mana".into(), a.mana);
                        character.stats.insert("current_stamina".into(), a.stamina);
                    }
                    network.character = Some(character);
                    network.pending = false;
                    network.status = "Экипировка сохранена".into();
                }
            }
            Event::Message(ServerMessage::Registered) => {
                network.disconnect();
                network.registered = true;
                network.status = "Аккаунт создан. Войдите, чтобы создать персонажа".into();
            }
            Event::Message(ServerMessage::Characters {
                characters,
                selected,
                ..
            }) => {
                if !matches!(network.mode, Mode::Connecting | Mode::Selection) {
                    continue;
                }
                network.selected = selected
                    .or(network.selected)
                    .filter(|id| characters.iter().any(|c| c.id == *id))
                    .or_else(|| characters.first().map(|c| c.id));
                network.characters = characters;
                network.mode = Mode::Selection;
                network.pending = false;
                network.selection_revision += 1;
                network.character = None;
                network.authoritative = None;
                network.roster.clear();
                network.monsters.clear();
                network.areas.clear();
                network.alliance = default();
                network.alliance_pending = false;
                network.party = default();
                network.party_pending = false;
                network.chat = default();
                network.mail = default();
                network.status = if network.characters.is_empty() {
                    "Создайте первого персонажа".into()
                } else {
                    "Выберите героя для входа в мир".into()
                };
                *movement = Movement::default();
            }
            Event::Message(ServerMessage::Welcome {
                version,
                world: version_world,
                character,
                actor,
            }) => {
                if version != PROTOCOL_VERSION || version_world != WORLD_VERSION {
                    network.disconnect();
                    network.status = "Версия мира не совпадает".into();
                    continue;
                }
                if network.mode != Mode::Selection {
                    continue;
                }
                hero.translation = Vec3::from_array(actor.position);
                hero.rotation = Quat::from_rotation_y(actor.yaw);
                network.status = format!(
                    "{} · уровень {:.0}",
                    character.nickname, character.stats["level"]
                );
                network.character = Some(character);
                network.authoritative = Some(actor);
                network.mode = Mode::Online;
                network.pending = false;
                network.seq = 0;
                network.sent_at = -1.;
                *movement = Movement::default();
            }
            Event::Message(ServerMessage::Snapshot {
                actors,
                monsters,
                residents,
                areas,
                ..
            }) if network.mode == Mode::Online => {
                if let Some(id) = network.character.as_ref().map(|p| p.id) {
                    network.authoritative = actors.iter().find(|a| a.id == id).cloned();
                }
                if let Some(a) = network.authoritative.clone() {
                    if a.hp <= 0. {
                        *movement = Movement::default();
                    }
                    if let Some(c) = network.character.as_mut() {
                        if a.progress.kills >= c.stats.get("kills").copied().unwrap_or(0.) as u64 {
                            let old_level = c.stats["level"] as u32;
                            mortown_protocol::progression::grow(
                                &mut c.stats,
                                a.progress.level.saturating_sub(old_level),
                            );
                            a.progress.write_stats(&mut c.stats);
                            c.loot = a.loot.clone();
                            mortown_protocol::items::include_loot(&mut c.inventory, &c.loot);
                        }
                        c.stats.insert("current_hp".into(), a.hp);
                        c.stats.insert("current_mana".into(), a.mana);
                        c.stats.insert("current_stamina".into(), a.stamina);
                    }
                }
                network.residents = residents;
                network.monsters = monsters;
                network.areas = areas;
                network.roster = actors;
            }
            Event::Message(ServerMessage::Error { message, fatal }) => {
                network.status = message;
                network.pending = false;
                if fatal {
                    network.disconnect();
                }
            }
            Event::Message(ServerMessage::Saved) => {
                if network.mode != Mode::Menu {
                    network.disconnect();
                }
                network.status = "Персонаж сохранён. Можно войти снова".into();
            }
            Event::Closed(reason) => {
                if matches!(
                    network.mode,
                    Mode::Online | Mode::Connecting | Mode::Selection
                ) {
                    network.disconnect();
                    network.status = format!("{reason}. Войдите снова");
                }
            }
            _ => {}
        }
    }
    if network.mode != Mode::Online {
        for (entity, ..) in &remotes {
            commands.entity(entity).despawn();
        }
        return;
    }
    if let Some(actor) = &network.authoritative {
        let target = Vec3::from_array(actor.position);
        let alpha = 1. - (-time.delta_secs() * 25.).exp();
        let next = hero.translation.lerp(target, alpha);
        let previous_position = hero.translation;
        // Never interpolate a diagonal through a corner: snap to the validated point instead.
        hero.translation = if world
            .0
            .segment_clear([hero.translation.x, hero.translation.z], [next.x, next.z])
        {
            next
        } else {
            target
        };
        movement.travelled += previous_position.distance(hero.translation);
        hero.translation.y = world
            .0
            .surface_height(hero.translation.x, hero.translation.z);
        hero.rotation = hero.rotation.slerp(Quat::from_rotation_y(actor.yaw), alpha);
    }
    let own = network.character.as_ref().map(|p| p.id);
    let ids: HashSet<_> = network.roster.iter().map(|p| p.id).collect();
    let existing: HashSet<_> = remotes.iter().map(|(_, p, _)| p.id).collect();
    for (entity, mut remote, mut transform) in &mut remotes {
        if !ids.contains(&remote.id) {
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(a) = network.roster.iter().find(|a| a.id == remote.id) {
            remote.target = Vec3::from_array(a.position);
            remote.yaw = a.yaw;
        }
        let next = transform
            .translation
            .lerp(remote.target, 1. - (-time.delta_secs() * 20.).exp());
        transform.translation = if world.0.segment_clear(
            [transform.translation.x, transform.translation.z],
            [next.x, next.z],
        ) {
            next
        } else {
            remote.target
        };
        transform.translation.y = world
            .0
            .surface_height(transform.translation.x, transform.translation.z);
        transform.rotation = Quat::from_rotation_y(remote.yaw);
    }
    for actor in &network.roster {
        if Some(actor.id) == own || existing.contains(&actor.id) {
            continue;
        }
        commands
            .spawn((
                Remote {
                    id: actor.id,
                    target: Vec3::from_array(actor.position),
                    yaw: actor.yaw,
                },
                AnimatedActor::default(),
                Transform::from_translation(Vec3::from_array(actor.position))
                    .with_rotation(Quat::from_rotation_y(actor.yaw)),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((
                    SceneRoot(
                        assets
                            .load(GltfAssetLabel::Scene(0).from_asset("characters/adventurer.glb")),
                    ),
                    crate::appearance::Wearer::Remote(actor.id),
                ));
            });
    }
}
