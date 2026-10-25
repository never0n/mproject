//! Run against a development PostgreSQL: TEST_DATABASE_URL=... cargo test -p mortown-server --test network -- --ignored
use futures_util::{SinkExt, StreamExt};
use mortown_protocol::{
    Actor, Character, ClientMessage, PROTOCOL_VERSION, ServerMessage, WORLD_VERSION,
    items::{self, Location},
};
use mortown_server::Server;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::{net::TcpStream, sync::watch};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn crafting_atomic_consumption_duplicates_and_restart() {
    use mortown_protocol::{
        crafting::{self, CUIRASS_RECIPE, LEATHER},
        layout::{self, Place},
    };
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "craft{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    // Only this disposable character receives materials and the recipe; all crafting uses WebSocket.
    sqlx::query("UPDATE characters SET stats=stats || '{\"level\":3}'::jsonb WHERE id=$1")
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    stock(
        &server.pool,
        id,
        &[(LEATHER, 12), (7, 8), (CUIRASS_RECIPE, 1)],
        None,
    )
    .await;
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome {
        character: initial, ..
    } = equipment_reply(&mut s).await
    else {
        panic!("welcome")
    };
    send(
        &mut s,
        ClientMessage::Craft {
            recipe: CUIRASS_RECIPE,
            expected: 5,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    assert_eq!(crafting::in_bag(&initial, LEATHER), 12);
    assert_eq!(crafting::in_bag(&initial, 7), 8);
    assert_eq!(crafting::count(&initial), 0);
    let recipe = initial
        .items
        .iter()
        .find(|s| s.kind == CUIRASS_RECIPE)
        .unwrap();
    let Location::Bag { cell: bag } = recipe.location else {
        panic!("recipe in the bag")
    };
    let recipe_token = layout::item_token(recipe.id);
    send(
        &mut s,
        ClientMessage::Arrange {
            source: Place::Bag(bag),
            target: Place::Bar(11),
            expected: recipe_token,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated {
        character: bound, ..
    } = equipment_reply(&mut s).await
    else {
        panic!("bind")
    };
    // A supply on the bar is bound by kind, so it keeps working as stacks change.
    let recipe_token = layout::kind_token(CUIRASS_RECIPE);
    assert_eq!(layout::at(&bound, Place::Bar(11)), recipe_token);
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 3,
            equipped: true,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated {
        character: equipped,
    } = equipment_reply(&mut s).await
    else {
        panic!("equip")
    };
    let defence = equipped.stats["f_defence"];
    send(
        &mut s,
        ClientMessage::Craft {
            recipe: CUIRASS_RECIPE,
            expected: 0,
        },
    )
    .await;
    send(
        &mut s,
        ClientMessage::Craft {
            recipe: CUIRASS_RECIPE,
            expected: 0,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated {
        character: crafted,
        actor,
    } = equipment_reply(&mut s).await
    else {
        panic!("craft")
    };
    assert_eq!(
        actor.equipment,
        items::EquipmentAppearance::from_items(&crafted.items)
    );
    assert_eq!(crafting::in_bag(&crafted, LEATHER), 6);
    assert_eq!(crafting::in_bag(&crafted, 7), 4);
    assert_eq!(carried(&crafted, 6), 1);
    assert_eq!(crafting::in_bag(&crafted, CUIRASS_RECIPE), 1);
    assert_eq!(crafted.stats["f_defence"], defence);
    assert_eq!(
        items::purse_value(&crafted.items),
        items::purse_value(&initial.items)
    );
    assert_eq!(crafting::count(&crafted), 1);
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;

    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let restored = list(&mut s).await.remove(0);
    assert_eq!(restored.items, crafted.items);
    assert_eq!(crafting::count(&restored), 1);
    assert_eq!(restored.stats["f_defence"], defence);
    assert_eq!(layout::at(&restored, Place::Bar(11)), recipe_token);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    send(
        &mut s,
        ClientMessage::Craft {
            recipe: CUIRASS_RECIPE,
            expected: 0,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(
        &mut s,
        ClientMessage::Craft {
            recipe: CUIRASS_RECIPE,
            expected: 1,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated {
        character: finished,
        ..
    } = equipment_reply(&mut s).await
    else {
        panic!("second craft")
    };
    assert_eq!(crafting::count(&finished), 2);
    assert_eq!(crafting::in_bag(&finished, CUIRASS_RECIPE), 1);
    assert!(!finished.items.iter().any(|i| matches!(i.kind, 7 | LEATHER)));
    // Every crafted cuirass is its own stack in its own cell.
    assert_eq!(
        layout::bag(&finished)
            .values()
            .filter(|t| layout::kind(&finished, **t) == Some(6))
            .count(),
        2
    );
    send(
        &mut s,
        ClientMessage::Craft {
            recipe: CUIRASS_RECIPE,
            expected: 2,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::LeaveWorld).await;
    let restored = list(&mut s).await;
    assert_eq!(restored[0].items, finished.items);
    assert_eq!(crafting::count(&restored[0]), 2);
    send(&mut s, creation(&format!("{name}b"))).await;
    let roster = list(&mut s).await;
    assert_eq!(crafting::count(&roster[1]), 0);
    assert!(
        roster[1]
            .items
            .iter()
            .all(|s| items::STARTER.contains(&s.kind) || items::coin_value(s.kind).is_some())
    );
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}
async fn send(s: &mut Socket, m: ClientMessage) {
    s.send(Message::Text(serde_json::to_string(&m).unwrap().into()))
        .await
        .unwrap();
}
/// Next message, skipping the background lists every session gets (nearby objects, chat
/// preferences, alliance view) so request/reply checks see the reply.
async fn receive(s: &mut Socket) -> ServerMessage {
    loop {
        match raw(s).await {
            ServerMessage::Objects { .. }
            | ServerMessage::Chat { .. }
            | ServerMessage::Alliance { .. } => {}
            other => return other,
        }
    }
}
async fn frame(s: &mut Socket) -> ServerMessage {
    loop {
        if let Message::Text(text) = s
            .next()
            .await
            .expect("connection closed")
            .expect("socket error")
        {
            return serde_json::from_str(&text).unwrap();
        }
    }
}
async fn casting_reply(s: &mut Socket) -> ServerMessage {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match frame(s).await {
                ServerMessage::Objects { .. }
                | ServerMessage::Chat { .. }
                | ServerMessage::Alliance { .. }
                | ServerMessage::Snapshot { .. } => {}
                other => return other,
            }
        }
    })
    .await
    .expect("casting reply")
}
async fn raw(s: &mut Socket) -> ServerMessage {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Message::Text(text) = s
                .next()
                .await
                .expect("connection closed")
                .expect("socket error")
            {
                return serde_json::from_str(&text).unwrap();
            }
        }
    })
    .await
    .expect("server response timeout")
}
/// Gives a disposable character extra bag stacks (after its existing cells) and, optionally,
/// replaces its purse with `coins` copper worth of coins.
async fn stock(pool: &sqlx::PgPool, id: i64, bag: &[(u8, u32)], coins: Option<u64>) {
    let mut cell: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(cell)+1,0)::int FROM items WHERE character_id=$1 AND place='bag'",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    for &(kind, quantity) in bag {
        sqlx::query(
            "INSERT INTO items(kind,quantity,place,character_id,cell) VALUES($1,$2,'bag',$3,$4)",
        )
        .bind(i16::from(kind))
        .bind(quantity as i32)
        .bind(id)
        .bind(cell)
        .execute(pool)
        .await
        .unwrap();
        cell += 1;
    }
    if let Some(value) = coins {
        sqlx::query("DELETE FROM items WHERE character_id=$1 AND place='purse'")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        for (kind, n) in items::denominate(value) {
            if n > 0 {
                sqlx::query(
                    "INSERT INTO items(kind,quantity,place,character_id) VALUES($1,$2,'purse',$3)",
                )
                .bind(i16::from(kind))
                .bind(n as i32)
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
            }
        }
    }
}
/// Carried (bag + body) quantity of a kind.
fn carried(c: &Character, kind: u8) -> u32 {
    c.items
        .iter()
        .filter(|s| s.kind == kind && s.carried())
        .map(|s| s.quantity)
        .sum()
}
fn worn(c: &Character) -> Vec<u8> {
    c.items
        .iter()
        .filter(|s| matches!(s.location, Location::Worn { .. }))
        .map(|s| s.kind)
        .collect()
}
async fn connect(address: &str) -> Socket {
    tokio_tungstenite::connect_async(address).await.unwrap().0
}
fn registration(login: &str) -> ClientMessage {
    ClientMessage::Register {
        version: PROTOCOL_VERSION,
        world: WORLD_VERSION.into(),
        login: login.into(),
        password: "test-password-06".into(),
    }
}
async fn register(address: &str, name: &str) {
    let mut s = connect(address).await;
    send(&mut s, registration(name)).await;
    assert!(matches!(receive(&mut s).await, ServerMessage::Registered));
}
async fn list(s: &mut Socket) -> Vec<mortown_protocol::Character> {
    loop {
        match receive(s).await {
            ServerMessage::Characters {
                characters, limit, ..
            } => {
                assert_eq!(limit, 12);
                return characters;
            }
            ServerMessage::Snapshot { .. } => {}
            other => panic!("character list expected: {other:?}"),
        }
    }
}
fn creation(name: &str) -> ClientMessage {
    ClientMessage::CreateCharacter {
        nickname: name.into(),
        gender: 1,
    }
}
async fn enter(s: &mut Socket, id: i64) -> Actor {
    send(s, ClientMessage::SelectCharacter { character_id: id }).await;
    welcome(s).await
}
fn login(name: &str, password: &str) -> ClientMessage {
    ClientMessage::Login {
        version: PROTOCOL_VERSION,
        world: WORLD_VERSION.into(),
        login: name.into(),
        password: password.into(),
    }
}
async fn welcome(s: &mut Socket) -> Actor {
    match receive(s).await {
        ServerMessage::Welcome {
            actor, character, ..
        } => {
            assert_eq!(
                base(&character.stats),
                mortown_protocol::starting_stats().unwrap()
            );
            actor
        }
        other => panic!("welcome expected: {other:?}"),
    }
}
/// The starting keys of a sheet; saving also stores progress flags next to them.
fn base(
    stats: &std::collections::BTreeMap<String, f64>,
) -> std::collections::BTreeMap<String, f64> {
    let start = mortown_protocol::starting_stats().unwrap();
    stats
        .iter()
        .filter(|(k, _)| start.contains_key(*k))
        .map(|(k, v)| (k.clone(), *v))
        .collect()
}
async fn saved(s: &mut Socket) {
    loop {
        match receive(s).await {
            ServerMessage::Saved => return,
            ServerMessage::Error { message, .. } => panic!("save failed: {message}"),
            _ => {}
        }
    }
}
async fn launch(
    database: &str,
) -> (
    Server,
    String,
    watch::Sender<bool>,
    tokio::task::JoinHandle<()>,
) {
    let (stop, rx) = watch::channel(false);
    let server = Server::new(database, rx.clone()).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("ws://{}/world", listener.local_addr().unwrap());
    let router = server.router();
    let mut quit = rx;
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = quit.changed().await;
            })
            .await
            .unwrap();
    });
    (server, address, stop, task)
}
#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn account_roster_switching_ownership_capacity_and_restart() {
    let database = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL");
    let (server, address, stop, task) = launch(&database).await;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 1_000_000_000;
    let a = format!("qa{unique}a");
    let b = format!("qa{unique}b");
    register(&address, &a).await;
    register(&address, &b).await;
    let account: i64 = sqlx::query_scalar("SELECT id FROM accounts WHERE login=$1")
        .bind(&a)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM characters WHERE account_id=$1")
        .bind(account)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "registration must not create a character");
    let mut alice = connect(&address).await;
    send(&mut alice, login(&a, "test-password-06")).await;
    assert!(list(&mut alice).await.is_empty());
    send(
        &mut alice,
        ClientMessage::Move {
            seq: 1,
            target: [0., 0.],
            walking: false,
        },
    )
    .await;
    assert!(matches!(
        receive(&mut alice).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut alice, creation(&a)).await;
    let chars = list(&mut alice).await;
    let first = chars[0].id;
    assert_eq!(chars[0].slot, 1);
    let start = enter(&mut alice, first).await;
    let mut bob = connect(&address).await;
    send(&mut bob, login(&b, "test-password-06")).await;
    assert!(list(&mut bob).await.is_empty());
    send(
        &mut bob,
        ClientMessage::SelectCharacter {
            character_id: first,
        },
    )
    .await;
    assert!(matches!(
        receive(&mut bob).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut bob, creation(&a)).await;
    assert!(matches!(
        receive(&mut bob).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut bob, creation(&b)).await;
    let other = list(&mut bob).await[0].id;
    enter(&mut bob, other).await;
    let mut extra = connect(&address).await;
    send(&mut extra, login(&a, "wrong-password")).await;
    assert!(matches!(
        receive(&mut extra).await,
        ServerMessage::Error { fatal: true, .. }
    ));
    let mut extra = connect(&address).await;
    send(&mut extra, login(&a, "test-password-06")).await;
    assert!(matches!(
        receive(&mut extra).await,
        ServerMessage::Error { fatal: true, .. }
    ));
    let mut extra = connect(&address).await;
    send(&mut extra, creation("intruder")).await;
    assert!(matches!(
        receive(&mut extra).await,
        ServerMessage::Error { fatal: true, .. }
    ));
    send(
        &mut alice,
        ClientMessage::Move {
            seq: 1,
            target: [start.position[0] - 30., start.position[2]],
            walking: false,
        },
    )
    .await;
    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::Snapshot { actors, .. } = receive(&mut bob).await
                && let Some(p) = actors
                    .into_iter()
                    .find(|p| p.id == first && p.position[0] < start.position[0] - 2.)
            {
                break p;
            }
        }
    })
    .await
    .unwrap();
    assert!(observed.position[0] > start.position[0] - 8.);
    send(&mut alice, ClientMessage::LeaveWorld).await;
    assert_eq!(list(&mut alice).await.len(), 1);
    assert!(
        server
            .simulation
            .lock()
            .await
            .actor(first)
            .is_some_and(|actor| actor.offline)
    );
    send(&mut alice, creation(&format!("{a}2"))).await;
    let chars = list(&mut alice).await;
    let second = chars[1].id;
    assert_eq!(chars[1].slot, 2);
    let second_actor = enter(&mut alice, second).await;
    let stored_x: f32 = sqlx::query_scalar("SELECT x FROM characters WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!((second_actor.position[0] - start.position[0]).abs() < 0.001);
    send(&mut alice, ClientMessage::LeaveWorld).await;
    list(&mut alice).await;
    let resumed = enter(&mut alice, first).await;
    assert!((resumed.position[0] - stored_x).abs() < 0.001);
    send(&mut alice, ClientMessage::LeaveWorld).await;
    list(&mut alice).await;
    for n in 3..=11 {
        send(&mut alice, creation(&format!("{a}{n}"))).await;
        assert_eq!(list(&mut alice).await.len(), n);
    }
    // Two independent service requests contend for the final slot. Database allocation must serialize.
    let mut c1 = server.pool.acquire().await.unwrap();
    let mut c2 = server.pool.acquire().await.unwrap();
    let create = |name: String| mortown_server::store::CharacterCreation {
        nickname: name,
        gender: 2,
    };
    let (r1, r2) = tokio::join!(
        mortown_server::store::create_character(&mut c1, account, create(format!("{a}x"))),
        mortown_server::store::create_character(&mut c2, account, create(format!("{a}y")))
    );
    assert_eq!(usize::from(r1.is_ok()) + usize::from(r2.is_ok()), 1);
    drop(c1);
    drop(c2);
    send(&mut alice, creation(&format!("{a}13"))).await;
    assert!(matches!(
        receive(&mut alice).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM characters WHERE account_id=$1")
        .bind(account)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(count, 12);
    send(&mut alice, ClientMessage::Logout).await;
    send(&mut bob, ClientMessage::Logout).await;
    saved(&mut bob).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut alice = connect(&address).await;
    send(&mut alice, login(&a.to_uppercase(), "test-password-06")).await;
    let chars = list(&mut alice).await;
    assert_eq!(chars.len(), 12);
    assert_eq!(
        chars.iter().map(|c| c.slot).collect::<Vec<_>>(),
        (1..=12).collect::<Vec<_>>()
    );
    let resumed = enter(&mut alice, first).await;
    assert!((resumed.position[0] - stored_x).abs() < 0.001);
    send(&mut alice, ClientMessage::Logout).await;
    saved(&mut alice).await;
    sqlx::query("DELETE FROM accounts WHERE id=$1 OR login=$2")
        .bind(account)
        .bind(&b)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn migration_preserves_existing_single_character_account() {
    use sqlx::Connection;
    let database = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL");
    let mut conn = sqlx::PgConnection::connect(&database).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    // Isolate the old schema in a transaction; rollback removes every fixture and DDL change.
    sqlx::query("CREATE SCHEMA mortown_migration07_test")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO mortown_migration07_test")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0001_accounts_characters.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO accounts(login,password_hash) VALUES('legacy','existing-hash'); INSERT INTO characters(account_id,nickname,race,class,gender,stats,x,z,yaw) VALUES(1,'LegacyHero',4,2,2,'{\"coins\":1234}',45.5,67.25,1.5);").execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0002_character_slots.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0003_inventory.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0004_loot.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let loot: serde_json::Value = sqlx::query_scalar("SELECT loot FROM characters WHERE id=1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(loot, serde_json::json!({}));
    let inventory: serde_json::Value =
        sqlx::query_scalar("SELECT inventory FROM characters WHERE id=1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        inventory,
        serde_json::json!([{"kind":2,"equipped":false},{"kind":3,"equipped":false}])
    );
    let row: (i64, i16, String, serde_json::Value, f32, f32, f32) =
        sqlx::query_as("SELECT id,slot,nickname,stats,x,z,yaw FROM characters WHERE account_id=1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        row,
        (
            1,
            1,
            "LegacyHero".into(),
            serde_json::json!({"coins":1234}),
            45.5,
            67.25,
            1.5
        )
    );
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM accounts WHERE id=1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(hash, "existing-hash");
    for migration in [
        include_str!("../migrations/0005_mail.sql"),
        include_str!("../migrations/0006_mail_attachments.sql"),
        include_str!("../migrations/0007_mail_payments.sql"),
        include_str!("../migrations/0008_mail_returns.sql"),
        include_str!("../migrations/0009_chat.sql"),
        include_str!("../migrations/0010_group_chat.sql"),
        include_str!("../migrations/0011_alliances.sql"),
        include_str!("../migrations/0012_alliance_storage.sql"),
        include_str!("../migrations/0013_chat_commands.sql"),
        include_str!("../migrations/0014_admin_roles.sql"),
        include_str!("../migrations/0015_remove_npc_quests.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&mut *tx).await.unwrap();
    }
    // Property still held by the removed mail and alliance warehouse, plus a chosen profession.
    // Mail 1: unclaimed sale; mail 2: claimed but its payment is not credited yet;
    // mail 3 bounced as return letter 4, which nobody has picked up.
    sqlx::raw_sql(
        "UPDATE characters SET stats=stats || '{\"craft_specialization\":2,\"craft_xp_smithing\":75,\"craft_count\":3}' WHERE id=1;
         INSERT INTO characters(account_id,slot,nickname,race,class,gender,stats,x,z,yaw) VALUES(1,2,'Partner',1,1,1,'{}',0,0,0);
         INSERT INTO character_mail(id,sender_id,recipient_id,subject,body,draft,attachment_kind,attachment_quantity,attachment_token,attachment_price) VALUES
            (1,1,2,'sale','b','00000000000000000000000000000001',5,1,21474836481,50);
         INSERT INTO character_mail(id,sender_id,recipient_id,subject,body,draft,attachment_kind,attachment_quantity,attachment_token,attachment_price,attachment_claimed) VALUES
            (2,2,1,'paid','b','00000000000000000000000000000002',7,3,30064771072,10,true);
         INSERT INTO mail_payments(mail_id,owner_id,amount) VALUES(2,2,10);
         INSERT INTO character_mail(id,sender_id,recipient_id,subject,body,draft,attachment_kind,attachment_quantity,attachment_token,attachment_returned) VALUES
            (3,1,2,'gift','b','00000000000000000000000000000003',8,2,34359738368,true);
         INSERT INTO character_mail(id,sender_id,recipient_id,subject,body,draft,attachment_kind,attachment_quantity,attachment_token,return_of,sender_deleted) VALUES
            (4,2,1,'back','b','00000000000000000000000000000003',8,2,34359738368,3,true);
         INSERT INTO alliances(id,name,leader_id) VALUES(1,'Old',2);
         INSERT INTO alliance_members(character_id,alliance_id) VALUES(2,1);
         INSERT INTO alliance_storage(alliance_id,slot,kind,quantity) VALUES(1,0,9,4);",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    for migration in [
        include_str!("../migrations/0016_human_characters.sql"),
        include_str!("../migrations/0017_remove_mail_storage_professions.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&mut *tx).await.unwrap();
    }
    // The old elven mage becomes a human: same id, name and coins, human base, both starter weapons.
    assert_eq!(
        mortown_server::store::convert_legacy_characters(&mut tx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        mortown_server::store::convert_legacy_characters(&mut tx)
            .await
            .unwrap(),
        0
    );
    let (nickname, stats, inventory): (String, serde_json::Value, serde_json::Value) =
        sqlx::query_as("SELECT nickname,stats,inventory FROM characters WHERE id=1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(nickname, "LegacyHero");
    assert_eq!(stats["coins"], 1234.);
    assert_eq!(stats["STR"], 10.);
    assert_eq!(stats["stat_point"], 5.);
    let kinds: Vec<u64> = inventory
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["kind"].as_u64().unwrap())
        .collect();
    assert_eq!(kinds, [2, 3, 1]);
    let archived: serde_json::Value = sqlx::query_scalar(
        "SELECT data FROM legacy_character_archive WHERE character_id=1 AND migration='0016_human_characters' AND converted_at IS NOT NULL",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        (archived["race"].as_u64(), archived["class"].as_u64()),
        (Some(4), Some(2))
    );
    let columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns WHERE table_schema='mortown_migration07_test' AND table_name='characters' AND column_name IN ('race','class')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(columns, 0);
    assert!(
        !stats
            .as_object()
            .unwrap()
            .contains_key("craft_specialization")
    );
    assert!(!stats.as_object().unwrap().contains_key("craft_xp_smithing"));
    assert_eq!(stats["craft_count"], 3.);
    let profession: serde_json::Value = sqlx::query_scalar(
        "SELECT data FROM legacy_character_archive WHERE character_id=1 AND migration='0017_remove_mail_storage_professions'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        profession,
        serde_json::json!({"craft_specialization":2,"craft_xp_smithing":75})
    );
    // Every unreleased item and payment is kept once, with its original owner.
    type Kept = (
        String,
        i64,
        Option<i64>,
        Option<i64>,
        Option<i16>,
        Option<i32>,
        Option<i64>,
    );
    let kept: Vec<Kept> = sqlx::query_as(
        "SELECT source,source_id,character_id,alliance_id,kind,quantity,coins FROM legacy_property WHERE released_at IS NULL ORDER BY source,source_id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        kept,
        vec![
            (
                "alliance_storage".into(),
                1,
                None,
                Some(1),
                Some(9),
                Some(4),
                None
            ),
            (
                "mail_attachment".into(),
                1,
                Some(1),
                None,
                Some(5),
                Some(1),
                None
            ),
            (
                "mail_attachment".into(),
                4,
                Some(1),
                None,
                Some(8),
                Some(2),
                None
            ),
            (
                "mail_payment".into(),
                2,
                Some(2),
                None,
                None,
                None,
                Some(10)
            ),
        ]
    );
    for id in [1, 2] {
        assert!(
            mortown_server::legacy::character_owns(&mut tx, id)
                .await
                .unwrap()
        );
    }
    assert!(
        mortown_server::legacy::alliance_owns(&mut tx, 1)
            .await
            .unwrap()
    );
    // Even after its old mail is erased, an owner of kept property cannot be deleted.
    let mut attempt = Connection::begin(&mut *tx).await.unwrap();
    sqlx::raw_sql(
        "DELETE FROM mail_payments; DELETE FROM character_mail WHERE return_of IS NOT NULL; DELETE FROM character_mail;",
    )
    .execute(&mut *attempt)
    .await
    .unwrap();
    assert!(
        sqlx::query("DELETE FROM characters WHERE id=1")
            .execute(&mut *attempt)
            .await
            .is_err()
    );
    attempt.rollback().await.unwrap();
    // Stage 4: every old item, coin and kept record becomes exactly one physical stack.
    let total = |stacks: &[(i16, i64)], kind: i16| -> i64 {
        stacks
            .iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, q)| q)
            .sum()
    };
    let old_kinds: Vec<u64> = kinds.clone();
    // The server runs each migration in its own transaction; flush the fixture's deferred
    // alliance checks as that commit would.
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0018_physical_items.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    // Current inventory statements read mass_mg. Old rows stay null and keep catalogue weight.
    sqlx::raw_sql(include_str!("../migrations/0024_casting.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let world = mortown_world::World::new();
    assert_eq!(
        mortown_server::store::convert_legacy_items(&mut tx, &world)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        mortown_server::store::convert_legacy_items(&mut tx, &world)
            .await
            .unwrap(),
        0
    );
    let stacks: Vec<(i16, i64)> =
        sqlx::query_as("SELECT kind, quantity::bigint FROM items ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    // Before: hero's kinds 2, 3, 1; mail 1 (kind 5 ×1) and returned mail 4 (kind 8 ×2) for
    // the hero; alliance warehouse kind 9 ×4; coins 1234 + unpaid payment 10.
    for kind in old_kinds {
        assert!(total(&stacks, kind as i16) >= 1, "kind {kind}");
    }
    assert_eq!(total(&stacks, 5), 1);
    assert_eq!(total(&stacks, 8), 2);
    assert_eq!(total(&stacks, 9), 4);
    assert_eq!(total(&stacks, items::KEY as i16), 1);
    let coins: i64 = stacks
        .iter()
        .filter_map(|(k, q)| items::coin_value(*k as u8).map(|v| v as i64 * q))
        .sum();
    assert_eq!(coins, 1234 + 10);
    let hero: (serde_json::Value, serde_json::Value, serde_json::Value) =
        sqlx::query_as("SELECT stats, inventory, loot FROM characters WHERE id=1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(!hero.0.as_object().unwrap().contains_key("coins"));
    assert_eq!(
        (hero.1, hero.2),
        (serde_json::json!([]), serde_json::json!({}))
    );
    let purse: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity * CASE kind WHEN 12 THEN 10000 WHEN 27 THEN 100 ELSE 1 END),0)::bigint FROM items WHERE character_id=1 AND place='purse'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(purse, 1234);
    // The warehouse is a locked chest; its only key is in the leader's bag.
    let (container, lock): (i64, i64) = sqlx::query_as("SELECT id, lock_code FROM world_objects")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let key: (Option<i64>, String, Option<i64>) =
        sqlx::query_as("SELECT character_id, place, lock_code FROM items WHERE kind=$1")
            .bind(items::KEY as i16)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(key, (Some(2), "bag".into(), Some(lock)));
    let stored: i64 = sqlx::query_scalar(
        "SELECT sum(quantity)::bigint FROM items WHERE container_id=$1 AND kind=9",
    )
    .bind(container)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(stored, 4);
    let unreleased: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM legacy_property WHERE released_at IS NULL OR released_to IS NULL",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(unreleased, 0);
    let archived: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM legacy_character_archive WHERE migration='0018_physical_items' AND converted_at IS NOT NULL AND data ? 'inventory'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(archived, 2);
    let items_before: i64 = sqlx::query_scalar("SELECT count(*) FROM items")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0019_progression.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        mortown_server::store::convert_legacy_progress(&mut tx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        mortown_server::store::convert_legacy_progress(&mut tx)
            .await
            .unwrap(),
        0
    );
    let hero_stats: serde_json::Value =
        sqlx::query_scalar("SELECT stats FROM characters WHERE id=1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(!hero_stats.as_object().unwrap().contains_key("sp"));
    assert!(hero_stats.as_object().unwrap().contains_key("talent_point"));
    let items_after: i64 = sqlx::query_scalar("SELECT count(*) FROM items")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(items_before, items_after);
    // Released property no longer protects its old owner from deletion.
    let mut attempt = Connection::begin(&mut *tx).await.unwrap();
    sqlx::raw_sql(
        "DELETE FROM mail_payments; DELETE FROM character_mail WHERE return_of IS NOT NULL; DELETE FROM character_mail; DELETE FROM characters WHERE id=1;",
    )
    .execute(&mut *attempt)
    .await
    .unwrap();
    attempt.rollback().await.unwrap();
    tx.rollback().await.unwrap();
}

async fn equipment_reply(s: &mut Socket) -> ServerMessage {
    loop {
        let message = receive(s).await;
        if !matches!(message, ServerMessage::Snapshot { .. }) {
            return message;
        }
    }
}
#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn equipment_is_persistent_idempotent_and_isolated_per_character() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        % 1_000_000_000;
    let name = format!("eq{unique}");
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let chars = list(&mut s).await;
    let id = chars[0].id;
    let mut kit: Vec<u8> = chars[0]
        .items
        .iter()
        .filter(|s| s.carried())
        .map(|s| s.kind)
        .collect();
    kit.sort();
    assert_eq!(kit, items::STARTER);
    assert_eq!(
        items::purse_value(&chars[0].items),
        mortown_protocol::starting_coins()
    );
    enter(&mut s, id).await;
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 17,
            equipped: true,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    // No class: the same human may hold the sword and then switch to the staff.
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 1,
            equipped: true,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
        panic!("missing equipment update")
    };
    let bonus = items::effective_bonus(1, &chars[0].stats);
    assert!(bonus < 6.);
    assert!((character.stats["f_attack"] - (chars[0].stats["f_attack"] + bonus)).abs() < 1e-6);
    for _ in 0..2 {
        send(
            &mut s,
            ClientMessage::SetEquipment {
                item: 2,
                equipped: true,
            },
        )
        .await;
        let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
            panic!("missing equipment update")
        };
        let bonus = items::effective_bonus(2, &chars[0].stats);
        assert!(bonus < 6.);
        assert!((character.stats["m_attack"] - (chars[0].stats["m_attack"] + bonus)).abs() < 1e-6);
        assert_eq!(worn(&character), [2]);
    }
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 3,
            equipped: true,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
        panic!("missing armor update")
    };
    let bonus = items::effective_bonus(3, &chars[0].stats);
    assert!(bonus < 4.);
    assert!((character.stats["f_defence"] - (chars[0].stats["f_defence"] + bonus)).abs() < 1e-6);
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 3,
            equipped: false,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
        panic!("missing armor update")
    };
    assert_eq!(character.stats["f_defence"], chars[0].stats["f_defence"]);
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    send(&mut s, creation(&format!("{name}b"))).await;
    let chars = list(&mut s).await;
    assert!(worn(&chars[1]).is_empty());
    assert_eq!(chars[1].stats, mortown_protocol::starting_stats().unwrap());
    send(&mut s, ClientMessage::Logout).await;
    drop(s);
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    assert_eq!(worn(&chars[0]), [2]);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 2,
            equipped: false,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
        panic!("missing equipment update")
    };
    assert_eq!(
        base(&character.stats),
        mortown_protocol::starting_stats().unwrap()
    );
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn combat_resources_and_death_persist_through_reconnect() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        % 1_000_000_000;
    let name = format!("fight{unique}");
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let chars = list(&mut s).await;
    let id = chars[0].id;
    let m = server.simulation.lock().await.monsters()[0].clone();
    // Position only this test fixture beside the camp; clients cannot set coordinates.
    sqlx::query("UPDATE characters SET x=$1,z=$2 WHERE id=$3")
        .bind(m.position[0])
        .bind(m.position[2])
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    enter(&mut s, id).await;
    // Without a class, the staff makes attacks and skills magical (paid with mana).
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 2,
            equipped: true,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::CharacterUpdated { .. }
    ));
    for _ in 0..10 {
        send(&mut s, ClientMessage::Attack { target: Some(m.id) }).await;
    }
    let fighting = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let ServerMessage::Snapshot { actors, .. } = receive(&mut s).await {
                let a = actors.into_iter().find(|a| a.id == id).unwrap();
                if a.mana < chars[0].stats["current_mana"] {
                    break a;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(fighting.mana, chars[0].stats["current_mana"] - 2.);
    send(&mut s, ClientMessage::UseSkill { target: m.id }).await;
    let casting = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let ServerMessage::Snapshot { actors, .. } = receive(&mut s).await {
                let a = actors.into_iter().find(|a| a.id == id).unwrap();
                if a.skill_cooldown_ms > 0 {
                    break a;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(casting.mana, fighting.mana - 5.);
    send(&mut s, ClientMessage::UseSkill { target: m.id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::LeaveWorld).await;
    let roster = list(&mut s).await;
    assert_eq!(roster[0].stats["current_mana"], casting.mana);
    // A persisted death must not be healed by re-entering the world.
    sqlx::query("UPDATE characters SET stats=jsonb_set(stats,'{current_hp}','0') WHERE id=$1")
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome { actor, .. } = equipment_reply(&mut s).await else {
        panic!("welcome expected")
    };
    assert_eq!(actor.hp, 0.);
    assert_eq!(actor.respawn_seconds, 5);
    send(&mut s, ClientMessage::Respawn).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn kill_reward_level_and_equipment_survive_restart_without_duplication() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "reward{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let chars = list(&mut s).await;
    let id = chars[0].id;
    let m = server.simulation.lock().await.monsters()[0].clone();
    sqlx::query(
        "UPDATE characters SET x=$1,z=$2,stats=jsonb_set(stats,'{current_exp}','75') WHERE id=$3",
    )
    .bind(m.position[0])
    .bind(m.position[2])
    .bind(id)
    .execute(&server.pool)
    .await
    .unwrap();
    stock(&server.pool, id, &[(5, 1), (5, 1)], None).await;
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    server.simulation.lock().await.combat.mobs[0].view.hp = 1.;
    send(&mut s, ClientMessage::Attack { target: Some(m.id) }).await;
    // Drops and coins become physical stacks stored before the owner hears about them.
    let (winner, rewarded) = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::EconomyUpdated { character, actor } = receive(&mut s).await
                && actor.progress.kills == 1
            {
                break (actor, character);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(carried(&rewarded, 7), 1);
    assert!(carried(&rewarded, 5) >= 2);
    assert_eq!(winner.progress.level, 2);
    assert_eq!(
        items::purse_value(&rewarded.items),
        items::purse_value(&chars[0].items) + 12
    );
    // Reward is committed before it is sent to its owner.
    let saved_stats: serde_json::Value =
        sqlx::query_scalar("SELECT stats FROM characters WHERE id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(saved_stats["level"], 2.);
    assert_eq!(saved_stats["kills"], 1.);
    let saved_cloth: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=7",
    )
    .bind(id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(saved_cloth, 1);
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 7,
            equipped: true,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 5,
            equipped: true,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
        panic!("equipment reply expected")
    };
    let attack = character.stats["m_attack"];
    let bonus = items::effective_bonus(5, &chars[0].stats);
    assert!(bonus < 10.);
    assert!(attack >= chars[0].stats["m_attack"] + bonus);
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    assert_eq!(carried(&chars[0], 5), carried(&rewarded, 5));
    assert_eq!(worn(&chars[0]), [5]);
    assert_eq!(chars[0].stats["m_attack"], attack);
    assert_eq!(
        items::purse_value(&chars[0].items),
        items::purse_value(&rewarded.items)
    );
    assert_eq!(chars[0].stats["current_exp"], 0.);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome { actor, .. } = equipment_reply(&mut s).await else {
        panic!("welcome expected")
    };
    assert_eq!(actor.progress, winner.progress);
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn potions_persist_and_removed_services_are_rejected() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "potion{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    sqlx::query("UPDATE characters SET stats=stats || '{\"current_hp\":10}'::jsonb WHERE id=$1")
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    stock(&server.pool, id, &[(8, 2), (7, 2)], Some(20)).await;
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    for ok in [true, true, false] {
        send(&mut s, ClientMessage::UsePotion).await;
        let result = equipment_reply(&mut s).await;
        assert_eq!(
            matches!(result, ServerMessage::EconomyUpdated { .. }),
            ok,
            "{result:?}"
        );
    }
    // Removed NPC trade, quests, mail, alliance warehouse and professions are invalid messages:
    // the server ends the session instead of moving items or coins through them.
    for removed in [
        r#"{"type":"trade","npc":1,"item":7,"buy":false,"sale":null}"#,
        r#"{"type":"quest","quest":1,"complete":false}"#,
        r#"{"type":"mail","command":{"action":"claim","id":1,"price":0}}"#,
        r#"{"type":"specialize","profession":"smithing","expected":0}"#,
        r#"{"type":"craft","recipe":10,"expected":0,"specialization_revision":0}"#,
        r#"{"type":"alliance","command":{"action":"withdraw","alliance":1,"revision":0,"entry":1,"quantity":1}}"#,
        r#"{"type":"learn_skill","rank":2}"#,
        r#"{"type":"learn_secondary"}"#,
        r#"{"type":"learn_ability","skill":3}"#,
        // Old copy-token exchange offer: items are now referred to by stack id.
        r#"{"type":"exchange","command":{"action":"offer","session":1,"revision":0,"item":{"token":4294967296,"quantity":1},"coins":5}}"#,
    ] {
        s.send(Message::Text(removed.into())).await.unwrap();
        loop {
            match receive(&mut s).await {
                ServerMessage::Error { fatal: true, .. } => break,
                ServerMessage::EconomyUpdated { .. } => panic!("removed service changed state"),
                _ => {}
            }
        }
        saved(&mut s).await;
        s = connect(&address).await;
        send(&mut s, login(&name, "test-password-06")).await;
        list(&mut s).await;
        send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
        assert!(matches!(
            equipment_reply(&mut s).await,
            ServerMessage::Welcome { .. }
        ));
    }
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    assert_eq!(items::purse_value(&chars[0].items), 20);
    assert!(!chars[0].stats.contains_key("coins"));
    assert_eq!(
        chars[0].stats["current_hp"],
        (10. + 2. * mortown_protocol::items::POTION_HEAL).min(chars[0].stats["hp"])
    );
    assert_eq!(carried(&chars[0], 7), 2);
    assert_eq!(carried(&chars[0], 8), 0);
    assert!(!chars[0].stats.contains_key("quest_stage"));
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn points_talents_practice_and_craft_survive_restart() {
    use mortown_protocol::crafting::{CUIRASS_RECIPE, LEATHER};
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "rank{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    send(&mut s, creation(&format!("{name}b"))).await;
    let chars = list(&mut s).await;
    let other = chars.iter().find(|c| c.id != id).unwrap().id;
    sqlx::query(
        "UPDATE characters SET stats=stats || '{\"talent_point\":1}'::jsonb WHERE id=$1 OR id=$2",
    )
    .bind(id)
    .bind(other)
    .execute(&server.pool)
    .await
    .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    equipment_reply(&mut s).await;
    send(
        &mut s,
        ClientMessage::SpendAttribute {
            stat: "STR".into(),
            expected: 10,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
        panic!("attribute")
    };
    assert_eq!(character.stats["STR"], 11.);
    send(
        &mut s,
        ClientMessage::SpendTalent {
            talent: "arms".into(),
            expected: 0,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
        panic!("talent")
    };
    assert_eq!(character.stats["talent_arms"], 1.);
    assert_eq!(character.stats["talent_point"], 0.);
    send(
        &mut s,
        ClientMessage::SpendTalent {
            talent: "arms".into(),
            expected: 0,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    send(
        &mut s,
        ClientMessage::SelectCharacter {
            character_id: other,
        },
    )
    .await;
    equipment_reply(&mut s).await;
    send(
        &mut s,
        ClientMessage::SpendAttribute {
            stat: "CON".into(),
            expected: 10,
        },
    )
    .await;
    equipment_reply(&mut s).await;
    send(
        &mut s,
        ClientMessage::SpendTalent {
            talent: "craft".into(),
            expected: 0,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
        panic!("other talent")
    };
    assert_eq!(character.stats["CON"], 11.);
    assert_eq!(character.stats["talent_craft"], 1.);
    assert!(!character.stats.contains_key("talent_arms"));
    assert_eq!(character.stats["STR"], 10.);
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    stock(
        &server.pool,
        id,
        &[(CUIRASS_RECIPE, 1), (LEATHER, 6), (7, 4)],
        None,
    )
    .await;
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    equipment_reply(&mut s).await;
    send(
        &mut s,
        ClientMessage::Craft {
            recipe: CUIRASS_RECIPE,
            expected: 0,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated {
        character, actor, ..
    } = equipment_reply(&mut s).await
    else {
        panic!("craft")
    };
    assert_eq!(
        character.stats["current_exp"],
        f64::from(mortown_protocol::progression::craft_experience())
    );
    assert!(!character.stats.contains_key("talent_craft"));
    assert_eq!(actor.progress.talent_points, 0.);
    let target;
    {
        let mut simulation = server.simulation.lock().await;
        let mob = simulation.monsters()[0].clone();
        target = mob.id;
        let mut hero = simulation.actor(id).unwrap();
        hero.position = mob.position;
        simulation.replace_actor(hero);
    }
    send(&mut s, ClientMessage::UseSkill { target }).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let ServerMessage::Snapshot { actors, .. } = receive(&mut s).await
                && actors
                    .iter()
                    .any(|a| a.id == id && a.progress.practice[0] == 1)
            {
                break;
            }
        }
    })
    .await
    .expect("practice");
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    let hero = chars.iter().find(|c| c.id == id).unwrap();
    let second = chars.iter().find(|c| c.id == other).unwrap();
    assert_eq!(hero.stats["STR"], 11.);
    assert_eq!(hero.stats["talent_arms"], 1.);
    assert_eq!(hero.stats["practice_1"], 1.);
    assert!(!hero.stats.contains_key("sp"));
    assert_eq!(second.stats["CON"], 11.);
    assert_eq!(second.stats["talent_craft"], 1.);
    assert!(!second.stats.contains_key("talent_arms"));
    assert_eq!(second.stats["STR"], 10.);
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn attributes_rebase_growth_equipment_and_survive_restart() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "attr{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    sqlx::query("UPDATE characters SET stats=stats || '{\"level\":6,\"stat_point\":2,\"current_hp\":50}'::jsonb WHERE id=$1")
        .bind(id).execute(&server.pool).await.unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome {
        character: before,
        actor: old_actor,
        ..
    } = equipment_reply(&mut s).await
    else {
        panic!("welcome")
    };
    let mut expected = before.stats.clone();
    for key in ["CON", "INT_"] {
        let old = before.stats[key] as u32;
        send(
            &mut s,
            ClientMessage::SpendAttribute {
                stat: key.into(),
                expected: old,
            },
        )
        .await;
        let ServerMessage::EconomyUpdated { character, actor } = equipment_reply(&mut s).await
        else {
            panic!("allocation")
        };
        assert_eq!(character.stats[key], old as f64 + 1.);
        if key == "CON" {
            assert_eq!(actor.hp - old_actor.hp, actor.max_hp - old_actor.max_hp);
            assert!(actor.max_hp > old_actor.max_hp);
            assert_eq!(actor.progress.stat_points, 1.);
        } else {
            assert_eq!(actor.progress.stat_points, 0.);
        }
        expected = character.stats;
        send(
            &mut s,
            ClientMessage::SpendAttribute {
                stat: key.into(),
                expected: old,
            },
        )
        .await;
        assert!(matches!(
            equipment_reply(&mut s).await,
            ServerMessage::Error { fatal: false, .. }
        ));
    }
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    for key in [
        "CON",
        "INT_",
        "hp",
        "mana",
        "m_attack",
        "f_defence",
        "stat_point",
    ] {
        assert_eq!(chars[0].stats[key], expected[key], "{key}");
    }
    send(&mut s, creation(&format!("{name}b"))).await;
    let chars = list(&mut s).await;
    assert_eq!(chars[1].stats["CON"], before.stats["CON"]);
    assert_eq!(
        chars[1].stats["stat_point"],
        mortown_protocol::starting_stats().unwrap()["stat_point"]
    );
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn hotbar_persists_per_character_and_rejects_invalid_bindings() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "bar{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    equipment_reply(&mut s).await;
    let potion = mortown_protocol::layout::kind_token(items::POTION) as f64;
    for (slot, binding, token) in [(0, 0, 0.), (9, 108, potion), (11, 1, 1.)] {
        send(&mut s, ClientMessage::BindHotbar { slot, binding }).await;
        let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
            panic!("binding expected")
        };
        assert_eq!(character.stats[&format!("hotbar_{slot}")], token);
    }
    // Unique items (the staff) and coins cannot be bound by kind.
    for (slot, binding) in [(12, 1), (0, 99), (255, 255), (0, 102), (0, 111)] {
        send(&mut s, ClientMessage::BindHotbar { slot, binding }).await;
        assert!(matches!(
            equipment_reply(&mut s).await,
            ServerMessage::Error { fatal: false, .. }
        ));
    }
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 2,
            equipped: true,
        },
    )
    .await;
    equipment_reply(&mut s).await;
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    assert_eq!(chars[0].stats["hotbar_0"], 0.);
    assert_eq!(chars[0].stats["hotbar_9"], potion);
    assert_eq!(chars[0].stats["hotbar_11"], 1.);
    assert_eq!(worn(&chars[0]), [2]);
    send(&mut s, creation(&format!("{name}b"))).await;
    let chars = list(&mut s).await;
    assert!(!chars[1].stats.keys().any(|k| k.starts_with("hotbar_")));
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn inventory_layout_stacks_and_swaps_survive_restart() {
    use mortown_protocol::layout::{self, Place};
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "grid{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    stock(
        &server.pool,
        id,
        &[(5, 1), (5, 1), (5, 1), (6, 1), (6, 1), (8, 5)],
        None,
    )
    .await;
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome {
        character: mut c, ..
    } = equipment_reply(&mut s).await
    else {
        panic!("welcome")
    };
    let carved = c.items.iter().filter(|s| s.kind == 5).nth(1).unwrap();
    let staff = layout::item_token(carved.id);
    let Location::Bag { cell } = carved.location else {
        panic!("staff in the bag")
    };
    let pos = Place::Bag(cell);
    let stacks = layout::bag(&c).len();
    for (source, target, expected) in [
        (pos, Place::Bag(29), staff),
        (Place::Bag(29), Place::Model, staff),
        (Place::Equipment(true), Place::Bar(10), staff),
        (Place::Skill(1), Place::Bar(0), 1),
        (Place::Bar(0), Place::Bar(10), 1),
    ] {
        send(
            &mut s,
            ClientMessage::Arrange {
                source,
                target,
                expected,
            },
        )
        .await;
        let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
            panic!("arrange")
        };
        c = character;
    }
    assert_eq!(layout::bag(&c).len(), stacks - 1);
    assert_eq!(layout::at(&c, Place::Bag(29)), 0);
    assert_eq!(layout::equipped(&c, true), staff);
    assert_eq!(layout::at(&c, Place::Bar(0)), staff);
    assert_eq!(layout::at(&c, Place::Bar(10)), 1);
    send(
        &mut s,
        ClientMessage::Arrange {
            source: Place::Bag(29),
            target: Place::Bag(20),
            expected: staff,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    assert_eq!(layout::bag(&chars[0]), layout::bag(&c));
    assert_eq!(layout::equipped(&chars[0], true), staff);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    equipment_reply(&mut s).await;
    send(
        &mut s,
        ClientMessage::Arrange {
            source: Place::Bar(0),
            target: Place::Bag(28),
            expected: staff,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
        panic!("unequip")
    };
    assert_eq!(layout::at(&character, Place::Bar(0)), 0);
    assert_eq!(layout::at(&character, Place::Bag(28)), staff);
    assert_eq!(layout::equipped(&character, true), 0);
    assert_eq!(layout::bag(&character).len(), stacks);
    assert_eq!(carried(&character, 5), 3);
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

async fn object_reply(s: &mut Socket) -> mortown_protocol::objects::Reply {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match receive(s).await {
                ServerMessage::Object { reply } => return reply,
                ServerMessage::Error { message, .. } => panic!("object request failed: {message}"),
                _ => {}
            }
        }
    })
    .await
    .expect("object reply timeout")
}
async fn act(
    s: &mut Socket,
    command: mortown_protocol::objects::Command,
) -> mortown_protocol::objects::Reply {
    // Stay under the server's budget of 8 item actions per second.
    tokio::time::sleep(Duration::from_millis(150)).await;
    send(s, ClientMessage::Objects { command }).await;
    object_reply(s).await
}
async fn exchange_view(
    s: &mut Socket,
    ready: impl Fn(&mortown_protocol::exchange::View) -> bool,
) -> mortown_protocol::exchange::View {
    let mut last = String::new();
    let found = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match receive(s).await {
                ServerMessage::Exchange { reply } => match reply.view {
                    Some(view) if ready(&view) => return view,
                    Some(view) => last = format!("{view:?} / {}", reply.message),
                    None => panic!("exchange closed: {}", reply.message),
                },
                ServerMessage::Error { message, .. } => last = message,
                _ => {}
            }
        }
    })
    .await;
    found.unwrap_or_else(|_| panic!("exchange view timeout; last: {last}"))
}
async fn exchange_done(s: &mut Socket) -> Character {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let ServerMessage::Exchange { reply } = receive(s).await {
                if let Some(update) = reply.update {
                    return update.character;
                }
                assert!(
                    reply.view.is_some(),
                    "exchange cancelled: {}",
                    reply.message
                );
            }
        }
    })
    .await
    .expect("exchange result timeout")
}
/// One full direct exchange: `a` invites `b`; each side offers its part and both confirm.
async fn trade(
    a: &mut Socket,
    b: &mut Socket,
    b_id: i64,
    give: Option<mortown_protocol::transfer::ItemRequest>,
    get: Option<mortown_protocol::transfer::ItemRequest>,
    coins: [u64; 2],
) -> (Character, Character) {
    use mortown_protocol::exchange::Command;
    let exchange = |command| ClientMessage::Exchange { command };
    send(a, exchange(Command::Invite { player: b_id })).await;
    let v = exchange_view(b, |v| v.invited && !v.accepted).await;
    send(
        b,
        exchange(Command::Accept {
            session: v.session,
            revision: v.revision,
        }),
    )
    .await;
    let v = exchange_view(a, |v| v.accepted).await;
    send(
        a,
        exchange(Command::Offer {
            session: v.session,
            revision: v.revision,
            item: give,
            coins: coins[0],
        }),
    )
    .await;
    let seen = v.revision;
    let v = exchange_view(b, |v| {
        v.revision > seen && v.other.item == give && v.other.coins == coins[0]
    })
    .await;
    send(
        b,
        exchange(Command::Offer {
            session: v.session,
            revision: v.revision,
            item: get,
            coins: coins[1],
        }),
    )
    .await;
    let seen = v.revision;
    let v = exchange_view(a, |v| {
        v.revision > seen && v.other.item == get && v.other.coins == coins[1]
    })
    .await;
    send(
        a,
        exchange(Command::Confirm {
            session: v.session,
            revision: v.revision,
        }),
    )
    .await;
    let v = exchange_view(b, |v| v.other.confirmed).await;
    send(
        b,
        exchange(Command::Confirm {
            session: v.session,
            revision: v.revision,
        }),
    )
    .await;
    (exchange_done(a).await, exchange_done(b).await)
}
/// A walkable spot outside halls with no placed object nearby (the dev database keeps chests
/// from other runs).
async fn free_spot(server: &Server) -> [f32; 2] {
    let world = server.simulation.lock().await.terrain();
    let placed: Vec<(f32, f32)> = sqlx::query_as("SELECT x, z FROM world_objects")
        .fetch_all(&server.pool)
        .await
        .unwrap();
    for ring in 1..200 {
        for step in 0..24 {
            let angle = step as f32 * std::f32::consts::TAU / 24.;
            let r = 6. + ring as f32 * 3.;
            let p = [
                mortown_world::SPAWN[0] + r * angle.cos(),
                mortown_world::SPAWN[1] + r * angle.sin(),
            ];
            let q = [p[0] + 2., p[1]];
            if [p, q]
                .iter()
                .all(|p| world.walkable(p[0], p[1]) && world.hall_at(p[0], p[1]).is_none())
                && world.segment_clear(p, q)
                && placed
                    .iter()
                    .all(|o| mortown_world::distance([o.0, o.1], p) > 8.)
            {
                return p;
            }
        }
    }
    panic!("no free spot")
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn physical_items_chest_key_exchange_and_restart_keep_one_instance() {
    use mortown_protocol::{objects::Command, transfer::ItemRequest};
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        % 1_000_000_000;
    let (a_name, b_name) = (format!("own{unique}a"), format!("own{unique}b"));
    let mut ids = vec![];
    let mut sockets = vec![];
    for name in [&a_name, &b_name] {
        register(&address, name).await;
        let mut s = connect(&address).await;
        send(&mut s, login(name, "test-password-06")).await;
        list(&mut s).await;
        send(&mut s, creation(name)).await;
        ids.push(list(&mut s).await[0].id);
        sockets.push(s);
    }
    let (a, b) = (ids[0], ids[1]);
    let [mut sa, mut sb]: [Socket; 2] = sockets.try_into().ok().unwrap();
    let spot = free_spot(&server).await;
    // Joining players keep 1.5 m apart; 2 m is still within reach of the chest at `spot`.
    for (id, dx) in [(a, 0.), (b, 2.)] {
        sqlx::query("UPDATE characters SET x=$1, z=$2 WHERE id=$3")
            .bind(spot[0] + dx)
            .bind(spot[1])
            .bind(id)
            .execute(&server.pool)
            .await
            .unwrap();
    }
    stock(&server.pool, a, &[(items::CHEST, 1)], None).await;
    let ground = |kind: i16, quantity: i32| {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO items(kind,quantity,place,x,z) VALUES($1,$2,'ground',$3,$4) RETURNING id",
        )
        .bind(kind)
        .bind(quantity)
        .bind(spot[0])
        .bind(spot[1])
        .fetch_one(&server.pool)
    };
    let thing = ground(6, 1).await.unwrap();
    let heavy = ground(13, 200).await.unwrap();
    for (s, id) in [(&mut sa, a), (&mut sb, b)] {
        send(s, ClientMessage::SelectCharacter { character_id: id }).await;
        assert!(matches!(
            equipment_reply(s).await,
            ServerMessage::Welcome { .. }
        ));
    }
    let place = |id: i64| {
        sqlx::query_as::<_, (String, Option<i64>, Option<i64>)>(
            "SELECT place, character_id, container_id FROM items WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&server.pool)
    };

    // Set the chest down first: the cuirass alone fits under the carry limit.
    let chest =
        sqlx::query_scalar::<_, i64>("SELECT id FROM items WHERE character_id=$1 AND kind=$2")
            .bind(a)
            .bind(items::CHEST as i16)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    let reply = act(&mut sa, Command::Place { item: chest }).await;
    let hero = reply.update.expect("placed").character;
    let key = *hero.items.iter().find(|s| s.kind == items::KEY).unwrap();
    let (container, lock): (i64, i64) =
        sqlx::query_as("SELECT id, lock_code FROM world_objects WHERE placed_by=$1")
            .bind(a)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(key.lock, Some(lock));
    assert!(
        sqlx::query("SELECT 1 FROM items WHERE id=$1")
            .bind(chest)
            .fetch_optional(&server.pool)
            .await
            .unwrap()
            .is_none()
    );

    // Pick up once; the replayed request finds nothing.
    let reply = act(&mut sa, Command::PickUp { item: thing }).await;
    assert!(reply.update.is_some(), "{}", reply.message);
    assert_eq!(place(thing).await.unwrap(), ("bag".into(), Some(a), None));
    let replay = act(&mut sa, Command::PickUp { item: thing }).await;
    assert!(replay.update.is_none());
    assert!(
        replay.message.contains("уже подобрали"),
        "{}",
        replay.message
    );
    assert_eq!(place(thing).await.unwrap(), ("bag".into(), Some(a), None));

    // Overflow: the heavy pile is refused and stays where it was (the transaction rolls back).
    let refused = act(&mut sb, Command::PickUp { item: heavy }).await;
    assert!(refused.update.is_none());
    assert!(refused.message.contains("тяжело"), "{}", refused.message);
    assert_eq!(place(heavy).await.unwrap(), ("ground".into(), None, None));

    // Into the chest, with coins; a stale revision is refused.
    let view = act(&mut sa, Command::Open { container })
        .await
        .view
        .unwrap();
    assert!(view.items.is_empty());
    let put = Command::Put {
        container,
        revision: view.revision,
        item: Some(ItemRequest {
            item: thing,
            quantity: 1,
        }),
        coins: 50,
    };
    let reply = act(&mut sa, put).await;
    let view = reply.view.expect("put");
    assert!(view.items.iter().any(|s| s.id == thing));
    assert_eq!(
        place(thing).await.unwrap(),
        ("container".into(), None, Some(container))
    );
    let stale = act(
        &mut sa,
        Command::Take {
            container,
            revision: view.revision - 1,
            item: None,
            coins: 50,
        },
    )
    .await;
    assert!(stale.update.is_none());
    assert!(stale.message.contains("изменилось"), "{}", stale.message);

    // Without the key the other player is refused by the server.
    let locked = act(&mut sb, Command::Open { container }).await;
    assert!(locked.view.is_none() && locked.update.is_none());
    assert!(locked.message.contains("ключ"), "{}", locked.message);

    // The key changes hands by direct exchange: the same stack, now in the other bag.
    let (a_after, b_after) = trade(
        &mut sa,
        &mut sb,
        b,
        Some(ItemRequest {
            item: key.id,
            quantity: 1,
        }),
        None,
        [0, 0],
    )
    .await;
    assert!(!a_after.items.iter().any(|s| s.id == key.id));
    assert!(b_after.items.iter().any(|s| s.id == key.id && s.carried()));
    assert!(
        act(&mut sa, Command::Open { container })
            .await
            .view
            .is_none()
    );
    let view = act(&mut sb, Command::Open { container })
        .await
        .view
        .unwrap();

    // Concurrent withdrawals of the same coins: a second key (a locksmith's copy, fixture)
    // lets both players try at the same revision; exactly one succeeds.
    sqlx::query("INSERT INTO items(kind,quantity,lock_code,place,character_id,cell) VALUES($1,1,$2,'bag',$3,40)")
        .bind(items::KEY as i16)
        .bind(lock)
        .bind(a)
        .execute(&server.pool)
        .await
        .unwrap();
    let take = || Command::Take {
        container,
        revision: view.revision,
        item: None,
        coins: 50,
    };
    send(&mut sa, ClientMessage::Objects { command: take() }).await;
    send(&mut sb, ClientMessage::Objects { command: take() }).await;
    let (ra, rb) = (object_reply(&mut sa).await, object_reply(&mut sb).await);
    assert_eq!(
        usize::from(ra.update.is_some()) + usize::from(rb.update.is_some()),
        1
    );
    let coins_left: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0)::bigint FROM items WHERE container_id=$1 AND kind IN (11,12,27)",
    )
    .bind(container)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(coins_left, 0);

    // The other player takes the cuirass and trades it back for 30 copper.
    let view = act(&mut sb, Command::Open { container })
        .await
        .view
        .unwrap();
    let reply = act(
        &mut sb,
        Command::Take {
            container,
            revision: view.revision,
            item: Some(ItemRequest {
                item: thing,
                quantity: 1,
            }),
            coins: 0,
        },
    )
    .await;
    assert!(reply.update.is_some(), "{}", reply.message);
    assert_eq!(place(thing).await.unwrap(), ("bag".into(), Some(b), None));
    let purse = |id: i64| {
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(sum(quantity * CASE kind WHEN 12 THEN 10000 WHEN 27 THEN 100 ELSE 1 END),0)::bigint FROM items WHERE character_id=$1 AND place='purse'",
        )
        .bind(id)
        .fetch_one(&server.pool)
    };
    let before = (purse(a).await.unwrap(), purse(b).await.unwrap());
    let (a_after, _) = trade(
        &mut sa,
        &mut sb,
        b,
        None,
        Some(ItemRequest {
            item: thing,
            quantity: 1,
        }),
        [30, 0],
    )
    .await;
    assert!(a_after.items.iter().any(|s| s.id == thing));
    assert_eq!(
        (purse(a).await.unwrap(), purse(b).await.unwrap()),
        (before.0 - 30, before.1 + 30)
    );

    // Connection drops right after a request: the stack ends in exactly one place.
    let view = act(&mut sa, Command::Open { container })
        .await
        .view
        .unwrap();
    send(
        &mut sa,
        ClientMessage::Objects {
            command: Command::Put {
                container,
                revision: view.revision,
                item: Some(ItemRequest {
                    item: thing,
                    quantity: 1,
                }),
                coins: 0,
            },
        },
    )
    .await;
    drop(sa);
    send(&mut sb, ClientMessage::Logout).await;
    saved(&mut sb).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;

    // After a restart there is still exactly one cuirass and the chest is where it was put.
    let (server, address, stop, task) = launch(&database).await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM items WHERE id=$1")
        .bind(thing)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    let (at, owner, inside): (String, Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT place, character_id, container_id FROM items WHERE id=$1")
            .bind(thing)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert!(
        (at == "bag" && owner == Some(a) && inside.is_none())
            || (at == "container" && owner.is_none() && inside == Some(container)),
        "{at} {owner:?} {inside:?}"
    );
    let mut s = connect(&address).await;
    send(&mut s, login(&b_name, "test-password-06")).await;
    let roster = list(&mut s).await;
    assert!(!roster[0].items.iter().any(|s| s.id == thing));
    send(&mut s, ClientMessage::SelectCharacter { character_id: b }).await;
    let listed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::Objects { containers, .. } = raw(&mut s).await {
                break containers;
            }
        }
    })
    .await
    .unwrap();
    assert!(listed.iter().any(|c| c.id == container && c.locked));
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    sqlx::raw_sql(&format!(
        "DELETE FROM items WHERE container_id={container} OR id={heavy}; DELETE FROM world_objects WHERE id={container};"
    ))
    .execute(&server.pool)
    .await
    .unwrap();
    for name in [a_name, b_name] {
        sqlx::query("DELETE FROM accounts WHERE login=$1")
            .bind(name)
            .execute(&server.pool)
            .await
            .unwrap();
    }
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}
#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn death_spends_one_life_keeps_items_and_logout_keeps_one_body() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "life{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    enter(&mut s, id).await;
    let chest: i64 = sqlx::query_scalar(
        "INSERT INTO world_objects(kind, x, z, lock_code) VALUES (28, 1, 1, 7) RETURNING id",
    )
    .fetch_one(&server.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO items(kind, quantity, lock_code, place, cell, container_id) VALUES (29, 1, 7, 'container', 0, $1)",
    )
    .bind(chest)
    .execute(&server.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO items(kind, quantity, lock_code, place, character_id, cell) VALUES (29, 1, 7, 'bag', $1, 10)",
    )
    .bind(id)
    .execute(&server.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO items(kind, quantity, place, character_id, cell) VALUES (9, 4, 'bag', $1, 11)",
    )
    .bind(id)
    .execute(&server.pool)
    .await
    .unwrap();
    {
        let mut sim = server.simulation.lock().await;
        for _ in 0..2 {
            let mut actor = sim.actor(id).unwrap();
            let tick = sim.tick();
            sim.combat.player_admin(&mut actor, true, tick);
            sim.replace_actor(actor);
        }
        assert!(sim.actor(id).unwrap().pending_death);
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let lives: f64 =
        sqlx::query_scalar("SELECT (stats->>'lives')::float8 FROM characters WHERE id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(lives, 9.);
    let goods: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=9",
    )
    .bind(id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(goods, 4);
    let carried_keys: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM items WHERE character_id=$1 AND kind=29 AND place IN ('bag','worn')",
    )
    .bind(id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(carried_keys, 0);
    let stored_keys: i64 =
        sqlx::query_scalar("SELECT count(*) FROM items WHERE container_id=$1 AND kind=29")
            .bind(chest)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(stored_keys, 1);
    {
        let mut sim = server.simulation.lock().await;
        let mut actor = sim.actor(id).unwrap();
        actor.pending_death = true;
        sim.replace_actor(actor);
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let lives_again: f64 =
        sqlx::query_scalar("SELECT (stats->>'lives')::float8 FROM characters WHERE id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(lives_again, 9.);
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    {
        let sim = server.simulation.lock().await;
        assert!(sim.actor(id).unwrap().offline);
    }
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    {
        let sim = server.simulation.lock().await;
        assert!(!sim.actor(id).unwrap().offline);
    }
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    assert_eq!(list(&mut s).await.len(), 1);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    let goods: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=9",
    )
    .bind(id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(goods, 4);
    sqlx::query("DELETE FROM items WHERE container_id=$1")
        .bind(chest)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM world_objects WHERE id=$1")
        .bind(chest)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(&name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}
#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn peaceful_work_gathers_crafts_and_sells_without_duplicating() {
    use mortown_protocol::work::{self, Command};
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    sqlx::query(
        "DELETE FROM items WHERE container_id IN (SELECT id FROM world_objects WHERE kind=37)",
    )
    .execute(&server.pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM world_objects WHERE kind=37")
        .execute(&server.pool)
        .await
        .unwrap();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        % 1_000_000_000;
    let maker_name = format!("mk{unique}");
    let buyer_name = format!("by{unique}");
    register(&address, &maker_name).await;
    register(&address, &buyer_name).await;
    let mut maker = connect(&address).await;
    send(&mut maker, login(&maker_name, "test-password-06")).await;
    list(&mut maker).await;
    send(&mut maker, creation(&maker_name)).await;
    let maker_id = list(&mut maker).await[0].id;
    sqlx::query("UPDATE characters SET stats = stats || '{\"talent_craft\":1}'::jsonb WHERE id=$1")
        .bind(maker_id)
        .execute(&server.pool)
        .await
        .unwrap();
    stock(
        &server.pool,
        maker_id,
        &[
            (work::WOOD, 4),
            (items::IRON, 6),
            (work::GRAIN, 3),
            (work::GOLD_ORE, 1),
            (work::SALT, 1),
            (work::WATER, 1),
        ],
        None,
    )
    .await;
    let entered = enter(&mut maker, maker_id).await;
    let at = [entered.position[0] + 15., entered.position[2]];
    {
        let mut sim = server.simulation.lock().await;
        sim.pin(maker_id, at);
    }
    let node: i64 = sqlx::query_scalar(
        "INSERT INTO resource_nodes(product,x,z,remaining) VALUES($1,$2,$3,1) RETURNING id",
    )
    .bind(i16::from(work::WOOD))
    .bind(at[0])
    .bind(at[1])
    .fetch_one(&server.pool)
    .await
    .unwrap();
    let purse_before: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND place='purse'",
    )
    .bind(maker_id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Gather { node },
        },
    )
    .await;
    let gathered = equipment_reply(&mut maker).await;
    assert!(
        matches!(gathered, ServerMessage::Object { reply } if reply.message.contains("Добыто"))
    );
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Gather { node },
        },
    )
    .await;
    let empty = equipment_reply(&mut maker).await;
    assert!(matches!(empty, ServerMessage::Object { reply } if reply.message.contains("иссяк")));
    let purse_after: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND place='purse'",
    )
    .bind(maker_id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(purse_before, purse_after);
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Design {
                material: items::IRON,
                geometry: 1,
                process: 1,
            },
        },
    )
    .await;
    let planned = equipment_reply(&mut maker).await;
    assert!(
        matches!(planned, ServerMessage::Object { ref reply } if reply.message.contains("горн")),
        "{planned:?}"
    );
    let iron: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=$2",
    )
    .bind(maker_id)
    .bind(i16::from(items::IRON))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(iron, 6, "the old blade command does not spend iron");
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Design {
                material: work::GOLD_ORE,
                geometry: 1,
                process: 1,
            },
        },
    )
    .await;
    let junk = equipment_reply(&mut maker).await;
    assert!(
        matches!(junk, ServerMessage::Object { ref reply } if reply.message.contains("горн")),
        "{junk:?}"
    );
    let ore: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=$2",
    )
    .bind(maker_id)
    .bind(i16::from(work::GOLD_ORE))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(ore, 1);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::MakeStall,
        },
    )
    .await;
    let made = equipment_reply(&mut maker).await;
    let ServerMessage::Object { reply } = &made else {
        panic!("{made:?}");
    };
    assert!(
        reply.message.contains("прилавок") || reply.message.contains("Прилавок"),
        "{made:?}"
    );
    let stall_item: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE character_id=$1 AND kind=$2")
            .bind(maker_id)
            .bind(i16::from(items::STALL))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    send(
        &mut maker,
        ClientMessage::Objects {
            command: mortown_protocol::objects::Command::Place { item: stall_item },
        },
    )
    .await;
    let placed = equipment_reply(&mut maker).await;
    let ServerMessage::Object { reply } = &placed else {
        panic!("{placed:?}");
    };
    assert!(reply.message.contains("поставлен"), "{placed:?}");
    let stall: i64 =
        sqlx::query_scalar("SELECT id FROM world_objects WHERE placed_by=$1 AND kind=$2")
            .bind(maker_id)
            .bind(i16::from(items::STALL))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    let (goods, goods_qty): (i64, i32) = sqlx::query_as(
        "SELECT id, quantity FROM items WHERE character_id=$1 AND kind=$2 ORDER BY id LIMIT 1",
    )
    .bind(maker_id)
    .bind(i16::from(work::SALT))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    send(
        &mut maker,
        ClientMessage::Objects {
            command: mortown_protocol::objects::Command::Put {
                container: stall,
                revision: 0,
                item: Some(mortown_protocol::transfer::ItemRequest {
                    item: goods,
                    quantity: goods_qty as u32,
                }),
                coins: 0,
            },
        },
    )
    .await;
    equipment_reply(&mut maker).await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Price {
                container: stall,
                price: 10,
            },
        },
    )
    .await;
    let priced = equipment_reply(&mut maker).await;
    let ServerMessage::Object { reply } = &priced else {
        panic!("{priced:?}");
    };
    assert!(reply.message.contains("Цена"), "{priced:?}");
    let mut buyer = connect(&address).await;
    send(&mut buyer, login(&buyer_name, "test-password-06")).await;
    list(&mut buyer).await;
    send(&mut buyer, creation(&buyer_name)).await;
    let buyer_id = list(&mut buyer).await[0].id;
    enter(&mut buyer, buyer_id).await;
    {
        let mut sim = server.simulation.lock().await;
        sim.pin(buyer_id, at);
    }
    send(
        &mut buyer,
        ClientMessage::Work {
            command: Command::Buy {
                container: stall,
                price: 10,
            },
        },
    )
    .await;
    let bought = equipment_reply(&mut buyer).await;
    let ServerMessage::Object { reply } = &bought else {
        panic!("{bought:?}");
    };
    assert!(reply.message.contains("Куплено"), "{bought:?}");
    send(
        &mut buyer,
        ClientMessage::Work {
            command: Command::Buy {
                container: stall,
                price: 10,
            },
        },
    )
    .await;
    let again = equipment_reply(&mut buyer).await;
    assert!(matches!(again, ServerMessage::Object { reply } if reply.message.contains("пуст")));
    let owned: i64 =
        sqlx::query_scalar("SELECT count(*) FROM items WHERE id=$1 AND character_id=$2")
            .bind(goods)
            .bind(buyer_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(owned, 1);
    let copies: i64 = sqlx::query_scalar("SELECT count(*) FROM items WHERE id=$1")
        .bind(goods)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(copies, 1);
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Cook,
        },
    )
    .await;
    equipment_reply(&mut maker).await;
    let loaf: i64 = sqlx::query_scalar("SELECT id FROM items WHERE character_id=$1 AND kind=$2")
        .bind(maker_id)
        .bind(i16::from(work::BREAD))
        .fetch_one(&server.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE items SET lock_code=1 WHERE id=$1")
        .bind(loaf)
        .execute(&server.pool)
        .await
        .unwrap();
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Eat { item: loaf },
        },
    )
    .await;
    let spoiled = equipment_reply(&mut maker).await;
    assert!(
        matches!(spoiled, ServerMessage::Object { reply } if reply.message.contains("испортилась"))
    );
    let loaf_left: i64 = sqlx::query_scalar("SELECT count(*) FROM items WHERE id=$1")
        .bind(loaf)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(loaf_left, 0);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let plot: i64 = sqlx::query_scalar("INSERT INTO plots(x,z) VALUES($1,$2) RETURNING id")
        .bind(at[0])
        .bind(at[1])
        .fetch_one(&server.pool)
        .await
        .unwrap();
    let grain_before: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=$2",
    )
    .bind(maker_id)
    .bind(i16::from(work::GRAIN))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Tend { plot },
        },
    )
    .await;
    equipment_reply(&mut maker).await;
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Tend { plot },
        },
    )
    .await;
    equipment_reply(&mut maker).await;
    sqlx::query("UPDATE plots SET ready_at=now() - interval '1 second' WHERE id=$1")
        .bind(plot)
        .execute(&server.pool)
        .await
        .unwrap();
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Tend { plot },
        },
    )
    .await;
    let harvested = equipment_reply(&mut maker).await;
    let ServerMessage::Object { reply } = &harvested else {
        panic!("{harvested:?}");
    };
    assert!(reply.message.contains("Собрано"), "{harvested:?}");
    let grain_after: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=$2",
    )
    .bind(maker_id)
    .bind(i16::from(work::GRAIN))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert!(grain_after > grain_before);
    sqlx::query("DELETE FROM plots WHERE id=$1")
        .bind(plot)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM items WHERE container_id=$1")
        .bind(stall)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM world_objects WHERE id=$1")
        .bind(stall)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM resource_nodes WHERE id=$1")
        .bind(node)
        .execute(&server.pool)
        .await
        .unwrap();
    for name in [&maker_name, &buyer_name] {
        sqlx::query("DELETE FROM accounts WHERE login=$1")
            .bind(name)
            .execute(&server.pool)
            .await
            .unwrap();
    }
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}
#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn timber_blocks_persist_and_the_plot_grant_is_once() {
    use mortown_protocol::work::{self, Command};
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        % 1_000_000_000;
    let name = format!("tm{unique}");
    register(&address, &name).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    list(&mut s).await;
    send(&mut s, creation(&name)).await;
    let id = list(&mut s).await[0].id;
    sqlx::query("UPDATE plots SET owner_id = NULL")
        .execute(&server.pool)
        .await
        .unwrap();
    {
        let mut conn = server.pool.acquire().await.unwrap();
        mortown_server::build::grant_starting_plots(&mut conn)
            .await
            .unwrap();
    }
    let restored: i64 = sqlx::query_scalar("SELECT count(*) FROM plots WHERE owner_id IS NOT NULL")
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(restored, 0, "a second grant must not put the plots back");
    stock(
        &server.pool,
        id,
        &[(work::WOOD, 17), (items::CHEST, 1)],
        None,
    )
    .await;
    let entered = enter(&mut s, id).await;
    let terrain = server.simulation.lock().await.terrain();
    let mut at = [entered.position[0], entered.position[2]];
    'spot: for x in -40..40 {
        for z in -40..40 {
            let feet = [
                entered.position[0] + x as f32 * 2.,
                entered.position[2] + z as f32 * 2.,
            ];
            let ahead = [feet[0], feet[1] + 1.5];
            let hauled = [feet[0], feet[1] + 3.];
            let cargo = [feet[0] + 2., feet[1]];
            let cargo_after = [feet[0] + 2., feet[1] + 3.];
            let open =
                |p: [f32; 2]| terrain.walkable(p[0], p[1]) && terrain.hall_at(p[0], p[1]).is_none();
            if open(feet) && open(ahead) && open(hauled) && open(cargo) && open(cargo_after) {
                at = feet;
                break 'spot;
            }
        }
    }
    {
        let mut sim = server.simulation.lock().await;
        sim.pin(id, at);
    }
    let pause = Duration::from_millis(200);
    send(
        &mut s,
        ClientMessage::Work {
            command: Command::MakeTimber,
        },
    )
    .await;
    let made = equipment_reply(&mut s).await;
    assert!(
        matches!(&made, ServerMessage::Object { reply } if reply.message.contains("брус")),
        "{made:?}"
    );
    tokio::time::sleep(pause).await;
    send(
        &mut s,
        ClientMessage::Work {
            command: Command::MakeTimber,
        },
    )
    .await;
    equipment_reply(&mut s).await;
    tokio::time::sleep(pause).await;
    send(
        &mut s,
        ClientMessage::Work {
            command: Command::PlaceTimber,
        },
    )
    .await;
    let placed = equipment_reply(&mut s).await;
    assert!(
        matches!(&placed, ServerMessage::Object { reply } if reply.message.contains("поставлен")),
        "{placed:?}"
    );
    let block: i64 = sqlx::query_scalar("SELECT id FROM blocks WHERE placed_by=$1")
        .bind(id)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    tokio::time::sleep(pause).await;
    send(
        &mut s,
        ClientMessage::Work {
            command: Command::PlaceTimber,
        },
    )
    .await;
    let stacked = equipment_reply(&mut s).await;
    assert!(
        matches!(&stacked, ServerMessage::Object { reply } if reply.message.contains("опору")),
        "{stacked:?}"
    );
    let upper: i64 =
        sqlx::query_scalar("SELECT count(*) FROM blocks WHERE placed_by=$1 AND level=1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(upper, 1);
    for _ in 0..4 {
        tokio::time::sleep(pause).await;
        send(
            &mut s,
            ClientMessage::Work {
                command: Command::Damage { block },
            },
        )
        .await;
        equipment_reply(&mut s).await;
    }
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM blocks WHERE placed_by=$1")
        .bind(id)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "the beam above falls with its support");
    let fallen: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE place='ground' AND kind=$1",
    )
    .bind(i16::from(work::TIMBER))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(
        fallen, 1,
        "a fallen beam is timber on the ground, the smashed one is not"
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut s,
        ClientMessage::Work {
            command: Command::MakeCart,
        },
    )
    .await;
    let cart_made = equipment_reply(&mut s).await;
    assert!(
        matches!(&cart_made, ServerMessage::Object { reply } if reply.message.contains("телега")),
        "{cart_made:?}"
    );
    let cart_item: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE character_id=$1 AND kind=$2")
            .bind(id)
            .bind(i16::from(items::CART))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    send(
        &mut s,
        ClientMessage::Objects {
            command: mortown_protocol::objects::Command::Place { item: cart_item },
        },
    )
    .await;
    equipment_reply(&mut s).await;
    let cart: i64 =
        sqlx::query_scalar("SELECT id FROM world_objects WHERE placed_by=$1 AND kind=$2")
            .bind(id)
            .bind(i16::from(items::CART))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    {
        let mut sim = server.simulation.lock().await;
        sim.pin(id, [at[0] + 2., at[1]]);
    }
    tokio::time::sleep(pause).await;
    let chest_item: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE character_id=$1 AND kind=$2")
            .bind(id)
            .bind(i16::from(items::CHEST))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    send(
        &mut s,
        ClientMessage::Objects {
            command: mortown_protocol::objects::Command::Place { item: chest_item },
        },
    )
    .await;
    equipment_reply(&mut s).await;
    let chest: i64 =
        sqlx::query_scalar("SELECT id FROM world_objects WHERE placed_by=$1 AND kind=$2")
            .bind(id)
            .bind(i16::from(items::CHEST))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    let wood: i64 = sqlx::query_scalar("SELECT id FROM items WHERE character_id=$1 AND kind=$2")
        .bind(id)
        .bind(i16::from(work::WOOD))
        .fetch_one(&server.pool)
        .await
        .unwrap();
    tokio::time::sleep(pause).await;
    send(
        &mut s,
        ClientMessage::Objects {
            command: mortown_protocol::objects::Command::Put {
                container: chest,
                revision: 0,
                item: Some(mortown_protocol::transfer::ItemRequest {
                    item: wood,
                    quantity: 1,
                }),
                coins: 0,
            },
        },
    )
    .await;
    equipment_reply(&mut s).await;
    {
        let mut sim = server.simulation.lock().await;
        let mut actor = sim.actor(id).unwrap();
        actor.yaw = 0.;
        sim.replace_actor(actor);
        sim.pin(id, at);
    }
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut s,
        ClientMessage::Work {
            command: Command::Haul,
        },
    )
    .await;
    let hauled = equipment_reply(&mut s).await;
    assert!(
        matches!(&hauled, ServerMessage::Object { reply } if reply.message.contains("Груз")),
        "{hauled:?}"
    );
    let (cart_x, cart_z): (f32, f32) = sqlx::query_as("SELECT x, z FROM world_objects WHERE id=$1")
        .bind(cart)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    let (chest_x, chest_z): (f32, f32) =
        sqlx::query_as("SELECT x, z FROM world_objects WHERE id=$1")
            .bind(chest)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert!((cart_z - (at[1] + 3.)).abs() < 0.05);
    assert!((cart_x - at[0]).abs() < 0.05);
    assert!(
        (chest_x - (at[0] + 2.)).abs() < 0.05,
        "the chest keeps its offset"
    );
    assert!((chest_z - (at[1] + 3.)).abs() < 0.05);
    let carried: i64 =
        sqlx::query_scalar("SELECT count(*) FROM items WHERE id=$1 AND container_id=$2")
            .bind(wood)
            .bind(chest)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(carried, 1);
    sqlx::query("DELETE FROM items WHERE container_id=$1 OR id=$2")
        .bind(chest)
        .bind(wood)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM items WHERE place='ground' AND kind=$1")
        .bind(i16::from(work::TIMBER))
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM world_objects WHERE id=$1 OR id=$2")
        .bind(cart)
        .bind(chest)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM accounts WHERE login=$1")
        .bind(&name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}
