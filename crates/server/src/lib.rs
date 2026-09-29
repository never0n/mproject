mod alliance;
mod alliance_storage;
mod chat;
pub mod combat;
mod console;
pub mod economy;
mod exchange;
pub mod mail;
mod party;
pub mod simulation;
pub mod store;

use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use mortown_protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage, WORLD_VERSION};
use simulation::Simulation;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore, watch};

#[derive(Clone)]
pub struct Server {
    pub pool: PgPool,
    pub simulation: Arc<Mutex<Simulation>>,
    chat: tokio::sync::broadcast::Sender<chat::Delivery>,
    alliance_events: watch::Sender<()>,
    admin_events: watch::Sender<()>,
    announcements: tokio::sync::broadcast::Sender<String>,
    snapshots: watch::Receiver<ServerMessage>,
    shutdown: watch::Receiver<bool>,
    connections: Arc<Semaphore>,
    auth: Arc<Semaphore>,
}
impl Server {
    pub async fn new(
        database_url: &str,
        shutdown: watch::Receiver<bool>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = PgPoolOptions::new()
            .max_connections(72)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let terrain = Arc::new(mortown_world::World::new());
        let simulation = Arc::new(Mutex::new(Simulation::new(terrain)));
        let (tx, snapshots) = watch::channel(ServerMessage::Snapshot {
            tick: 0,
            actors: vec![],
            monsters: vec![],
            residents: simulation.lock().await.residents.clone(),
            areas: vec![],
        });
        let sim = simulation.clone();
        let mut stop = shutdown.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(50));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = stop.changed() => break,
                    _ = tick.tick() => {
                        let mut s = sim.lock().await; s.update();
                        tx.send_replace(ServerMessage::Snapshot { tick: s.tick(), actors: s.snapshot(), monsters:s.monsters(), residents:s.residents.clone(), areas: s.areas() });
                    }
                }
            }
        });
        let (chat, _) = tokio::sync::broadcast::channel(256);
        let (alliance_events, _) = watch::channel(());
        let (admin_events, _) = watch::channel(());
        let (announcements, _) = tokio::sync::broadcast::channel(32);
        Ok(Self {
            admin_events,
            announcements,
            alliance_events,
            chat,
            pool,
            simulation,
            snapshots,
            shutdown,
            connections: Arc::new(Semaphore::new(64)),
            auth: Arc::new(Semaphore::new(4)),
        })
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route("/health", get(|| async { "mortown-56" }))
            .route("/world", get(upgrade))
            .with_state(self.clone())
    }
}
async fn upgrade(
    ws: WebSocketUpgrade,
    State(server): State<Server>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
) -> impl IntoResponse {
    let ip = peer.map(|axum::Extension(axum::extract::ConnectInfo(addr))| {
        addr.ip().to_canonical().to_string()
    });
    ws.max_message_size(8192)
        .max_frame_size(8192)
        .on_upgrade(move |socket| session(socket, server, ip))
}
const PEER_TIMEOUT: Duration = Duration::from_secs(20);
async fn send_frame(socket: &mut WebSocket, frame: Message) -> Result<(), ()> {
    tokio::time::timeout(Duration::from_secs(2), socket.send(frame))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}
