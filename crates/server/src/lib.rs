mod alliance;
pub mod build;
mod casting;
mod chat;
pub mod combat;
mod console;
pub mod crafting_physics;
pub mod economy;
mod exchange;
mod inventory;
pub mod legacy;
mod life;
mod objects;
mod party;
pub mod simulation;
mod society;
pub mod store;
mod work;

use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use mortown_protocol::{
    ClientMessage, PROTOCOL_VERSION, ServerMessage, VERSION_MISMATCH, WORLD_VERSION,
};
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
    hearing: tokio::sync::broadcast::Sender<(Vec<i64>, i64, String)>,
    alliance_events: watch::Sender<()>,
    admin_events: watch::Sender<()>,
    /// Ground stacks or containers changed somewhere; sessions resend their nearby list.
    object_events: watch::Sender<()>,
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
        store::convert_legacy_characters(&mut *pool.acquire().await?).await?;
        let terrain = Arc::new(mortown_world::World::new());
        let converted = store::convert_legacy_items(&mut *pool.acquire().await?, &terrain).await?;
        if converted > 0 {
            println!("Старые инвентари переведены в физические вещи: {converted} персонажей");
        }
        let progressed = store::convert_legacy_progress(&mut *pool.acquire().await?).await?;
        if progressed > 0 {
            println!("Старый прогресс переведён: {progressed} персонажей");
        }
        work::seed(&pool, &terrain).await?;
        build::grant_pool(&pool).await?;
        let kept = legacy::report(&mut *pool.acquire().await?).await?;
        if kept.records > 0 {
            println!(
                "Сохранено до физических хранилищ: {} записей, {} предметов, {} монет, владельцев: {}",
                kept.records, kept.items, kept.coins, kept.owners
            );
        }
        let mut simulation_state = Simulation::new(terrain);
        if let Ok(mut conn) = pool.acquire().await
            && let Ok(blocks) = build::all(&mut conn).await
        {
            simulation_state.set_blocks(blocks);
        }
        let simulation = Arc::new(Mutex::new(simulation_state));
        let (tx, snapshots) = watch::channel(ServerMessage::Snapshot {
            tick: 0,
            actors: vec![],
            monsters: vec![],
            areas: vec![],
        });
        let (object_events, _) = watch::channel(());
        let sim = simulation.clone();
        let death_pool = pool.clone();
        let moved = object_events.clone();
        let mut stop = shutdown.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(50));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = stop.changed() => break,
                    _ = tick.tick() => {
                        let (deaths, expired, terrain) = {
                            let mut s = sim.lock().await;
                            s.update();
                            let terrain = s.tick().is_multiple_of(20).then(|| s.terrain());
                            (s.take_deaths(), s.expired_offline(), terrain)
                        };
                        if let Some(terrain) = terrain
                            && society::fly(&death_pool, &terrain).await.unwrap_or(false)
                        {
                            moved.send_replace(());
                        }
                        for actor in deaths {
                            let mut settled = false;
                            if let Ok(changed) = life::apply(&death_pool, actor.death_mark, actor.id).await {
                                settled = changed.is_none();
                                if changed.is_some()
                                    && let Ok(mut conn) = death_pool.acquire().await
                                    && let Ok(sheet) = store::sheet(&mut conn, actor.id).await
                                {
                                    sim.lock().await.finish_death(&sheet);
                                    settled = true;
                                }
                            }
                            if !settled {
                                sim.lock().await.mark_pending(actor.id);
                            }
                        }
                        for actor in expired {
                            if !sim.lock().await.still_offline(actor.id) {
                                continue;
                            }
                            if let Ok(mut conn) = death_pool.acquire().await {
                                let _ = store::save(&mut conn, &actor).await;
                            }
                            sim.lock().await.retire_offline(&[actor.id]);
                        }
                        let s = sim.lock().await;
                        tx.send_replace(ServerMessage::Snapshot { tick: s.tick(), actors: s.snapshot(), monsters:s.monsters(), areas: s.areas() });
                    }
                }
            }
        });
        let (chat, _) = tokio::sync::broadcast::channel(256);
        let (alliance_events, _) = watch::channel(());
        let (admin_events, _) = watch::channel(());
        let (announcements, _) = tokio::sync::broadcast::channel(32);
        let (hearing, _) = tokio::sync::broadcast::channel(64);
        Ok(Self {
            admin_events,
            object_events,
            announcements,
            alliance_events,
            chat,
            hearing,
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
        error(&mut socket, VERSION_MISMATCH).await;
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
                Ok(ClientMessage::CreateCharacter { nickname, gender }) => {
                    match store::create_character(
                        conn,
                        account,
                        store::CharacterCreation { nickname, gender },
                    )
                    .await
                    {
                        Ok(id) => {
                            let _ = build::grant_starting_plots(conn).await;
                            character_list(socket, conn, account, Some(id)).await
                        }
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
    let mut object_events = server.object_events.subscribe();
    let id = character.id;
    if character.stats.get("ended").copied() == Some(1.) {
        let _ = send(
            socket,
            &ServerMessage::Error {
                message: "Жизни этого человека закончились".into(),
                fatal: false,
            },
        )
        .await;
        return Ok(true);
    }
    let mates: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM characters WHERE account_id=(SELECT account_id FROM characters WHERE id=$1) AND id<>$1",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(store::database_error)?;
    {
        let simulation = server.simulation.lock().await;
        if simulation.account_busy(&mates) {
            drop(simulation);
            let _ = send(
                socket,
                &ServerMessage::Error {
                    message: "На этом аккаунте уже кто-то в игре".into(),
                    fatal: false,
                },
            )
            .await;
            return Ok(true);
        }
        let bots: Vec<mortown_protocol::Actor> = mates
            .iter()
            .filter_map(|mate| simulation.actor(*mate).filter(|actor| actor.offline))
            .collect();
        drop(simulation);
        for actor in &bots {
            store::save(conn, actor).await?;
        }
        server.simulation.lock().await.retire_offline(&mates);
    }
    let actor = {
        let mut simulation = server.simulation.lock().await;
        simulation.join(id, character.nickname.clone(), pos, yaw)?;
        // Restore membership only after the actor has its saved stats, before the next tick.
        let mut actor = simulation.configure(&character);
        if let Ok((presented, hidden)) = society::presentation(conn, id).await {
            actor.presented = presented;
            actor.face_hidden = hidden;
        }
        actor.identity = character.stats.get("identity").copied().unwrap_or(1.);
        simulation.replace_actor(actor.clone());
        actor
    };
    let mut known = society::notes(conn, id).await.unwrap_or_default();
    let mut saved_kills = actor.progress.kills;
    let welcome = ServerMessage::Welcome {
        version: PROTOCOL_VERSION,
        world: WORLD_VERSION.into(),
        character,
        actor,
    };
    let mut hearing = server.hearing.subscribe();
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
    let mut exchange_received = 0;
    let mut object_received = 0;
    let mut chat_received = 0;
    let mut return_to_selection = false;
    let mut uncertain_inventory = false;
    if send(socket, &welcome).await.is_ok()
        && send_objects(socket, server, conn, id).await.is_ok()
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
                        ServerMessage::EconomyUpdated{character,actor}
                    });
                    snapshots.borrow_and_update();
                    drop(simulation);
                    match update {Ok(reply)=>{if send(socket,&reply).await.is_err(){break;}},Err(e)=>{error(socket,e).await;break;}}

                },
                _ = object_events.changed() => {
                    if send_objects(socket,server,conn,id).await.is_err(){break;}
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
                heard = hearing.recv() => {
                    if let Ok((recipients, speaker, label)) = heard
                        && recipients.contains(&id)
                        && send(socket, &ServerMessage::Hearing { speaker, label }).await.is_err()
                    {
                        break;
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
                    let simulation = server.simulation.lock().await;
                    let actor = simulation.actor(id).unwrap();
                    if let Err(e) = store::save(conn, &actor).await { drop(simulation); error(socket,e).await; break; }
                    saved_kills = actor.progress.kills;
                    drop(simulation);
                    if send_objects(socket,server,conn,id).await.is_err(){break;}
                    let actors=server.simulation.lock().await.snapshot();
                    match alliance::view(conn,id,&actors).await {
                        Ok(reply)=>{if send(socket,&ServerMessage::Alliance{reply}).await.is_err(){break;}},
                        Err(message)=>{error(socket,message).await;break;}
                    }
                }
                changed = snapshots.changed() => {
                    if changed.is_err() { break; }
                    let mut snapshot = snapshots.borrow_and_update().clone();
                    let mut simulation=server.simulation.lock().await;
                    // Cross-player exchange may have updated this actor after snapshot generation.
                    let mut current=simulation.actor(id).unwrap();
                    let grants=simulation.take_grants(id);
                    if !grants.is_empty() {
                        simulation.exchanges.cancel(id,"Обмен отменён: в рюкзак поступила добыча");
                        match objects::grant(conn,&mut current,&grants).await {
                            Ok((character,ground))=>{
                                simulation.replace_actor(current.clone());
                                simulation.refresh_character(&character);
                                saved_kills=current.progress.kills;
                                if ground {server.object_events.send_replace(());}
                                if send(socket,&ServerMessage::EconomyUpdated{character,actor:current.clone()}).await.is_err(){drop(simulation);break;}
                            }
                            Err(store::ExchangeFailure::Rejected(e))=>{drop(simulation);error(socket,e).await;break;}
                            Err(store::ExchangeFailure::Uncertain)=>{uncertain_inventory=true;drop(simulation);error(socket,"Не удалось подтвердить сохранение добычи. Войдите снова").await;break;}
                        }
                    }
                    if current.progress.kills != saved_kills {
                        if let Err(e)=store::save(conn,&current).await { drop(simulation); error(socket,e).await; break; }
                        saved_kills=current.progress.kills;
                    }
                    if let ServerMessage::Snapshot{actors,..}=&mut snapshot {
                        actors.retain(|a| a.id == id || !a.powers.invisible);
                        for actor in actors.iter_mut() {
                            if actor.id != id {
                                let note = known.get(&actor.id).and_then(|(identity, note)| {
                                    (*identity == actor.identity).then_some(note.as_str())
                                });
                                let label = mortown_protocol::society::public_label(
                                    &actor.presented,
                                    actor.face_hidden,
                                    note,
                                );
                                mortown_protocol::conceal(actor, label);
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
                    if budget_start.elapsed() >= Duration::from_secs(1) { budget_start=Instant::now(); received=0; exchange_received=0; object_received=0; chat_received=0; }
                    received+=1; if received > 100 { error(socket,"Слишком много команд").await; break; }
                    match message {
                        Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Alliance{command})=>{
                                chat_received+=1;
                                let mut simulation=server.simulation.lock().await;
                                let refresh=matches!(command,mortown_protocol::alliance::Command::Refresh);
                                let result=if chat_received>4 {Err("Слишком много действий с альянсом".into())} else {
                                    if matches!(command,mortown_protocol::alliance::Command::Create{..}) {
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
                            Ok(ClientMessage::Society { command }) => {
                                chat_received += 1;
                                if chat_received > 4 {
                                    error(socket, "Слишком много действий").await;
                                    break;
                                }
                                let actor = server.simulation.lock().await.actor(id).unwrap();
                                match society::apply(conn, &actor, command).await {
                                    Ok(message) => {
                                        if let Ok((presented, hidden)) = society::presentation(conn, id).await {
                                            let mut actor = server.simulation.lock().await.actor(id).unwrap();
                                            actor.presented = presented;
                                            actor.face_hidden = hidden;
                                            server.simulation.lock().await.replace_actor(actor);
                                        }
                                        if let Ok(notes) = society::notes(conn, id).await {
                                            known = notes;
                                        }
                                        if send(socket, &ServerMessage::Chat { reply: mortown_protocol::chat::Reply::Status { message } }).await.is_err() { break; }
                                    }
                                    Err(message) => {
                                        if send(socket, &ServerMessage::Chat { reply: mortown_protocol::chat::Reply::Status { message } }).await.is_err() { break; }
                                    }
                                }
                            }
                            Ok(ClientMessage::Voice { speaking }) => {
                                if !speaking { continue; }
                                let (listeners, label) = {
                                    let sim = server.simulation.lock().await;
                                    let Some(actor) = sim.actor(id) else { continue; };
                                    let world = sim.terrain();
                                    let from = [actor.position[0], actor.position[2]];
                                    let label = mortown_protocol::society::public_label(&actor.presented, actor.face_hidden, None);
                                    let listeners = sim.snapshot().into_iter().filter(|other| {
                                        let at = [other.position[0], other.position[2]];
                                        other.id != id && mortown_protocol::society::in_earshot(from, at, world.segment_clear(from, at))
                                    }).map(|other| other.id).collect();
                                    (listeners, label)
                                };
                                let _ = server.hearing.send((listeners, id, label));
                            }
                            Ok(ClientMessage::Chat{command})=>{
                                chat_received+=1;
                                let reply=if chat_received>4 {Some(mortown_protocol::chat::Reply::Status{message:"Слишком много действий в чате".into()})} else {chat::handle(server,conn,id,&mut chat_state,command).await};
                                // A refused console command drops its transaction, which only queues the
                                // ROLLBACK; run it now so its row locks do not outlive the command.
                                if sqlx::Connection::ping(&mut *conn).await.is_err(){error(socket,"База данных недоступна").await;break;}
                                if matches!(reply,Some(mortown_protocol::chat::Reply::Logout)){break;}
                                if let Some(reply)=reply && send(socket,&ServerMessage::Chat{reply}).await.is_err(){break;}
                                if send(socket,&ServerMessage::Chat{reply:chat_state.preferences()}).await.is_err(){break;}
                            }
                            Ok(ClientMessage::Exchange { command }) => {
                                exchange_received+=1;
                                if exchange_received>8 { error(socket,"Слишком много запросов обмена").await; break; }
                                let mut simulation=server.simulation.lock().await;
                                exchange::handle(&mut simulation,conn,id,command).await;
                            }
                            Ok(ClientMessage::Cast { command }) => {
                                object_received += 1;
                                if object_received > 8 {
                                    error(socket, "Слишком много действий с вещами").await;
                                    break;
                                }
                                let (actor, terrain) = {
                                    let simulation = server.simulation.lock().await;
                                    let Some(actor) = simulation.actor(id) else {
                                        drop(simulation);
                                        error(socket, "Персонаж не в мире").await;
                                        break;
                                    };
                                    (actor, simulation.terrain())
                                };
                                match crate::casting::handle(conn, &terrain, actor, command).await {
                                    Ok(view) => {
                                        if let Some(update) = &view.update {
                                            let mut simulation = server.simulation.lock().await;
                                            simulation.replace_actor(update.actor.clone());
                                            simulation.refresh_character(&update.character);
                                            saved_kills = update.actor.progress.kills;
                                            snapshots.borrow_and_update();
                                        }
                                        if send(socket, &ServerMessage::Cast { view }).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(store::ExchangeFailure::Rejected(message)) => {
                                        if send(
                                            socket,
                                            &ServerMessage::Error {
                                                message,
                                                fatal: false,
                                            },
                                        )
                                        .await
                                        .is_err()
                                        {
                                            break;
                                        }
                                    }
                                    Err(store::ExchangeFailure::Uncertain) => {
                                        uncertain_inventory = true;
                                        error(socket, "Не удалось подтвердить литьё. Войдите снова").await;
                                        break;
                                    }
                                }
                            }
                            Ok(ClientMessage::Work { command }) => {
                                object_received += 1;
                                if object_received > 8 { error(socket, "Слишком много действий с вещами").await; break; }
                                let mut simulation = server.simulation.lock().await;
                                simulation.exchanges.cancel(id, "Обмен отменён: вещи перемещаются");
                                let actor = simulation.actor(id).unwrap();
                                let terrain = simulation.terrain();
                                let reply = match work::apply(conn, &terrain, actor, command).await {
                                    Ok(outcome) => {
                                        let update = outcome.update.map(|(character, actor)| {
                                            simulation.replace_actor(actor.clone());
                                            simulation.refresh_character(&character);
                                            saved_kills = actor.progress.kills;
                                            snapshots.borrow_and_update();
                                            Box::new(mortown_protocol::transfer::InventoryUpdate { character, actor })
                                        });
                                        if outcome.changed {
                                            if let Ok(blocks) = build::all(conn).await {
                                                simulation.set_blocks(blocks);
                                            }
                                            server.object_events.send_replace(());
                                        }
                                        mortown_protocol::objects::Reply { view: outcome.view, message: outcome.message, update }
                                    }
                                    Err(store::ExchangeFailure::Rejected(message)) => mortown_protocol::objects::Reply { view: None, message, update: None },
                                    Err(store::ExchangeFailure::Uncertain) => {
                                        uncertain_inventory = true;
                                        drop(simulation);
                                        error(socket, "Не удалось подтвердить работу. Войдите снова").await;
                                        break;
                                    }
                                };
                                drop(simulation);
                                if send(socket, &ServerMessage::Object { reply }).await.is_err() { break; }
                            }
                            Ok(ClientMessage::Objects { command }) => {
                                object_received+=1;
                                if object_received>8 { error(socket,"Слишком много действий с вещами").await; break; }
                                let mut simulation=server.simulation.lock().await;
                                simulation.exchanges.cancel(id,"Обмен отменён: вещи перемещаются");
                                let actor=simulation.actor(id).unwrap();
                                let terrain=simulation.terrain();
                                let reply=match objects::apply(conn,&terrain,actor,command).await {
                                    Ok(outcome)=>{
                                        let update=outcome.update.map(|(character,actor)|{
                                            simulation.replace_actor(actor.clone());
                                            simulation.refresh_character(&character);
                                            saved_kills=actor.progress.kills;
                                            snapshots.borrow_and_update();
                                            Box::new(mortown_protocol::transfer::InventoryUpdate{character,actor})
                                        });
                                        if outcome.changed {server.object_events.send_replace(());}
                                        mortown_protocol::objects::Reply{view:outcome.view,message:outcome.message,update}
                                    }
                                    Err(store::ExchangeFailure::Rejected(message))=>mortown_protocol::objects::Reply{view:None,message,update:None},
                                    Err(store::ExchangeFailure::Uncertain)=>{
                                        uncertain_inventory=true;
                                        drop(simulation);
                                        error(socket,"Не удалось подтвердить перемещение вещей. Войдите снова, чтобы загрузить сохранённое состояние").await;
                                        break;
                                    }
                                };
                                drop(simulation);
                                if send(socket,&ServerMessage::Object{reply}).await.is_err(){break;}
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
                                let reply=match store::set_equipment(conn,actor,item,equipped).await {
                                    Ok((character,actor))=>{
                                        simulation.replace_actor(actor);
                                        simulation.refresh_character(&character);
                                        snapshots.borrow_and_update();
                                        ServerMessage::CharacterUpdated {character}
                                    },
                                    Err(store::ExchangeFailure::Rejected(message))=>ServerMessage::Error {message,fatal:false},
                                    Err(store::ExchangeFailure::Uncertain)=>{
                                        uncertain_inventory=true;
                                        drop(simulation);
                                        error(socket,"Не удалось подтвердить сохранение экипировки. Войдите снова").await;
                                        break;
                                    }
                                };
                                drop(simulation);
                                if send(socket,&reply).await.is_err(){break;}
                            }
                            Ok(command @ (ClientMessage::Craft { .. } | ClientMessage::UsePotion | ClientMessage::SpendTalent { .. } | ClientMessage::SpendAttribute { .. } | ClientMessage::BindHotbar { .. } | ClientMessage::Arrange { .. })) => {
                                let action = match command { ClientMessage::SpendTalent { talent, expected } => economy::Action::SpendTalent { talent, expected }, ClientMessage::Craft { recipe, expected } => economy::Action::Craft { recipe, expected }, ClientMessage::Arrange { source, target, expected } => economy::Action::Arrange { source,target,expected }, ClientMessage::BindHotbar { slot, binding } => economy::Action::BindHotbar { slot, binding }, ClientMessage::SpendAttribute { stat, expected } => economy::Action::SpendAttribute { stat, expected }, _ => economy::Action::Potion };
                                let mut simulation = server.simulation.lock().await;
                                simulation.exchanges.cancel(id,"Обмен отменён: инвентарь или характеристики изменяются");
                                let actor = simulation.actor(id).unwrap();
                                let result=store::exchange(conn, actor, action).await;
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
                            Ok(ClientMessage::OfflineOrders { returns, guards, works }) => {
                                let actor = {
                                    let mut simulation = server.simulation.lock().await;
                                    let Some(mut actor) = simulation.actor(id) else { break };
                                    actor.orders = mortown_protocol::life::Orders { returns, guards, works };
                                    simulation.replace_actor(actor.clone());
                                    actor
                                };
                                if let Err(message) = store::save(conn, &actor).await && send(socket, &ServerMessage::Error { message, fatal: false }).await.is_err() { break; }
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
    let mut actor = simulation.actor(id).unwrap();
    let grants = simulation.take_grants(id);
    let saved = if uncertain_inventory {
        Err("Соединение завершено без повторной записи инвентаря и баланса; состояние будет загружено из базы при входе".into())
    } else if !grants.is_empty() {
        match objects::grant(conn, &mut actor, &grants).await {
            Ok((_, ground)) => {
                if ground {
                    server.object_events.send_replace(());
                }
                Ok(())
            }
            Err(store::ExchangeFailure::Rejected(e)) => Err(e),
            Err(store::ExchangeFailure::Uncertain) => {
                Err("Не удалось подтвердить сохранение добычи; состояние будет загружено из базы при входе".into())
            }
        }
    } else {
        store::save(conn, &actor).await
    };
    if actor.lives == 0 {
        simulation.leave(id);
    } else {
        simulation.disconnect(id);
    }
    drop(simulation);
    saved?;
    if !return_to_selection {
        let _ = send(socket, &ServerMessage::Saved).await;
        let _ = send_frame(socket, Message::Close(None)).await;
    }
    Ok(return_to_selection)
}
async fn send_objects(
    socket: &mut WebSocket,
    server: &Server,
    conn: &mut sqlx::PgConnection,
    id: i64,
) -> Result<(), ()> {
    let Some(actor) = server.simulation.lock().await.actor(id) else {
        return Ok(());
    };
    let at = [actor.position[0], actor.position[2]];
    let Ok((mut containers, ground)) = objects::nearby(conn, at).await else {
        return Ok(());
    };
    let castings = objects::casting_bodies(conn, &ground)
        .await
        .unwrap_or_default();
    if work::offers(conn, &mut containers).await.is_err() {
        return Ok(());
    }
    let Ok((nodes, plots)) = work::nearby(conn, at).await else {
        return Ok(());
    };
    let Ok(blocks) = build::nearby(conn, at).await else {
        return Ok(());
    };
    send(
        socket,
        &ServerMessage::Objects {
            containers,
            ground,
            nodes,
            plots,
            blocks,
            castings,
        },
    )
    .await
}