#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn strangers_miss_the_name_and_one_kind_of_organisation_is_enough() {
    use mortown_protocol::{alliance, society, work::Command};
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        % 1_000_000_000;
    let a_name = format!("sa{unique}");
    let b_name = format!("sb{unique}");
    register(&address, &a_name).await;
    register(&address, &b_name).await;
    let mut a = connect(&address).await;
    send(&mut a, login(&a_name, "test-password-06")).await;
    list(&mut a).await;
    send(&mut a, creation(&a_name)).await;
    let a_id = list(&mut a).await[0].id;
    let mut b = connect(&address).await;
    send(&mut b, login(&b_name, "test-password-06")).await;
    list(&mut b).await;
    send(&mut b, creation(&b_name)).await;
    let b_id = list(&mut b).await[0].id;
    stock(
        &server.pool,
        a_id,
        &[(mortown_protocol::work::WOOD, 1)],
        Some(3_000),
    )
    .await;
    let entered = enter(&mut a, a_id).await;
    enter(&mut b, b_id).await;
    let terrain = server.simulation.lock().await.terrain();
    let mut at = [entered.position[0], entered.position[2]];
    'spot: for x in -30..30 {
        for z in -30..30 {
            let feet = [
                entered.position[0] + x as f32 * 2.,
                entered.position[2] + z as f32 * 2.,
            ];
            let ahead = [feet[0], feet[1] + society::BIRD_RANGE];
            if terrain.walkable(feet[0], feet[1])
                && terrain.segment_clear(feet, ahead)
                && terrain.walkable(ahead[0], ahead[1])
            {
                at = feet;
                break 'spot;
            }
        }
    }
    {
        let mut sim = server.simulation.lock().await;
        sim.pin(a_id, at);
        sim.pin(b_id, [at[0] + 4., at[1]]);
    }
    let seen = |actors: &[Actor], expect: &str| {
        actors
            .iter()
            .find(|actor| actor.id == a_id)
            .is_some_and(|actor| actor.nickname == expect && actor.nickname != a_name)
    };
    let mut deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let message = raw(&mut b).await;
        if let ServerMessage::Snapshot { actors, .. } = &message
            && seen(actors, "Незнакомец")
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
    }
    send(
        &mut a,
        ClientMessage::Society {
            command: society::Command::Present {
                name: "Чужак".into(),
            },
        },
    )
    .await;
    deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let message = raw(&mut b).await;
        if let ServerMessage::Snapshot { actors, .. } = message
            && seen(&actors, "Чужак")
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
    }
    send(
        &mut a,
        ClientMessage::Society {
            command: society::Command::Cover { hidden: true },
        },
    )
    .await;
    deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let message = raw(&mut b).await;
        if let ServerMessage::Snapshot { actors, .. } = message
            && seen(&actors, "Закрытое лицо")
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
    }
    send(&mut a, ClientMessage::Voice { speaking: true }).await;
    deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let message = raw(&mut b).await;
        if let ServerMessage::Hearing { label, .. } = message {
            assert_ne!(label, a_name);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
    }
    {
        let mut sim = server.simulation.lock().await;
        sim.pin(b_id, [at[0] + 80., at[1]]);
    }
    send(&mut a, ClientMessage::Voice { speaking: true }).await;
    let missed = tokio::time::timeout(Duration::from_millis(400), raw(&mut b)).await;
    if let Ok(ServerMessage::Hearing { .. }) = missed {
        panic!("a voice carried past earshot");
    }
    for kind in [alliance::Kind::Guild, alliance::Kind::Order] {
        tokio::time::sleep(Duration::from_millis(1100)).await;
        send(
            &mut a,
            ClientMessage::Alliance {
                command: alliance::Command::Create {
                    name: format!("{kind:?}{unique}"),
                    kind,
                },
            },
        )
        .await;
    }
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let kinds: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT kind) FROM alliance_members WHERE character_id=$1",
    )
    .bind(a_id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(kinds, 2);
    send(
        &mut a,
        ClientMessage::Alliance {
            command: alliance::Command::Create {
                name: format!("again{unique}"),
                kind: alliance::Kind::Guild,
            },
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let still: i64 =
        sqlx::query_scalar("SELECT count(*) FROM alliances WHERE leader_id=$1 AND kind=2")
            .bind(a_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(still, 1);
    send(
        &mut a,
        ClientMessage::Work {
            command: Command::WriteDocument {
                text: "Передай это человеку".into(),
            },
        },
    )
    .await;
    let wrote = loop {
        let message = equipment_reply(&mut a).await;
        if matches!(
            message,
            ServerMessage::Object { .. } | ServerMessage::Error { .. }
        ) {
            break message;
        }
    };
    assert!(
        matches!(&wrote, ServerMessage::Object { reply } if reply.message.contains("Письмо")),
        "{wrote:?}"
    );
    let letter: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM documents d JOIN items i ON i.id=d.item_id WHERE i.character_id=$1",
    )
    .bind(a_id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(letter, 1);
    send(
        &mut a,
        ClientMessage::Work {
            command: Command::DispatchDocument,
        },
    )
    .await;
    let sent = loop {
        let message = equipment_reply(&mut a).await;
        if matches!(
            message,
            ServerMessage::Object { .. } | ServerMessage::Error { .. }
        ) {
            break message;
        }
    };
    assert!(
        matches!(&sent, ServerMessage::Object { reply } if reply.message.contains("Птица")),
        "{sent:?}"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let flying: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM couriers c JOIN items i ON i.id=c.item_id WHERE i.kind=$1",
    )
    .bind(i16::from(society::DOCUMENT))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(flying, 1, "the letter is still on the way");
    tokio::time::sleep(Duration::from_secs(4)).await;
    let arrived: i64 = sqlx::query_scalar("SELECT count(*) FROM couriers")
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(arrived, 0);
    sqlx::query("DELETE FROM alliances WHERE leader_id=$1 OR leader_id=$2")
        .bind(a_id)
        .bind(b_id)
        .execute(&server.pool)
        .await
        .unwrap();
    for name in [&a_name, &b_name] {
        sqlx::query("DELETE FROM accounts WHERE login=$1")
            .bind(name)
            .execute(&server.pool)
            .await
            .unwrap();
    }
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

/// Two clients, one shared loop: a new human meets another, gathers, sells iron
/// from a stall, places a beam, is knocked down and finished, leaves a body,
/// comes back, and still owns the result after a process restart.
/// A character converted from the old race/class sheet is the migration test; after
/// that conversion the same enter path is this one.
#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn two_players_walk_the_shared_loop() {
    use mortown_protocol::work::{self, Command};
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        % 1_000_000_000;
    let maker_name = format!("loop{unique}a");
    let buyer_name = format!("loop{unique}b");
    register(&address, &maker_name).await;
    register(&address, &buyer_name).await;
    let mut maker = connect(&address).await;
    send(&mut maker, login(&maker_name, "test-password-06")).await;
    list(&mut maker).await;
    send(&mut maker, creation(&maker_name)).await;
    let maker_id = list(&mut maker).await[0].id;
    let mut buyer = connect(&address).await;
    send(&mut buyer, login(&buyer_name, "test-password-06")).await;
    list(&mut buyer).await;
    send(&mut buyer, creation(&buyer_name)).await;
    let buyer_id = list(&mut buyer).await[0].id;
    let raced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='characters' AND column_name IN ('race','class')",
    )
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(raced, 0);
    stock(
        &server.pool,
        maker_id,
        &[(work::WOOD, 8), (items::IRON, 3)],
        None,
    )
    .await;
    let entered = enter(&mut maker, maker_id).await;
    enter(&mut buyer, buyer_id).await;
    let terrain = server.simulation.lock().await.terrain();
    let mut at = [entered.position[0], entered.position[2]];
    'spot: for x in -40..40 {
        for z in -40..40 {
            let feet = [
                entered.position[0] + x as f32 * 2.,
                entered.position[2] + z as f32 * 2.,
            ];
            let ahead = [feet[0], feet[1] + 1.5];
            if terrain.walkable(feet[0], feet[1])
                && terrain.hall_at(feet[0], feet[1]).is_none()
                && terrain.walkable(ahead[0], ahead[1])
                && terrain.hall_at(ahead[0], ahead[1]).is_none()
            {
                at = feet;
                break 'spot;
            }
        }
    }
    {
        let mut sim = server.simulation.lock().await;
        sim.pin(maker_id, at);
        sim.pin(buyer_id, at);
    }
    let label_of = |actors: &[Actor], expect: &str| {
        actors
            .iter()
            .find(|actor| actor.id == maker_id)
            .is_some_and(|actor| actor.nickname == expect && actor.nickname != maker_name)
    };
    let mut deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let ServerMessage::Snapshot { actors, .. } = raw(&mut buyer).await
            && label_of(&actors, "Незнакомец")
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "buyer never saw a stranger"
        );
    }
    send(
        &mut maker,
        ClientMessage::Society {
            command: mortown_protocol::society::Command::Present {
                name: "Чужак".into(),
            },
        },
    )
    .await;
    deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let ServerMessage::Snapshot { actors, .. } = raw(&mut buyer).await
            && label_of(&actors, "Чужак")
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "claimed name did not arrive"
        );
    }
    let node: i64 = sqlx::query_scalar(
        "INSERT INTO resource_nodes(product,x,z,remaining) VALUES($1,$2,$3,1) RETURNING id",
    )
    .bind(i16::from(work::WOOD))
    .bind(at[0])
    .bind(at[1])
    .fetch_one(&server.pool)
    .await
    .unwrap();
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Gather { node },
        },
    )
    .await;
    let gathered = equipment_reply(&mut maker).await;
    assert!(
        matches!(&gathered, ServerMessage::Object { reply } if reply.message.contains("Добыто")),
        "{gathered:?}"
    );
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Design {
                material: items::IRON,
                geometry: 0,
                process: 0,
            },
        },
    )
    .await;
    let planned = equipment_reply(&mut maker).await;
    assert!(
        matches!(&planned, ServerMessage::Object { reply } if reply.message.contains("горн")),
        "{planned:?}"
    );
    let iron: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(quantity),0) FROM items WHERE character_id=$1 AND kind=$2",
    )
    .bind(maker_id)
    .bind(i16::from(items::IRON))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(iron, 3);
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::MakeStall,
        },
    )
    .await;
    let stall_made = equipment_reply(&mut maker).await;
    assert!(
        matches!(&stall_made, ServerMessage::Object { reply } if reply.message.contains("прилавок")),
        "{stall_made:?}"
    );
    let stall_item: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE character_id=$1 AND kind=$2")
            .bind(maker_id)
            .bind(i16::from(items::STALL))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    send(
        &mut maker,
        ClientMessage::Objects {
            command: mortown_protocol::objects::Command::Place { item: stall_item },
        },
    )
    .await;
    let placed = equipment_reply(&mut maker).await;
    assert!(
        matches!(&placed, ServerMessage::Object { reply } if reply.message.contains("поставлен")),
        "{placed:?}"
    );
    let stall: i64 =
        sqlx::query_scalar("SELECT id FROM world_objects WHERE placed_by=$1 AND kind=$2")
            .bind(maker_id)
            .bind(i16::from(items::STALL))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    let (goods, goods_qty): (i64, i32) = sqlx::query_as(
        "SELECT id, quantity FROM items WHERE character_id=$1 AND kind=$2 ORDER BY id LIMIT 1",
    )
    .bind(maker_id)
    .bind(i16::from(items::IRON))
    .fetch_one(&server.pool)
    .await
    .unwrap();
    send(
        &mut maker,
        ClientMessage::Objects {
            command: mortown_protocol::objects::Command::Put {
                container: stall,
                revision: 0,
                item: Some(mortown_protocol::transfer::ItemRequest {
                    item: goods,
                    quantity: goods_qty as u32,
                }),
                coins: 0,
            },
        },
    )
    .await;
    equipment_reply(&mut maker).await;
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::Price {
                container: stall,
                price: 10,
            },
        },
    )
    .await;
    let priced = equipment_reply(&mut maker).await;
    assert!(
        matches!(&priced, ServerMessage::Object { reply } if reply.message.contains("Цена")),
        "{priced:?}"
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut buyer,
        ClientMessage::Work {
            command: Command::Buy {
                container: stall,
                price: 10,
            },
        },
    )
    .await;
    let bought = equipment_reply(&mut buyer).await;
    assert!(
        matches!(&bought, ServerMessage::Object { reply } if reply.message.contains("Куплено")),
        "{bought:?}"
    );
    let owned: i64 =
        sqlx::query_scalar("SELECT count(*) FROM items WHERE id=$1 AND character_id=$2")
            .bind(goods)
            .bind(buyer_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(owned, 1);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::MakeTimber,
        },
    )
    .await;
    let beam = equipment_reply(&mut maker).await;
    assert!(
        matches!(&beam, ServerMessage::Object { reply } if reply.message.contains("брус")),
        "{beam:?}"
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut maker,
        ClientMessage::Work {
            command: Command::PlaceTimber,
        },
    )
    .await;
    let built = equipment_reply(&mut maker).await;
    assert!(
        matches!(&built, ServerMessage::Object { reply } if reply.message.contains("поставлен")),
        "{built:?}"
    );
    let block: i64 = sqlx::query_scalar("SELECT id FROM blocks WHERE placed_by=$1")
        .bind(maker_id)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    {
        let mut sim = server.simulation.lock().await;
        let mut actor = sim.actor(maker_id).unwrap();
        let tick = sim.tick();
        sim.combat.player_admin(&mut actor, true, tick);
        assert!(actor.downed);
        assert!(!actor.pending_death);
        sim.replace_actor(actor);
    }
    send(&mut maker, ClientMessage::Respawn).await;
    let early = equipment_reply(&mut maker).await;
    assert!(
        matches!(&early, ServerMessage::Error { message, fatal: false } if message.contains("рано")),
        "{early:?}"
    );
    {
        let mut sim = server.simulation.lock().await;
        let mut actor = sim.actor(maker_id).unwrap();
        let tick = sim.tick();
        sim.combat.player_admin(&mut actor, true, tick);
        assert!(actor.pending_death);
        sim.replace_actor(actor);
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let lives: f64 =
        sqlx::query_scalar("SELECT (stats->>'lives')::float8 FROM characters WHERE id=$1")
            .bind(maker_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(lives, 9.);
    send(&mut maker, ClientMessage::LeaveWorld).await;
    list(&mut maker).await;
    {
        let sim = server.simulation.lock().await;
        assert!(sim.actor(maker_id).unwrap().offline);
    }
    deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let ServerMessage::Snapshot { actors, .. } = raw(&mut buyer).await
            && actors.iter().any(|actor| actor.id == maker_id)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "offline body disappeared"
        );
    }
    send(
        &mut maker,
        ClientMessage::SelectCharacter {
            character_id: maker_id,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut maker).await,
        ServerMessage::Welcome { .. }
    ));
    {
        let sim = server.simulation.lock().await;
        assert!(!sim.actor(maker_id).unwrap().offline);
    }
    send(&mut maker, ClientMessage::Logout).await;
    saved(&mut maker).await;
    send(&mut buyer, ClientMessage::Logout).await;
    saved(&mut buyer).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, _address, stop, task) = launch(&database).await;
    let still_owned: i64 =
        sqlx::query_scalar("SELECT count(*) FROM items WHERE id=$1 AND character_id=$2")
            .bind(goods)
            .bind(buyer_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(still_owned, 1);
    let beam_left: i64 = sqlx::query_scalar("SELECT count(*) FROM blocks WHERE id=$1")
        .bind(block)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(beam_left, 1);
    let lives_again: f64 =
        sqlx::query_scalar("SELECT (stats->>'lives')::float8 FROM characters WHERE id=$1")
            .bind(maker_id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(lives_again, 9.);
    sqlx::query("DELETE FROM resource_nodes WHERE id=$1")
        .bind(node)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM items WHERE container_id=$1")
        .bind(stall)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM world_objects WHERE id=$1")
        .bind(stall)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM blocks WHERE id=$1")
        .bind(block)
        .execute(&server.pool)
        .await
        .unwrap();
    for name in [&maker_name, &buyer_name] {
        sqlx::query("DELETE FROM accounts WHERE login=$1")
            .bind(name)
            .execute(&server.pool)
            .await
            .unwrap();
    }
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn foundry_saves_a_drawing_and_keeps_a_misrun() {
    use mortown_protocol::casting::{self, Command, DefectKind, Phase};
    use mortown_world::stations::{Furnishing, STATIONS};
    let database = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL");
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "cast{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 1_000_000_000
    );
    register(&address, &name).await;
    let mut socket = connect(&address).await;
    send(&mut socket, login(&name, "test-password-06")).await;
    assert!(list(&mut socket).await.is_empty());
    send(&mut socket, creation(&name)).await;
    let id = list(&mut socket).await[0].id;
    let xp_before: f64 =
        sqlx::query_scalar("SELECT (stats->>'current_exp')::float8 FROM characters WHERE id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    stock(&server.pool, id, &[(items::BRONZE, 36)], None).await;
    let actor = enter(&mut socket, id).await;
    let terrain = server.simulation.lock().await.terrain();
    let station = STATIONS
        .iter()
        .copied()
        .find(|s| s.furnishing == Furnishing::Smithy)
        .unwrap();
    let surface = station.work_surface(&terrain);
    let target = [surface[0], surface[2]];
    let mut stand = target;
    'search: for radius in [0., 0.4, 0.8, 1.2, 1.8, 2.5, 3.5] {
        for step in 0..12 {
            let angle = step as f32 * std::f32::consts::PI / 6.;
            let point = [
                target[0] + radius * angle.cos(),
                target[1] + radius * angle.sin(),
            ];
            if mortown_world::distance(point, target) <= mortown_protocol::objects::REACH
                && terrain.walkable(point[0], point[1])
            {
                stand = point;
                break 'search;
            }
        }
    }
    assert!(
        terrain.walkable(stand[0], stand[1]),
        "no walkable point at the smithy"
    );
    drop(terrain);
    server.simulation.lock().await.pin(actor.id, stand);
    let (sword, thickness) = casting::example_sword();
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: 0,
                points: sword.clone(),
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    let view = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("save: {other:?}"),
    };
    assert!(view.pourable, "{}", view.message);
    assert_eq!(view.phase, Phase::Draft);
    assert_eq!(view.charge_mg, 0);
    let bronze_left = async |pool: &sqlx::PgPool| {
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(sum(quantity),0)::bigint FROM items WHERE character_id=$1 AND kind=$2",
        )
        .bind(id)
        .bind(i16::from(items::BRONZE))
        .fetch_one(pool)
        .await
        .unwrap()
    };
    assert_eq!(
        bronze_left(&server.pool).await,
        36,
        "saving a drawing does not spend bronze"
    );
    let branch = vec![
        [0, 0],
        [80, 0],
        [80, 15],
        [20, 15],
        [20, 45],
        [80, 45],
        [80, 60],
        [0, 60],
    ];
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: view.revision,
                points: branch,
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    let branched = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("branched save: {other:?}"),
    };
    assert!(!branched.pourable, "{}", branched.message);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: branched.revision,
                units: 18,
            },
        },
    )
    .await;
    assert!(matches!(
        casting_reply(&mut socket).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    assert_eq!(bronze_left(&server.pool).await, 36);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: branched.revision,
                points: sword.clone(),
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    let drawn = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("redraw: {other:?}"),
    };
    assert!(drawn.pourable);
    let bronze_ready = bronze_left(&server.pool).await;
    {
        let mut actor = server.simulation.lock().await.actor(id).unwrap();
        actor.bound = true;
        server.simulation.lock().await.replace_actor(actor);
    }
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: drawn.revision,
                units: 18,
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("не можете"), "{message}");
        }
        other => panic!("bound charge: {other:?}"),
    }
    {
        let mut actor = server.simulation.lock().await.actor(id).unwrap();
        actor.bound = false;
        server.simulation.lock().await.replace_actor(actor);
    }
    server.simulation.lock().await.pin(id, [10_000., 10_000.]);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: drawn.revision,
                units: 18,
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("горну"), "{message}");
        }
        other => panic!("far charge: {other:?}"),
    }
    server.simulation.lock().await.pin(id, stand);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: drawn.revision.wrapping_add(3),
                units: 18,
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("изменился"), "{message}");
        }
        other => panic!("stale charge: {other:?}"),
    }
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: drawn.revision,
                points: vec![[0, 0], [200, 40], [0, 40], [200, 0]],
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("пересекает"), "{message}");
        }
        other => panic!("crossed save: {other:?}"),
    }
    assert_eq!(bronze_left(&server.pool).await, bronze_ready);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: drawn.revision,
                units: 18,
            },
        },
    )
    .await;
    let charged = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("charge: {other:?}"),
    };
    assert_eq!(charged.phase, Phase::Charged);
    assert_eq!(charged.charge_mg, 1_800_000);
    assert_eq!(bronze_left(&server.pool).await, 18);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: charged.revision,
                target_c: 1300,
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("не держит"), "{message}");
        }
        other => panic!("overheat: {other:?}"),
    }
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: charged.revision,
                target_c: 800,
            },
        },
    )
    .await;
    let warm = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("warm: {other:?}"),
    };
    assert_eq!(warm.temperature_c, Some(800));
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: warm.revision,
                gate_mm2: 25,
                ideal_feed: true,
                seconds: 0,
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("не жидкий"), "{message}");
        }
        other => panic!("cold pour: {other:?}"),
    }
    let still_warm: (String, i64) =
        sqlx::query_as("SELECT phase, charge_mg FROM craft_sessions WHERE character_id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(still_warm, ("hot".into(), 1_800_000));
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: warm.revision,
                target_c: 1040,
            },
        },
    )
    .await;
    let hot = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("heat: {other:?}"),
    };
    assert_eq!(hot.phase, Phase::Hot);
    assert_eq!(hot.temperature_c, Some(1040));
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: hot.revision,
                gate_mm2: 0,
                ideal_feed: true,
                seconds: 0,
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("Площадь"), "{message}");
        }
        other => panic!("zero gate: {other:?}"),
    }
    let still_hot: String =
        sqlx::query_scalar("SELECT phase FROM craft_sessions WHERE character_id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(still_hot, "hot");
    send(&mut socket, ClientMessage::LeaveWorld).await;
    assert_eq!(list(&mut socket).await[0].id, id);
    enter(&mut socket, id).await;
    server.simulation.lock().await.pin(id, stand);
    assert_eq!(
        bronze_left(&server.pool).await,
        18,
        "reconnect does not refund the crucible"
    );
    let kept_heat: (String, i64) =
        sqlx::query_as("SELECT phase, charge_mg FROM craft_sessions WHERE character_id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(kept_heat, ("hot".into(), 1_800_000));
    let second_session = sqlx::query(
        "INSERT INTO craft_sessions(character_id, revision, phase) VALUES($1, 0, 'draft')",
    )
    .bind(id)
    .execute(&server.pool)
    .await;
    assert!(second_session.is_err(), "one foundry session per character");
    sqlx::query(
        "UPDATE craft_sessions SET phase='pouring', gate_mm2=25, ideal_feed=true, pour_seconds=0 \
         WHERE character_id=$1",
    )
    .bind(id)
    .execute(&server.pool)
    .await
    .unwrap();
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: hot.revision,
                gate_mm2: 2,
                ideal_feed: false,
                seconds: 2,
            },
        },
    )
    .await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: hot.revision,
                gate_mm2: 2,
                ideal_feed: false,
                seconds: 2,
            },
        },
    )
    .await;
    let wide = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("wide pour: {other:?}"),
    };
    assert_eq!(wide.phase, Phase::Poured, "{}", wide.message);
    assert!(
        wide.message.contains("заполнена"),
        "a reserved wide gate fills even if the retry asks for a short narrow pour: {}",
        wide.message
    );
    let wide_center = wide
        .center_tenth_mm
        .expect("the full blank has a centre of mass");
    let wide_inertia = wide.inertia_mg_m2.expect("the full blank has inertia");
    assert!(wide_center > 0, "{wide_center}");
    assert!(wide_inertia > 0);
    let field: Vec<i16> = wide
        .spans
        .iter()
        .filter(|span| span.fill_milli > 0)
        .map(|span| span.temperature_c)
        .collect();
    assert!(
        field.iter().all(|temperature| *temperature > 900),
        "filled cells keep a temperature: {field:?}"
    );
    let coldest = field.iter().copied().min();
    assert!(coldest.unwrap() < 1040, "{field:?}");
    assert_eq!(wide.temperature_c, coldest);
    let reserved: (i32, i32) =
        sqlx::query_as("SELECT gate_mm2, pour_seconds FROM craft_sessions WHERE character_id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(reserved, (25, 0));
    let first = wide.result_item.expect("a blank");
    let mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!(mass > 0);
    let bodies: i64 = sqlx::query_scalar("SELECT count(*) FROM item_bodies WHERE item_id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(bodies, 1);
    let again = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("repeat pour: {other:?}"),
    };
    assert_eq!(again.result_item, Some(first));
    assert!(
        again.message.contains("уже записана"),
        "the queued pour is the same blank: {}",
        again.message
    );
    let still: i64 =
        sqlx::query_scalar("SELECT count(*) FROM items WHERE character_id=$1 AND kind=$2")
            .bind(id)
            .bind(i16::from(items::CASTING))
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(still, 1, "a repeated pour does not mint a second blank");
    send(
        &mut socket,
        ClientMessage::SetEquipment {
            item: items::CASTING,
            equipped: true,
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("нельзя надеть"), "{message}");
        }
        other => panic!("equip a casting: {other:?}"),
    }
    let worn: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM items WHERE id=$1 AND character_id=$2 AND place='bag'",
    )
    .bind(first)
    .bind(id)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(worn, 1);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let body_before: String =
        sqlx::query_scalar("SELECT geometry::text FROM item_bodies WHERE item_id=$1")
            .bind(first)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    let dropped = act(
        &mut socket,
        mortown_protocol::objects::Command::Drop {
            item: first,
            quantity: 1,
        },
    )
    .await;
    assert!(dropped.update.is_some(), "{}", dropped.message);
    let on_ground: (String, Option<i64>) =
        sqlx::query_as("SELECT place, character_id FROM items WHERE id=$1")
            .bind(first)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(on_ground, ("ground".into(), None));
    let body_ground: String =
        sqlx::query_scalar("SELECT geometry::text FROM item_bodies WHERE item_id=$1")
            .bind(first)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(body_ground, body_before);
    let lifted = act(
        &mut socket,
        mortown_protocol::objects::Command::PickUp { item: first },
    )
    .await;
    assert!(lifted.update.is_some(), "{}", lifted.message);
    let back: (String, Option<i64>, i64) =
        sqlx::query_as("SELECT place, character_id, mass_mg FROM items WHERE id=$1")
            .bind(first)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(back, ("bag".into(), Some(id), mass));
    let body_back: String =
        sqlx::query_scalar("SELECT geometry::text FROM item_bodies WHERE item_id=$1")
            .bind(first)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(body_back, body_before);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Machine {
                item: first,
                edge_tenth_mm: 1,
                side: casting::Side::Both,
                from_mm: None,
                to_mm: None,
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("остудите"), "{message}");
        }
        other => panic!("hot bevel: {other:?}"),
    }
    let hot_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(hot_mass, mass);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Cool { item: first },
        },
    )
    .await;
    let cooled = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("cool: {other:?}"),
    };
    assert!(cooled.message.contains("остыла"), "{}", cooled.message);
    assert_eq!(cooled.temperature_c, Some(300));
    assert!(
        cooled
            .spans
            .iter()
            .filter(|span| span.fill_milli > 0)
            .all(|span| span.temperature_c == 300)
    );
    let cooled_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(cooled_mass, mass);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Cool { item: first },
        },
    )
    .await;
    let cooled_again = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("second cool: {other:?}"),
    };
    assert!(
        cooled_again.message.contains("уже холодная"),
        "{}",
        cooled_again.message
    );
    let cooled_twice: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(cooled_twice, mass);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Machine {
                item: first,
                edge_tenth_mm: 1,
                side: casting::Side::Both,
                from_mm: None,
                to_mm: None,
            },
        },
    )
    .await;
    let beveled = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("bevel: {other:?}"),
    };
    assert!(beveled.message.contains("Снят скос"), "{}", beveled.message);
    assert_eq!(beveled.center_tenth_mm, Some(wide_center));
    assert!(beveled.inertia_mg_m2.unwrap_or(0) < wide_inertia);
    let beveled_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!(beveled_mass < mass);
    let solver_version: i32 =
        sqlx::query_scalar("SELECT solver_version FROM item_bodies WHERE item_id=$1")
            .bind(first)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(solver_version, 1);
    let other_name = format!("{name}b");
    register(&address, &other_name).await;
    let mut other = connect(&address).await;
    send(&mut other, login(&other_name, "test-password-06")).await;
    list(&mut other).await;
    send(&mut other, creation(&other_name)).await;
    let other_id = list(&mut other).await[0].id;
    enter(&mut other, other_id).await;
    server.simulation.lock().await.pin(other_id, stand);
    send(
        &mut other,
        ClientMessage::Cast {
            command: Command::Cool { item: first },
        },
    )
    .await;
    match casting_reply(&mut other).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("не у вас"), "{message}");
        }
        other_reply => panic!("foreign cool: {other_reply:?}"),
    }
    send(
        &mut other,
        ClientMessage::Cast {
            command: Command::Machine {
                item: first,
                edge_tenth_mm: 1,
                side: casting::Side::Both,
                from_mm: None,
                to_mm: None,
            },
        },
    )
    .await;
    match casting_reply(&mut other).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("не у вас"), "{message}");
        }
        other_reply => panic!("foreign bevel: {other_reply:?}"),
    }
    let untouched: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(untouched, beveled_mass);
    let rest_after_wide = bronze_left(&server.pool).await;
    assert!(
        rest_after_wide > 18,
        "unused ingots come back, left {rest_after_wide}"
    );
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Machine {
                item: first,
                edge_tenth_mm: 1,
                side: casting::Side::Both,
                from_mm: None,
                to_mm: None,
            },
        },
    )
    .await;
    let beveled_again = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("second bevel: {other:?}"),
    };
    assert!(
        beveled_again.message.contains("уже снят"),
        "{}",
        beveled_again.message
    );
    let still_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(still_mass, beveled_mass);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: beveled_again.revision,
                points: sword.clone(),
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    let next = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("second drawing: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: next.revision,
                units: 18,
            },
        },
    )
    .await;
    let charged = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("second charge: {other:?}"),
    };
    assert_eq!(bronze_left(&server.pool).await, rest_after_wide - 18);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: charged.revision,
                target_c: 1040,
            },
        },
    )
    .await;
    let hot = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("second heat: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: hot.revision,
                gate_mm2: 2,
                ideal_feed: true,
                seconds: 0,
            },
        },
    )
    .await;
    let narrow = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("narrow pour: {other:?}"),
    };
    assert!(
        narrow.defects.iter().any(|d| d.kind == DefectKind::Misrun),
        "narrow gate should leave an empty span: {}",
        narrow.message
    );
    assert!(
        narrow
            .defects
            .iter()
            .any(|d| d.kind == DefectKind::SolidifiedFeed),
        "the plug is a separate mark from the empty span: {}",
        narrow.message
    );
    assert!(narrow.spans.iter().any(|span| span.fill_milli == 0));
    assert!(narrow.center_tenth_mm.unwrap_or(wide_center) < wide_center);
    assert!(narrow.spans.iter().any(|span| span.fill_milli > 0));
    let metal: f32 = casting::blank_boxes(&narrow.spans)
        .iter()
        .map(|blank| blank.size_m[0])
        .sum();
    let mold: f32 = narrow
        .spans
        .iter()
        .map(|span| (span.x1_mm - span.x0_mm) as f32)
        .sum::<f32>()
        / 1000.;
    assert!(
        metal > 0. && metal < mold,
        "the visible blank is only the filled length"
    );
    let second = narrow.result_item.expect("short blank");
    assert_ne!(first, second);
    let narrow_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(second)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!(narrow_mass < mass);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Cool { item: second },
        },
    )
    .await;
    let short_cool = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("short cool: {other:?}"),
    };
    assert_eq!(short_cool.temperature_c, Some(300));
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Machine {
                item: second,
                edge_tenth_mm: 1,
                side: casting::Side::Both,
                from_mm: None,
                to_mm: None,
            },
        },
    )
    .await;
    let short_cut = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("short bevel: {other:?}"),
    };
    assert!(short_cut.spans.iter().any(|span| span.fill_milli == 0));
    assert!(
        short_cut
            .spans
            .iter()
            .any(|span| span.edges_tenth_mm == [1, 1])
    );
    assert!(
        short_cut
            .spans
            .iter()
            .filter(|span| span.fill_milli == 0)
            .all(|span| span.edges_tenth_mm == [0, 0])
    );
    let short_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(second)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!(short_mass < narrow_mass);
    assert!(bronze_left(&server.pool).await > rest_after_wide - 18);
    let blanks: Vec<(i64, i32)> = sqlx::query_as(
        "SELECT id, quantity FROM items WHERE character_id=$1 AND kind=$2 ORDER BY id",
    )
    .bind(id)
    .bind(i16::from(items::CASTING))
    .fetch_all(&server.pool)
    .await
    .unwrap();
    assert_eq!(blanks.len(), 2);
    assert!(blanks.iter().all(|(_, quantity)| *quantity == 1));
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: short_cut.revision,
                points: sword.clone(),
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    let early = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("early drawing: {other:?}"),
    };
    let left = bronze_left(&server.pool).await;
    assert!(left >= 6, "need metal for a timed pour, left {left}");
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: early.revision,
                units: u16::try_from(left.min(18)).unwrap(),
            },
        },
    )
    .await;
    let early = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("early charge: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: early.revision,
                target_c: 1040,
            },
        },
    )
    .await;
    let early = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("early heat: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: early.revision,
                gate_mm2: 25,
                ideal_feed: true,
                seconds: 2,
            },
        },
    )
    .await;
    let early = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("early pour: {other:?}"),
    };
    assert_eq!(early.phase, Phase::Poured);
    assert!(
        early.defects.iter().any(|d| d.kind == DefectKind::Halted),
        "{:?}",
        early.defects
    );
    assert!(
        early.message.contains("Подача закрыта"),
        "{}",
        early.message
    );
    assert!(early.spans.iter().any(|span| span.fill_milli == 0));
    assert!(early.spans.iter().any(|span| span.fill_milli > 0));
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let buyer_name = format!("b{name}");
    register(&address, &buyer_name).await;
    let mut buyer = connect(&address).await;
    send(&mut buyer, login(&buyer_name, "test-password-06")).await;
    list(&mut buyer).await;
    send(&mut buyer, creation(&buyer_name)).await;
    let buyer_id = list(&mut buyer).await[0].id;
    enter(&mut buyer, buyer_id).await;
    {
        let mut sim = server.simulation.lock().await;
        let terrain = sim.terrain();
        let mut spot = stand;
        'peer: for radius in [0.6, 1.2, 2.0, 3.0] {
            for step in 0..12 {
                let angle = step as f32 * std::f32::consts::PI / 6.;
                let point = [
                    stand[0] + radius * angle.cos(),
                    stand[1] + radius * angle.sin(),
                ];
                if terrain.walkable(point[0], point[1])
                    && mortown_world::distance(stand, point) <= 6.
                    && terrain.segment_clear(stand, point)
                {
                    spot = point;
                    break 'peer;
                }
            }
        }
        assert!(
            terrain.segment_clear(stand, spot) && mortown_world::distance(stand, spot) > 0.,
            "no place to hand over the blank"
        );
        sim.pin(buyer_id, spot);
    }
    let (maker_sheet, buyer_sheet) = trade(
        &mut socket,
        &mut buyer,
        buyer_id,
        Some(mortown_protocol::transfer::ItemRequest {
            item: first,
            quantity: 1,
        }),
        None,
        [0, 0],
    )
    .await;
    assert!(maker_sheet.items.iter().all(|stack| stack.id != first));
    assert!(buyer_sheet.items.iter().any(|stack| {
        stack.id == first && stack.mass_mg == Some(u64::try_from(beveled_mass).unwrap())
    }));
    let handed: (i64, i64) = sqlx::query_as("SELECT character_id, mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(handed, (buyer_id, beveled_mass));
    let edge: String =
        sqlx::query_scalar("SELECT geometry::text FROM item_bodies WHERE item_id=$1")
            .bind(first)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert!(edge.contains("\"edges_tenth_mm\": [1, 1]"), "{edge}");
    let bodies: i64 = sqlx::query_scalar("SELECT count(*) FROM item_bodies WHERE item_id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(bodies, 1, "the exchange keeps the same blank");
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: early.revision,
                points: sword.clone(),
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    let short_charge = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("short-charge drawing: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: short_charge.revision,
                units: 1,
            },
        },
    )
    .await;
    let short_charge = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("short charge: {other:?}"),
    };
    assert_eq!(short_charge.charge_mg, 100_000);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: short_charge.revision,
                target_c: 1040,
            },
        },
    )
    .await;
    let short_charge = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("short-charge heat: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: short_charge.revision,
                gate_mm2: 25,
                ideal_feed: true,
                seconds: 0,
            },
        },
    )
    .await;
    let short_charge = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("short-charge pour: {other:?}"),
    };
    assert!(
        short_charge
            .defects
            .iter()
            .any(|defect| defect.kind == DefectKind::ShortCharge),
        "{:?}",
        short_charge.defects
    );
    assert!(
        short_charge.message.contains("кончилась"),
        "{}",
        short_charge.message
    );
    assert!(short_charge.spans.iter().any(|span| span.fill_milli == 0));
    assert!(short_charge.spans.iter().any(|span| span.fill_milli > 0));
    let starved = short_charge
        .result_item
        .expect("a short charge still keeps its metal");
    let owner: i64 = sqlx::query_scalar("SELECT character_id FROM items WHERE id=$1")
        .bind(starved)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(owner, id);
    let starved_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(starved)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Cool { item: starved },
        },
    )
    .await;
    let starved_cool = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("cool the short charge: {other:?}"),
    };
    assert!(
        starved_cool
            .spans
            .iter()
            .filter(|span| span.fill_milli > 0)
            .all(|span| span.temperature_c == 300)
    );
    assert!(
        starved_cool
            .spans
            .iter()
            .filter(|span| span.fill_milli == 0)
            .all(|span| span.temperature_c == 0)
    );
    let starved_after: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(starved)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(starved_after, starved_mass);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let tang_zone = (-120, -60);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Machine {
                item: starved,
                edge_tenth_mm: 10,
                side: casting::Side::Left,
                from_mm: Some(tang_zone.0),
                to_mm: Some(tang_zone.1),
            },
        },
    )
    .await;
    let one_side = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("one-sided pass: {other:?}"),
    };
    assert!(
        one_side.message.contains("с левой стороны"),
        "{}",
        one_side.message
    );
    let inside = |span: &casting::Span| {
        let mid = (span.x0_mm + span.x1_mm) as f64 / 2.;
        span.fill_milli > 0 && mid >= f64::from(tang_zone.0) && mid <= f64::from(tang_zone.1)
    };
    assert!(one_side.spans.iter().any(inside));
    assert!(
        one_side
            .spans
            .iter()
            .filter(|span| inside(span))
            .all(|span| span.edges_tenth_mm == [10, 0]),
        "only the left side of the zone is ground"
    );
    assert!(
        one_side
            .spans
            .iter()
            .filter(|span| !inside(span))
            .all(|span| span.edges_tenth_mm == [0, 0]),
        "cells outside the zone keep their section"
    );
    let one_side_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(starved)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!(one_side_mass < starved_mass);
    assert_eq!(
        one_side.passes,
        [casting::Pass {
            edge_tenth_mm: 10,
            side: casting::Side::Left,
            from_mm: Some(tang_zone.0),
            to_mm: Some(tang_zone.1),
        }],
        "the blank records the pass that removed its metal"
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Machine {
                item: starved,
                edge_tenth_mm: 10,
                side: casting::Side::Right,
                from_mm: Some(300),
                to_mm: Some(600),
            },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("нет металла"), "{message}");
        }
        other => panic!("grinding the empty misrun: {other:?}"),
    }
    let misrun_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(starved)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(misrun_mass, one_side_mass);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: one_side.revision,
                points: sword.clone(),
                thickness_mm: thickness.clone(),
                passes: one_side.passes.clone(),
            },
        },
    )
    .await;
    let reserved_draw = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("reserved drawing: {other:?}"),
    };
    assert_eq!(reserved_draw.blueprint_passes, one_side.passes);
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: reserved_draw.revision,
                units: 1,
            },
        },
    )
    .await;
    let reserved_draw = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("reserved charge: {other:?}"),
    };
    assert_eq!(reserved_draw.charge_mg, 100_000);
    sqlx::query("DELETE FROM items WHERE character_id=$1 AND kind=$2 AND place='bag'")
        .bind(id)
        .bind(i16::from(items::BRONZE))
        .execute(&server.pool)
        .await
        .unwrap();
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: reserved_draw.revision,
                target_c: 1040,
            },
        },
    )
    .await;
    let reserved_draw = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("reserved heat: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: reserved_draw.revision,
                gate_mm2: 25,
                ideal_feed: true,
                seconds: 0,
            },
        },
    )
    .await;
    let reserved_pour = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("pour after the bag bronze was sold: {other:?}"),
    };
    assert_eq!(
        reserved_pour.phase,
        Phase::Poured,
        "{}",
        reserved_pour.message
    );
    let repeat = reserved_pour.result_item.expect("the second blank");
    assert_eq!(bronze_left(&server.pool).await, 0);
    let linked: Option<i64> =
        sqlx::query_scalar("SELECT blueprint_id FROM item_bodies WHERE item_id=$1")
            .bind(repeat)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert!(linked.is_some(), "the blank knows its blueprint");
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Finish { item: repeat },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("остудите"), "{message}");
        }
        other => panic!("finishing a hot blank: {other:?}"),
    }
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Cool { item: repeat },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Cast { .. } => {}
        other => panic!("cool the repeat: {other:?}"),
    }
    let repeat_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(repeat)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Finish { item: repeat },
        },
    )
    .await;
    let finished = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("finish by the blueprint: {other:?}"),
    };
    assert!(
        finished.message.contains("по чертежу"),
        "{}",
        finished.message
    );
    assert_eq!(finished.passes, one_side.passes);
    assert!(
        finished
            .spans
            .iter()
            .any(|span| span.edges_tenth_mm == [10, 0])
    );
    assert!(
        finished
            .spans
            .iter()
            .filter(|span| span.fill_milli == 0)
            .all(|span| span.edges_tenth_mm == [0, 0])
    );
    let finished_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(repeat)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert!(finished_mass < repeat_mass);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Finish { item: repeat },
        },
    )
    .await;
    let again = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("second finish: {other:?}"),
    };
    assert!(again.message.contains("уже выполнена"), "{}", again.message);
    let again_mass: i64 = sqlx::query_scalar("SELECT mass_mg FROM items WHERE id=$1")
        .bind(repeat)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(
        again_mass, finished_mass,
        "a repeated finish removes nothing"
    );
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Finish { item: starved },
        },
    )
    .await;
    match casting_reply(&mut socket).await {
        ServerMessage::Error { message, fatal } => {
            assert!(!fatal);
            assert!(message.contains("не записана"), "{message}");
        }
        other => panic!("finish without recorded passes: {other:?}"),
    }
    stock(&server.pool, id, &[(items::BRONZE, 18)], None).await;
    sqlx::query(
        "INSERT INTO items(kind, quantity, place, character_id, cell) VALUES ($1, 200, 'bag', $2, 200)",
    )
    .bind(i16::from(items::IRON))
    .bind(id)
    .execute(&server.pool)
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Save {
                revision: again.revision,
                points: sword.clone(),
                thickness_mm: thickness.clone(),
                passes: Vec::new(),
            },
        },
    )
    .await;
    let heavy = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("heavy drawing: {other:?}"),
    };
    let left = bronze_left(&server.pool).await;
    assert!(left >= 6, "need metal for the overweight pour, left {left}");
    let units = u16::try_from(left.min(18)).unwrap();
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Charge {
                revision: heavy.revision,
                units,
            },
        },
    )
    .await;
    let heavy = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("heavy charge: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Heat {
                revision: heavy.revision,
                target_c: 1040,
            },
        },
    )
    .await;
    let heavy = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("heavy heat: {other:?}"),
    };
    send(
        &mut socket,
        ClientMessage::Cast {
            command: Command::Pour {
                revision: heavy.revision,
                gate_mm2: 25,
                ideal_feed: true,
                seconds: 2,
            },
        },
    )
    .await;
    let heavy = match casting_reply(&mut socket).await {
        ServerMessage::Cast { view } => view,
        other => panic!("heavy pour: {other:?}"),
    };
    assert!(heavy.message.contains("у горна"), "{}", heavy.message);
    let heavy_blank = heavy.result_item.expect("the blank stays in the world");
    let landed: (Option<i64>, String) =
        sqlx::query_as("SELECT character_id, place FROM items WHERE id=$1")
            .bind(heavy_blank)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(landed.0, None);
    assert_eq!(landed.1, "ground");
    assert_eq!(
        bronze_left(&server.pool).await,
        left - i64::from(units),
        "returned ingots must not re-enter an overweight bag"
    );
    sqlx::query("UPDATE craft_sessions SET result_item=NULL WHERE result_item=$1")
        .bind(heavy_blank)
        .execute(&server.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM items WHERE place='ground' AND character_id IS NULL AND id>=$1")
        .bind(heavy_blank)
        .execute(&server.pool)
        .await
        .unwrap();
    let xp_after: f64 =
        sqlx::query_scalar("SELECT (stats->>'current_exp')::float8 FROM characters WHERE id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(xp_after, xp_before, "casting does not grant experience");
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, _address, stop, task) = launch(&database).await;
    let kept: (i64, i64) = sqlx::query_as("SELECT character_id, mass_mg FROM items WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(
        kept,
        (buyer_id, beveled_mass),
        "restart keeps the handed blank"
    );
    let short_owner: i64 = sqlx::query_scalar("SELECT character_id FROM items WHERE id=$1")
        .bind(second)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    assert_eq!(short_owner, id);
    let sided: (i64, serde_json::Value) = sqlx::query_as(
        "SELECT i.mass_mg, b.geometry FROM items i JOIN item_bodies b ON b.item_id=i.id WHERE i.id=$1",
    )
    .bind(starved)
    .fetch_one(&server.pool)
    .await
    .unwrap();
    assert_eq!(sided.0, one_side_mass, "restart keeps the ground mass");
    let sided: Vec<casting::Span> = serde_json::from_value(sided.1).unwrap();
    assert!(
        sided.iter().any(|span| span.edges_tenth_mm == [10, 0]),
        "restart keeps the one-sided edge"
    );
    let body_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM item_bodies WHERE item_id=$1 OR item_id=$2")
            .bind(first)
            .bind(second)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(body_rows, 2);
    sqlx::query("DELETE FROM accounts WHERE login=$1 OR login=$2")
        .bind(&name)
        .bind(&buyer_name)
        .execute(&server.pool)
        .await
        .unwrap();
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
}