async fn send(socket: &mut WebSocket, message: &ServerMessage) -> Result<(), ()> {
    send_frame(
        socket,
        Message::Text(serde_json::to_string(message).map_err(|_| ())?.into()),
    )
    .await
}
async fn error(socket: &mut WebSocket, message: impl Into<String>) {
    let _ = send(
        socket,
        &ServerMessage::Error {
            message: message.into(),
            fatal: true,
        },
    )
    .await;
}
async fn session(mut socket: WebSocket, server: Server, ip: Option<String>) {
    let Ok(_connection) = server.connections.clone().try_acquire_owned() else {
        error(&mut socket, "Сервер заполнен").await;
        return;
    };
    let first = tokio::time::timeout(Duration::from_secs(15), socket.recv()).await;
    let Ok(Some(Ok(Message::Text(first)))) = first else {
        return;
    };
    let Ok(message) = serde_json::from_str::<ClientMessage>(&first) else {
        error(&mut socket, "Неверный формат входа").await;
        return;
    };
    let Ok(auth_permit) = server.auth.clone().try_acquire_owned() else {
        error(
            &mut socket,
            "Сервер занят входами, повторите через несколько секунд",
        )
        .await;
        return;
    };
    let version_ok = match &message {
        ClientMessage::Login { version, world, .. }
        | ClientMessage::Register { version, world, .. } => {
            *version == PROTOCOL_VERSION && world == WORLD_VERSION
        }
        _ => false,
    };
    if !version_ok {
        error(&mut socket, "Обновите клиент: версия мира не совпадает").await;
        return;
    }
    let account = match message {
        ClientMessage::Login {
            login, password, ..
        } => store::login(&server.pool, login, password).await,
        ClientMessage::Register {
            login, password, ..
        } => {
            match store::register(&server.pool, store::Registration { login, password }).await {
                Ok(_) => {
                    let _ = send(&mut socket, &ServerMessage::Registered).await;
                }
                Err(e) => error(&mut socket, e).await,
            }
            let _ = send_frame(&mut socket, Message::Close(None)).await;
            return;
        }
        _ => unreachable!(),
    };
    drop(auth_permit);
    let account = match account {
        Ok(id) => id,
        Err(e) => {
            error(&mut socket, e).await;
            return;
        }
    };
    let Ok(mut conn) = server.pool.acquire().await else {
        error(&mut socket, "База данных недоступна").await;
        return;
    };
    // A dedicated connection owns the lease until final save, including across server processes.
    let locked = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
        .bind(account)
        .fetch_one(&mut *conn)
        .await
        .unwrap_or(false);
    if !locked {
        error(&mut socket, "Этот аккаунт уже подключён").await;
        return;
    }
    let result = match console::access(&mut conn, account, ip).await {
        Ok(access) => character_lobby(&mut socket, &server, &mut conn, account, &access).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        error(&mut socket, e).await;
    }
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(account)
        .execute(&mut *conn)
        .await;
    // Do not return a connection with an uncertain advisory lock to the pool.
    let _ = conn.close().await;
}
async fn character_list(
    socket: &mut WebSocket,
    conn: &mut sqlx::PgConnection,
    account: i64,
    selected: Option<i64>,
) -> Result<(), String> {
    let characters = store::characters(conn, account).await?;
    send(
        socket,
        &ServerMessage::Characters {
            characters,
            selected,
            limit: mortown_protocol::CHARACTER_LIMIT,
        },
    )
    .await
    .map_err(|_| "Соединение закрыто".into())
}
async fn character_lobby(
    socket: &mut WebSocket,
    server: &Server,
    conn: &mut sqlx::PgConnection,
    account: i64,
    access: &console::Access,
) -> Result<(), String> {
    let mut admin_events = server.admin_events.subscribe();
    character_list(socket, conn, account, None).await?;
    let mut shutdown = server.shutdown.clone();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_received = Instant::now();
    let mut budget_start = Instant::now();
    let mut received = 0;
    loop {
        if *shutdown.borrow() {
            break;
        }
        let incoming = tokio::select! {
            _=shutdown.changed()=>break,
            _=admin_events.changed()=> {console::check_access(conn,access).await?;continue;},
            _=heartbeat.tick()=> {
                console::check_access(conn,access).await?;
                if last_received.elapsed() >= PEER_TIMEOUT { break; }
                if send_frame(socket, Message::Ping(Vec::new().into())).await.is_err() {break;}
                continue;
            },
            message=socket.recv()=>message,
        };
        let Some(Ok(message)) = incoming else {
            break;
        };
        last_received = Instant::now();
        if budget_start.elapsed() >= Duration::from_secs(1) {
            budget_start = Instant::now();
            received = 0;
        }
        received += 1;
        if received > 100 {
            return Err("Слишком много команд".into());
        }
        let result = match message {
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::CreateCharacter {
                    nickname,
                    race,
                    class,
                    gender,
                }) => {
                    match store::create_character(
                        conn,
                        account,
                        store::CharacterCreation {
                            nickname,
                            race,
                            class,
                            gender,
                        },
                    )
                    .await
                    {
                        Ok(id) => character_list(socket, conn, account, Some(id)).await,
                        Err(e) => Err(e),
                    }
                }
                Ok(ClientMessage::SelectCharacter { character_id }) => {
                    match store::load(conn, account, character_id).await {
                        Ok(data) => {
                            if !play_character(socket, server, conn, data, access).await? {
                                return Ok(());
                            }
                            last_received = Instant::now();
                            character_list(socket, conn, account, Some(character_id)).await
                        }
                        Err(e) => Err(e),
                    }
                }
                Ok(ClientMessage::Logout) => break,
                _ => Err("Сначала выберите персонажа".into()),
            },
            Message::Close(_) => break,
            Message::Ping(bytes) => {
                if send_frame(socket, Message::Pong(bytes)).await.is_err() {
                    break;
                }
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(message) = result {
            send(
                socket,
                &ServerMessage::Error {
                    message,
                    fatal: false,
                },
            )
            .await
            .map_err(|_| "Соединение закрыто")?;
        }
    }
    let _ = send_frame(socket, Message::Close(None)).await;
    Ok(())
}
async fn play_character(
    socket: &mut WebSocket,
    server: &Server,
    conn: &mut sqlx::PgConnection,
    data: (mortown_protocol::Character, [f32; 2], f32),
    access: &console::Access,
) -> Result<bool, String> {
    if *server.shutdown.borrow() {
        return Ok(false);
    }
    let (character, pos, yaw) = data;
    let mut chat_state = chat::Session::load(conn, character.id).await?;
    let mut chat_events = server.chat.subscribe();
    let mut announcements = server.announcements.subscribe();
    let mut admin_events = server.admin_events.subscribe();
    let mut alliance_events = server.alliance_events.subscribe();
    let id = character.id;
    let actor = {
        let mut simulation = server.simulation.lock().await;
        simulation.join(id, character.nickname.clone(), pos, yaw)?;
        // Restore membership only after the actor has its saved stats, before the next tick.
        simulation.configure(&character)
    };
    let mut saved_kills = actor.progress.kills;
    let welcome = ServerMessage::Welcome {
        version: PROTOCOL_VERSION,
        world: WORLD_VERSION.into(),
        character,
        actor,
    };
    let mut snapshots = server.snapshots.clone();
    snapshots.borrow_and_update(); // Ignore snapshots from before this actor joined.
    let mut shutdown = server.shutdown.clone();
    let mut exchange_tick = tokio::time::interval(Duration::from_millis(250));
    exchange_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_received = Instant::now();
    let mut save_tick = tokio::time::interval(Duration::from_secs(5));
    save_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut budget_start = Instant::now();
    let mut received = 0;
    let mut mail_received = 0;
    let mut chat_received = 0;
    let mut last_unread = None;
    let mut return_to_selection = false;
    let mut uncertain_inventory = false;
    if send(socket, &welcome).await.is_ok()
        && send(
            socket,
            &ServerMessage::Chat {
                reply: chat_state.preferences(),
            },
        )
        .await
        .is_ok()
    {
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = heartbeat.tick() => {
                    if let Err(e)=console::check_access(conn,access).await {error(socket,e).await;break;}
                    if last_received.elapsed() >= PEER_TIMEOUT { break; }
                    if send_frame(socket, Message::Ping(Vec::new().into())).await.is_err() {break;}
                },
                _ = admin_events.changed() => {
                    if let Err(e)=console::check_access(conn,access).await {error(socket,e).await;break;}
                    let simulation=server.simulation.lock().await;
                    let update=store::load(conn,access.account,id).await.map(|(mut character,_,_)|{
                        let actor=simulation.actor(id).unwrap();
                        actor.progress.write_stats(&mut character.stats);
                        character.loot=actor.loot.clone();
                        mortown_protocol::items::include_loot(&mut character.inventory,&character.loot);
                        ServerMessage::EconomyUpdated{character,actor}
                    });
                    snapshots.borrow_and_update();
                    drop(simulation);
                    match update {Ok(reply)=>{if send(socket,&reply).await.is_err(){break;}},Err(e)=>{error(socket,e).await;break;}}

                },
                _ = alliance_events.changed() => {
                    let actors=server.simulation.lock().await.snapshot();
                    match alliance::view(conn,id,&actors).await {
                        Ok(reply)=>{if send(socket,&ServerMessage::Alliance{reply}).await.is_err(){break;}},
                        Err(message)=>{error(socket,message).await;break;}
                    }
                },
                message=announcements.recv()=>{if let Ok(message)=message && send(socket,&ServerMessage::Chat{reply:mortown_protocol::chat::Reply::Announcement{message}}).await.is_err(){break;}},
                delivery=chat_events.recv()=>{
                    match delivery {
                        Ok(event) if event.recipients.contains(&id) && !chat_state.ignore_all && !chat_state.ignored.contains(&event.message.sender) && (!chat_state.silence || event.message.channel!=mortown_protocol::chat::Channel::Private || event.message.sender==id)=>{
                            chat_state.delivered(event.message.id);
                            if send(socket,&ServerMessage::Chat{reply:mortown_protocol::chat::Reply::Message{message:event.message}}).await.is_err(){break;}
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{
                            if send(socket,&ServerMessage::Chat{reply:mortown_protocol::chat::Reply::Status{message:"Часть сообщений пропущена из-за задержки соединения".into()}}).await.is_err(){break;}
                        },
                        _=>{},
                    }
                },
                _ = exchange_tick.tick() => {
                    let mut simulation=server.simulation.lock().await;
                    let actors=simulation.snapshot();
                    let party_reply=simulation.parties.poll(id,&actors);
                    let result=exchange::poll(&mut simulation,conn,id).await;
                    if matches!(&result,Ok(Some(reply)) if reply.update.is_some()) { snapshots.borrow_and_update(); }
                    drop(simulation);
                    if let Some(reply)=party_reply && send(socket,&ServerMessage::Party{reply}).await.is_err(){break;}
                    match result {
                        Ok(Some(reply)) => { if send(socket,&ServerMessage::Exchange{reply}).await.is_err(){break;} },
                        Err(message) => { error(socket,message).await; break; },
                        _=>{},
                    }
                },
                _ = save_tick.tick() => {
                    let mut simulation = server.simulation.lock().await;
                    let actor = simulation.actor(id).unwrap();
                    match mail::settle(conn, actor).await {
                        Ok(actor) => {
                            saved_kills = actor.progress.kills;
                            simulation.replace_actor(actor);
                            snapshots.borrow_and_update();
                        }
                        Err(e) => { drop(simulation); error(socket,e).await; break; }
                    }
                    drop(simulation);
                    let actors=server.simulation.lock().await.snapshot();
                    match alliance::view(conn,id,&actors).await {
                        Ok(reply)=>{if send(socket,&ServerMessage::Alliance{reply}).await.is_err(){break;}},
                        Err(message)=>{error(socket,message).await;break;}
                    }
                    if let Ok(count) = mail::unread(conn, id).await && last_unread != Some(count) {
                        last_unread = Some(count);
                        if send(socket, &ServerMessage::Mail { reply: mortown_protocol::mail::Reply::Unread { count } }).await.is_err() { break; }
                    }
                }
                changed = snapshots.changed() => {
                    if changed.is_err() { break; }
                    let mut snapshot = snapshots.borrow_and_update().clone();
                    let simulation=server.simulation.lock().await;
                    // Cross-player exchange may have updated this actor after snapshot generation.
                    let current=simulation.actor(id).unwrap();
                    if current.progress.kills != saved_kills {
                        if let Err(e)=store::save(conn,&current).await { drop(simulation); error(socket,e).await; break; }
                        saved_kills=current.progress.kills;
                    }
                    if let ServerMessage::Snapshot{actors,..}=&mut snapshot {
                        actors.retain(|a| a.id == id || !a.powers.invisible);
                        for actor in actors.iter_mut() {
                            if actor.id != id {
                                actor.powers = Default::default();
                            }
                        }
                        if let Some(actor)=actors.iter_mut().find(|a|a.id==id){*actor=current;}
                    }
                    drop(simulation);
                    if send(socket,&snapshot).await.is_err() { break; }
                }
                incoming = socket.recv() => {
                    let Some(Ok(message)) = incoming else { break; };
                    last_received=Instant::now();
                    if budget_start.elapsed() >= Duration::from_secs(1) { budget_start=Instant::now(); received=0; mail_received=0; chat_received=0; }
                    received+=1; if received > 100 { error(socket,"Слишком много команд").await; break; }
                    match message {
                        Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Alliance{command})=>{
                                chat_received+=1;
                                let mut simulation=server.simulation.lock().await;
                                let refresh=matches!(command,mortown_protocol::alliance::Command::Refresh);
                                let result=if chat_received>4 {Err("Слишком много действий с альянсом".into())} else {
                                    if matches!(command,mortown_protocol::alliance::Command::Create{..} | mortown_protocol::alliance::Command::Deposit{..} | mortown_protocol::alliance::Command::Withdraw{..}) {
                                        simulation.exchanges.cancel(id,"Обмен отменён: операция с альянсом меняет инвентарь");
                                    }
                                    let actor=simulation.actor(id).unwrap();
                                    alliance::apply(conn,actor,command).await
                                };
                                let (message,update)=match result {
                                    Ok(change)=>{
                                        if let Some((_,actor))=&change.update {
                                            simulation.replace_actor(actor.clone());
                                            saved_kills=actor.progress.kills;
                                            snapshots.borrow_and_update();
                                        }
                                        if !refresh {server.alliance_events.send_replace(());}
                                        (change.message,change.update)
                                    }
                                    Err(alliance::Failure::Rejected(message))=>(message,None),
                                    Err(alliance::Failure::UncertainInventory)=>{
                                        uncertain_inventory=true;
                                        drop(simulation);
                                        error(socket,"Не удалось подтвердить операцию с имуществом альянса. Войдите снова, чтобы загрузить сохранённое состояние").await;
                                        break;
                                    }
                                };
                                let actors=simulation.snapshot();
                                drop(simulation);
                                if let Some((character,actor))=update && send(socket,&ServerMessage::EconomyUpdated{character,actor}).await.is_err(){break;}
                                match alliance::view(conn,id,&actors).await {
                                    Ok(mut reply)=>{reply.message=message;reply.response=true;if send(socket,&ServerMessage::Alliance{reply}).await.is_err(){break;}},
                                    Err(message)=>{error(socket,message).await;break;}
                                }
                            }
                            Ok(ClientMessage::Party{command})=>{
                                chat_received+=1;
                                if chat_received>4{error(socket,"Слишком много действий с группой").await;break;}
                                let blocked=if let mortown_protocol::party::Command::Invite{player}=&command {
                                    sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM chat_ignores WHERE owner_id=$1 AND blocked_id=$2)").bind(player).bind(id).fetch_one(&mut *conn).await.unwrap_or(true)
                                }else{false};
                                let mut sim=server.simulation.lock().await;
                                let actors=sim.snapshot();
                                if blocked {
                                    let mut reply=sim.parties.view(id,&actors,"Приглашение недоступно".into());
                                    reply.response=true;
                                    drop(sim);
                                    if send(socket,&ServerMessage::Party{reply}).await.is_err(){break;}
                                }else{sim.parties.handle(id,&actors,command);}
                            }
                            Ok(ClientMessage::Chat{command})=>{
                                chat_received+=1;
                                let reply=if chat_received>4 {Some(mortown_protocol::chat::Reply::Status{message:"Слишком много действий в чате".into()})} else {chat::handle(server,conn,id,&mut chat_state,command).await};
                                if matches!(reply,Some(mortown_protocol::chat::Reply::Logout)){break;}
                                if let Some(reply)=reply && send(socket,&ServerMessage::Chat{reply}).await.is_err(){break;}
                                if send(socket,&ServerMessage::Chat{reply:chat_state.preferences()}).await.is_err(){break;}
                            }
                            Ok(ClientMessage::Exchange { command }) => {
                                mail_received+=1;
                                if mail_received>8 { error(socket,"Слишком много запросов обмена").await; break; }
                                let mut simulation=server.simulation.lock().await;
                                exchange::handle(&mut simulation,conn,id,command).await;
                            }
                            Ok(ClientMessage::Mail { command }) => {
                                mail_received += 1;
                                let reply = if mail_received > 8 {
                                    mortown_protocol::mail::Reply::Error { message: "Слишком много почтовых действий. Подождите секунду".into() }
                                } else if command.changes_inventory() {
                                    let mut simulation=server.simulation.lock().await;
                                    simulation.exchanges.cancel(id,"Обмен отменён: инвентарь изменяется через почту");
                                    let actor=simulation.actor(id).unwrap();
                                    let reply=mail::handle(conn,id,command,Some(actor)).await;
                                    if let Some(update)=reply.inventory_update() {
                                        simulation.replace_actor(update.actor.clone());
                                        simulation.refresh_character(&update.character);
                                        saved_kills=update.actor.progress.kills;
                                        snapshots.borrow_and_update();
                                    }
                                    reply
                                } else {
                                    mail::handle(conn, id, command, None).await
                                };
                                if send(socket, &ServerMessage::Mail { reply }).await.is_err() { break; }
                            }
                            Ok(ClientMessage::Move { seq,target,walking }) => {
                                let result = server.simulation.lock().await.input(id,seq,target,walking);
                                if let Err(e) = result { error(socket,e).await; break; }
                            }
                            Ok(ClientMessage::Teleport { target }) => {
                                let result = server.simulation.lock().await.click_teleport(id, target);
                                if let Err(message) = result && send(socket, &ServerMessage::Error { message: message.into(), fatal: false }).await.is_err() {
                                    break;
                                }
                            }
                            Ok(ClientMessage::SetEquipment {item,equipped}) => {
                                let mut simulation=server.simulation.lock().await;
                                simulation.exchanges.cancel(id,"Обмен отменён: экипировка изменяется");
                                let actor = simulation.actor(id).unwrap();
                                if let Err(e) = store::save(conn, &actor).await { drop(simulation); error(socket,e).await; break; }
                                let reply=match store::set_equipment(conn,id,item,equipped).await {
                                    Ok(character)=>{simulation.refresh_character(&character);ServerMessage::CharacterUpdated {character}},
                                    Err(message)=>ServerMessage::Error {message,fatal:false},
                                };
                                drop(simulation);
                                if send(socket,&reply).await.is_err(){break;}
                            }
                            Ok(command @ (ClientMessage::Specialize { .. } | ClientMessage::Craft { .. } | ClientMessage::Trade { .. } | ClientMessage::UsePotion | ClientMessage::Quest { .. } | ClientMessage::LearnSkill { .. } | ClientMessage::LearnSecondary | ClientMessage::LearnAbility { .. } | ClientMessage::SpendAttribute { .. } | ClientMessage::BindHotbar { .. } | ClientMessage::Arrange { .. })) => {
                                let action = match command { ClientMessage::LearnAbility { skill } => economy::Action::LearnAbility { skill }, ClientMessage::Specialize { profession, expected } => economy::Action::Specialize { profession, expected }, ClientMessage::Craft { recipe, expected, specialization_revision } => economy::Action::Craft { recipe, expected, specialization_revision }, ClientMessage::Arrange { source, target, expected } => economy::Action::Arrange { source,target,expected }, ClientMessage::BindHotbar { slot, binding } => economy::Action::BindHotbar { slot, binding }, ClientMessage::Trade { npc, item, buy, sale } => economy::Action::Trade { npc, item, buy, sale }, ClientMessage::Quest { quest, complete } => economy::Action::Quest { quest, complete }, ClientMessage::LearnSkill { rank } => economy::Action::LearnSkill { rank }, ClientMessage::LearnSecondary => economy::Action::LearnSecondary, ClientMessage::SpendAttribute { stat, expected } => economy::Action::SpendAttribute { stat, expected }, _ => economy::Action::Potion };
                                let mut simulation = server.simulation.lock().await;
                                simulation.exchanges.cancel(id,"Обмен отменён: инвентарь или характеристики изменяются");
                                let actor = simulation.actor(id).unwrap();
                                let world = simulation.terrain();
                                let resident=if let economy::Action::Trade{npc,..}=&action{simulation.resident(*npc)}else{None};
                                let invalid=(matches!(&action,economy::Action::Trade{..}) && resident.is_none()) || (matches!(&action,economy::Action::Quest{..}) && simulation.resident(mortown_world::npcs::PLAZA_TRADER).is_none());
                                let result=if invalid{Err(store::ExchangeFailure::Rejected("NPC недоступен".into()))}else{store::exchange_at(conn, actor, action, &world,resident).await};
                                let reply = match result {
                                    Ok((character, actor)) => {
                                        simulation.replace_actor(actor.clone());
                                        simulation.refresh_character(&character);
                                        saved_kills = actor.progress.kills;
                                        snapshots.borrow_and_update();
                                        ServerMessage::EconomyUpdated { character, actor }
                                    },
                                    Err(store::ExchangeFailure::Rejected(message)) => ServerMessage::Error { message, fatal: false },
                                    Err(store::ExchangeFailure::Uncertain) => {
                                        uncertain_inventory = true;
                                        drop(simulation);
                                        error(socket, "Не удалось подтвердить сохранение. Войдите снова, чтобы загрузить сохранённые вещи и прогресс").await;
                                        break;
                                    },
                                };
                                drop(simulation);
                                if send(socket, &reply).await.is_err() { break; }
                            }
                            Ok(ClientMessage::Attack {target}) => {
                                let result=server.simulation.lock().await.attack(id,target);
                                if let Err(message)=result && send(socket,&ServerMessage::Error{message:message.into(),fatal:false}).await.is_err(){break;}
                            }
                            Ok(ClientMessage::UseSkill {target}) => {
                                let result=server.simulation.lock().await.use_skill(id,target);
                                if let Err(message)=result && send(socket,&ServerMessage::Error{message:message.into(),fatal:false}).await.is_err(){break;}
                            }
                            Ok(ClientMessage::UseSecondary { target }) => {
                                let result = server.simulation.lock().await.use_secondary(id, target);
                                if let Err(message)=result && send(socket,&ServerMessage::Error{message:message.into(),fatal:false}).await.is_err(){break;}
                            }
                            Ok(ClientMessage::UseAbility { skill, target }) => {
                                let result = server.simulation.lock().await.use_ability(id, skill, target);
                                if let Err(message) = result && send(socket, &ServerMessage::Error { message: message.into(), fatal: false }).await.is_err() { break; }
                            }
                            Ok(ClientMessage::Respawn) => {
                                let result=server.simulation.lock().await.respawn(id);
                                if let Err(message)=result && send(socket,&ServerMessage::Error{message:message.into(),fatal:false}).await.is_err(){break;}
                            }
                            Ok(ClientMessage::LeaveWorld) => {return_to_selection=true;break;},
                            Ok(ClientMessage::Logout) => break,
                            _ => { error(socket,"Неверная команда").await; break; }
                        },
                        Message::Close(_) => break,
                        Message::Ping(bytes) => { if send_frame(socket, Message::Pong(bytes)).await.is_err() { break; } }
                        _ => {}
                    }
                }
            }
        }
    }
    // Remove only after persistence, so reconnect never races a stale save.
    let mut simulation = server.simulation.lock().await;
    simulation.exchanges.leave(id);
    let actor = simulation.actor(id).unwrap();
    let saved = if uncertain_inventory {
        Err("Соединение завершено без повторной записи инвентаря и баланса; состояние будет загружено из базы при входе".into())
    } else {
        store::save(conn, &actor).await
    };
    simulation.leave(id);
    drop(simulation);
    saved?;
    if !return_to_selection {
        let _ = send(socket, &ServerMessage::Saved).await;
        let _ = send_frame(socket, Message::Close(None)).await;
    }
    Ok(return_to_selection)
}
