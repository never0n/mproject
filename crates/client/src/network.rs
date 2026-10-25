//! Client networking. `Network` (a Bevy resource) owns the WebSocket connection, run on a separate
//! OS thread with its own Tokio runtime, and stores the latest server state (own actor, players,
//! monsters, chat/party…). UI modules send requests through `Network` helper methods;
//! `receive` applies server replies once per frame and moves the hero and other players' models.
use crate::{Movement, Player, Terrain, character::AnimatedActor};
use bevy::prelude::*;
use futures_util::{SinkExt, StreamExt};
use mortown_protocol::{
    Actor, Character, ClientMessage, PROTOCOL_VERSION, ServerMessage, VERSION_MISMATCH,
    WORLD_VERSION,
};
use std::{
    collections::HashSet,
    sync::{Mutex, mpsc},
    time::Duration,
};
use tokio_tungstenite::tungstenite::Message;

/// Sends one frame with a 2 s timeout so a stalled socket can't hang the network thread.
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

/// Connection state: `Menu` (logged out) → `Connecting` → `Selection` (character list) → `Online`.
/// `Offline` is only the local visual preview (`--view`), with no server.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Menu,
    Offline,
    Connecting,
    Online,
    Selection,
}
/// What the socket thread passes to `receive`. A plain enum, not a Bevy Event.
#[allow(clippy::large_enum_variant)]
enum Event {
    Message(ServerMessage),
    Closed(String),
}
/// Connection plus the client's copy of server state; most UI `display` systems read from here.
/// Never set game results here directly: change them by sending a request and wait for the reply.
#[derive(Resource)]
pub struct Network {
    pub mode: Mode,
    /// Last status/error line shown to the player.
    pub status: String,
    /// Own persistent sheet (stats and carried stacks), refreshed from server replies.
    pub character: Option<Character>,
    pub characters: Vec<Character>,
    pub selected: Option<i64>,
    /// A lobby/economy `request` is in flight; blocks sending another until the reply.
    pub pending: bool,
    /// Bumped on each new character list so the lobby knows to rebuild it.
    pub selection_revision: u64,
    pub registered: bool,
    /// Own live actor as last reported by the server (position, hp, cooldowns…).
    pub authoritative: Option<Actor>,
    /// All players in the latest snapshot, including yourself.
    pub roster: Vec<Actor>,
    pub monsters: Vec<mortown_protocol::Monster>,
    pub areas: Vec<mortown_protocol::skills::Area>,
    pub alliance: mortown_protocol::alliance::Reply,
    pub alliance_pending: bool,
    pub party: mortown_protocol::party::Reply,
    pub party_pending: bool,
    pub party_received_at: f64,
    pub chat: crate::chat::Log,
    pub exchange: Option<mortown_protocol::exchange::View>,
    pub exchange_status: String,
    pub exchange_pending: bool,
    /// Containers and ground stacks near the character (`ServerMessage::Objects`).
    pub containers: Vec<mortown_protocol::objects::Container>,
    pub nodes: Vec<mortown_protocol::work::Node>,
    pub plots: Vec<mortown_protocol::work::Plot>,
    pub blocks: Vec<mortown_protocol::work::Block>,
    pub ground: Vec<mortown_protocol::objects::Loose>,
    /// Fill of castings on the ground. The mesh is built from these cells.
    pub castings: Vec<mortown_protocol::objects::CastingBody>,
    /// The chest opened with `objects::Command::Open`, as last committed by the server.
    pub container: Option<mortown_protocol::objects::View>,
    pub object_pending: bool,
    /// Last foundry reply. The drawing on screen is local until the server accepts it.
    pub casting: Option<mortown_protocol::casting::View>,
    pub address: String,
    /// Outgoing commands to the socket thread.
    tx: Option<tokio::sync::mpsc::Sender<ClientMessage>>,
    // Mutex only because Bevy resources must be `Sync` and std `Receiver` is not.
    rx: Option<Mutex<mpsc::Receiver<Event>>>,
    /// Increasing `Move` counter; the server drops older ones.
    seq: u64,
    /// Time of the last `Move`, for rate limiting in `intent`.
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
            areas: vec![],
            alliance: default(),
            alliance_pending: false,
            party: default(),
            party_pending: false,
            party_received_at: 0.,
            chat: default(),
            exchange: None,
            exchange_status: String::new(),
            exchange_pending: false,
            containers: vec![],
            nodes: vec![],
            plots: vec![],
            blocks: vec![],
            ground: vec![],
            castings: vec![],
            container: None,
            object_pending: false,
            casting: None,
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
    /// Opens a new connection and sends the first message (`Login` or `Register`).
    /// Spawns a thread that owns the socket until logout, error or `Saved`/`Registered`.
    pub fn begin(&mut self, message: ClientMessage) {
        self.disconnect();
        self.mode = Mode::Connecting;
        self.registered = false;
        self.status = "Соединяемся с миром…".into();
        // Bounded queues: 64 incoming frames before the client is considered too slow; 8 outgoing.
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
                    // Wait for whichever comes first: liveness check, server frame, or outgoing command.
                    // 15 s without any frame (server pings every 5 s) means the connection is dead.
                    loop { tokio::select! {
                        _=heartbeat.tick()=> { if last_received.elapsed()>Duration::from_secs(15) { return Err("Сервер перестал отвечать"); } }
                        incoming=socket.next() => {
                            last_received=tokio::time::Instant::now();
                            match incoming {
                                Some(Ok(Message::Text(text))) => {
                                    let message: ServerMessage=serde_json::from_str(&text).map_err(|_| "Несовместимый ответ сервера")?;
                                    // These two end the session: the server closes after sending them.
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
                            // Sender dropped (resource reset) counts as logout so the server saves.
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
    /// Asks the server to log out (it saves the character) and clears all cached world state.
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
        self.areas.clear();
        self.alliance = default();
        self.alliance_pending = false;
        self.party = default();
        self.party_pending = false;
        self.chat = default();
        self.exchange = None;
        self.exchange_status.clear();
        self.exchange_pending = false;
        self.containers.clear();
        self.nodes.clear();
        self.plots.clear();
        self.blocks.clear();
        self.ground.clear();
        self.container = None;
        self.object_pending = false;
    }
    /// Subsystem requests (alliance/party/chat/exchange): one in flight per subsystem,
    /// tracked by its own `*_pending` flag that `receive` clears when the reply arrives.
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
    pub fn society_request(&mut self, command: mortown_protocol::society::Command) {
        if self.mode != Mode::Online {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(ClientMessage::Society { command });
        }
    }
    pub fn voice(&mut self, speaking: bool) {
        if self.mode != Mode::Online {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(ClientMessage::Voice { speaking });
        }
    }
    pub fn cast_request(&mut self, command: mortown_protocol::casting::Command) {
        if self.mode != Mode::Online || self.object_pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Cast { command }).is_ok())
        {
            self.object_pending = true;
        } else {
            self.status = "Соединение недоступно".into();
        }
    }
    pub fn work_request(&mut self, command: mortown_protocol::work::Command) {
        if self.mode != Mode::Online || self.object_pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Work { command }).is_ok())
        {
            self.object_pending = true;
        } else {
            self.status = "Соединение недоступно".into();
        }
    }
    pub fn object_request(&mut self, command: mortown_protocol::objects::Command) {
        if self.mode != Mode::Online || self.object_pending {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(ClientMessage::Objects { command }).is_ok())
        {
            self.object_pending = true;
        } else {
            self.status = "Соединение недоступно".into();
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
    /// Lobby/economy request (craft, equip, potion…). Sets `pending` until the server answers,
    /// so double clicks can't send duplicates.
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
    /// Fire-and-forget in-world action (attack, skill, respawn); results come via snapshots.
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
    /// Movement wish: "walk toward `target` [x, z]". Sent at most every 45 ms (~22/s), slightly
    /// faster than the 50 ms server tick; extra calls are silently dropped.
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
    /// Admin click-teleport; the server rejects it without the `click_teleport` power.
    pub fn teleport(&mut self, target: [f32; 2]) {
        if self.mode != Mode::Online {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(ClientMessage::Teleport { target });
        }
    }
    /// True when the 3D world is active (online or offline preview), i.e. not in menus/lobby.
    pub fn playing(&self) -> bool {
        matches!(self.mode, Mode::Offline | Mode::Online)
    }
}
/// Another player's model; `target`/`yaw` are the latest server values it is smoothed toward.
#[derive(Component)]
pub struct Remote {
    pub id: i64,
    target: Vec3,
    yaw: f32,
}
/// Runs early in `Update`: drains all queued server messages into `Network`, then smoothly moves
/// the hero to the server position and spawns/moves/despawns `Remote` players.
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
                // `response` = answer to our own request (vs. a broadcast refresh).
                if reply.response {
                    network.alliance_pending = false;
                }
                // A broadcast refresh has no text; keep the last message for the same alliance.
                if reply.message.is_empty()
                    && reply.alliance.as_ref().map(|a| a.id)
                        == network.alliance.alliance.as_ref().map(|a| a.id)
                {
                    reply.message = network.alliance.message.clone();
                }
                network.alliance = reply;
            }
            Event::Message(ServerMessage::Hearing { label, .. })
                if network.mode == Mode::Online =>
            {
                network.status = format!("{label} говорит рядом");
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
            Event::Message(ServerMessage::Cast { view }) if network.mode == Mode::Online => {
                if let Some(update) = view.update.as_ref()
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| c.id == update.character.id)
                {
                    network.character = Some(update.character.clone());
                    network.authoritative = Some(update.actor.clone());
                }
                network.status = view.message.clone();
                network.casting = Some(view);
                network.object_pending = false;
            }
            Event::Message(ServerMessage::Objects {
                containers,
                ground,
                nodes,
                plots,
                blocks,
                castings,
            }) if network.mode == Mode::Online => {
                // An open chest that is no longer there (lifted by someone) closes.
                if let Some(open) = network.container.as_ref()
                    && !containers.iter().any(|c| c.id == open.container.id)
                {
                    network.container = None;
                }
                network.containers = containers;
                network.ground = ground;
                network.castings = castings;
                network.nodes = nodes;
                network.plots = plots;
                network.blocks = blocks;
            }
            Event::Message(ServerMessage::Object { reply }) if network.mode == Mode::Online => {
                if let Some(update) = reply.update
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| c.id == update.character.id)
                {
                    network.character = Some(update.character);
                    network.authoritative = Some(update.actor);
                }
                if reply.view.is_some() {
                    network.container = reply.view;
                }
                network.status = reply.message;
                network.object_pending = false;
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
                    // The DB sheet may lag behind live progress: re-apply level growth, XP, loot
                    // and current resources from the live actor unless the sheet is newer.
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
                // Different message format or world geometry would desync movement/collision.
                if version != PROTOCOL_VERSION || version_world != WORLD_VERSION {
                    network.disconnect();
                    network.status = VERSION_MISMATCH.into();
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
                    // Same live-progress merge as in `CharacterUpdated`, so HUD/stats track every tick.
                    if let Some(c) = network.character.as_mut() {
                        if a.progress.kills >= c.stats.get("kills").copied().unwrap_or(0.) as u64 {
                            let old_level = c.stats["level"] as u32;
                            mortown_protocol::progression::grow(
                                &mut c.stats,
                                a.progress.level.saturating_sub(old_level),
                            );
                            a.progress.write_stats(&mut c.stats);
                        }
                        c.stats.insert("current_hp".into(), a.hp);
                        c.stats.insert("current_mana".into(), a.mana);
                        c.stats.insert("current_stamina".into(), a.stamina);
                    }
                }
                network.monsters = monsters;
                network.areas = areas;
                network.roster = actors;
            }
            Event::Message(ServerMessage::Error { message, fatal }) => {
                network.status = message;
                network.pending = false;
                network.object_pending = false;
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
        // Exponential smoothing toward the server position; rate 25/s ≈ catches up in ~0.1 s.
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
    // Sync `Remote` entities with the roster: despawn leavers, smooth existing, spawn newcomers.
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
        // Slightly softer than the hero (20 vs 25); same corner-snap rule as above.
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
            // The glTF model is a child so animation/equipment systems can find it via `Wearer`.
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
