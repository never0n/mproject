//! Run against a development PostgreSQL: TEST_DATABASE_URL=... cargo test -p mortown-server --test network -- --ignored
use futures_util::{SinkExt, StreamExt};
use mortown_protocol::{Actor, ClientMessage, PROTOCOL_VERSION, ServerMessage, WORLD_VERSION};
use mortown_server::Server;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::{net::TcpStream, sync::watch};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[tokio::test]
#[ignore = "requires a running development PostgreSQL"]
async fn crafting_purchase_atomic_consumption_duplicates_and_restart() {
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
    // Only this disposable character receives materials; all crafting and purchases use WebSocket.
    sqlx::query("UPDATE characters SET loot='{\"9\":12,\"7\":8}'::jsonb, stats=stats || '{\"level\":3}'::jsonb WHERE id=$1")
        .bind(id).execute(&server.pool).await.unwrap();
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
            specialization_revision: 0,
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
        ClientMessage::Trade {
            sale: None,
            npc: mortown_world::npcs::PLAZA_TRADER,
            item: CUIRASS_RECIPE,
            buy: true,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated {
        character: purchased,
        ..
    } = equipment_reply(&mut s).await
    else {
        panic!("purchase")
    };
    assert_eq!(purchased.loot[&LEATHER], 12);
    assert_eq!(purchased.loot[&7], 8);
    assert_eq!(purchased.stats["coins"], initial.stats["coins"] - 25.);
    assert_eq!(crafting::count(&purchased), 0);
    let recipe_token = layout::token(CUIRASS_RECIPE, 0);
    let bag = *layout::bag(&purchased)
        .iter()
        .find(|(_, v)| **v == recipe_token)
        .unwrap()
        .0;
    send(
        &mut s,
        ClientMessage::Arrange {
            source: Place::Bag(bag),
            target: Place::Bar(11),
            expected: recipe_token,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::EconomyUpdated { .. }
    ));
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
            specialization_revision: 0,
            recipe: CUIRASS_RECIPE,
            expected: 0,
        },
    )
    .await;
    send(
        &mut s,
        ClientMessage::Craft {
            specialization_revision: 0,
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
    assert_eq!(crafted.loot, actor.loot);
    assert_eq!(crafted.loot[&LEATHER], 6);
    assert_eq!(crafted.loot[&7], 4);
    assert_eq!(crafted.loot[&6], 1);
    assert_eq!(crafted.loot[&CUIRASS_RECIPE], 1);
    assert_eq!(crafted.stats["f_defence"], defence);
    assert_eq!(crafted.stats["coins"], purchased.stats["coins"]);
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
    assert_eq!(restored.loot, crafted.loot);
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
            specialization_revision: 0,
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
            specialization_revision: 0,
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
    assert_eq!(finished.loot[&6], 2);
    assert_eq!(finished.loot[&LEATHER], 0);
    assert_eq!(finished.loot[&7], 0);
    assert_eq!(finished.loot[&CUIRASS_RECIPE], 1);
    assert!(
        !finished
            .inventory
            .iter()
            .any(|i| matches!(i.kind, 7 | LEATHER))
    );
    assert_eq!(
        layout::bag(&finished)
            .values()
            .filter(|t| layout::kind(**t) == Some(6))
            .count(),
        2
    );
    send(
        &mut s,
        ClientMessage::Craft {
            specialization_revision: 0,
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
    assert_eq!(restored[0].loot, finished.loot);
    assert_eq!(crafting::count(&restored[0]), 2);
    send(&mut s, creation(&format!("{name}b"))).await;
    let roster = list(&mut s).await;
    assert_eq!(crafting::count(&roster[1]), 0);
    assert!(roster[1].loot.is_empty());
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
async fn receive(s: &mut Socket) -> ServerMessage {
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
        race: 3,
        class: 2,
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
                character.stats,
                mortown_protocol::starting_stats(3, 2).unwrap()
            );
            actor
        }
        other => panic!("welcome expected: {other:?}"),
    }
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
    assert!(server.simulation.lock().await.actor(first).is_none());
    let stored_x: f32 = sqlx::query_scalar("SELECT x FROM characters WHERE id=$1")
        .bind(first)
        .fetch_one(&server.pool)
        .await
        .unwrap();
    send(&mut alice, creation(&format!("{a}2"))).await;
    let chars = list(&mut alice).await;
    let second = chars[1].id;
    assert_eq!(chars[1].slot, 2);
    let second_actor = enter(&mut alice, second).await;
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
        race: 1,
        class: 1,
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
        serde_json::to_value(mortown_protocol::items::starter(2)).unwrap()
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
    assert_eq!(chars[0].inventory, mortown_protocol::items::starter(2));
    enter(&mut s, id).await;
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 1,
            equipped: true,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
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
        assert_eq!(character.stats["m_attack"], chars[0].stats["m_attack"] + 6.);
        assert_eq!(character.inventory.iter().filter(|i| i.equipped).count(), 1);
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
    assert_eq!(
        character.stats["f_defence"],
        chars[0].stats["f_defence"] + 4.
    );
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
    assert!(chars[1].inventory.iter().all(|i| !i.equipped));
    assert_eq!(
        chars[1].stats,
        mortown_protocol::starting_stats(3, 2).unwrap()
    );
    send(&mut s, ClientMessage::Logout).await;
    drop(s);
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    assert!(chars[0].inventory[0].equipped);
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
        character.stats,
        mortown_protocol::starting_stats(3, 2).unwrap()
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
    assert_eq!(actor.respawn_seconds, 10);
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
        "UPDATE characters SET x=$1,z=$2,stats=jsonb_set(stats,'{current_exp}','75'),loot='{\"5\":2}' WHERE id=$3",
    )
    .bind(m.position[0])
    .bind(m.position[2])
    .bind(id)
    .execute(&server.pool)
    .await
    .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    server.simulation.lock().await.combat.mobs[0].view.hp = 1.;
    send(&mut s, ClientMessage::Attack { target: Some(m.id) }).await;
    let winner = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::Snapshot { actors, .. } = receive(&mut s).await {
                let a = actors.into_iter().find(|a| a.id == id).unwrap();
                if a.progress.kills == 1 {
                    break a;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(winner.loot[&7], 1);
    assert!(winner.loot[&5] >= 2);
    assert_eq!(winner.progress.level, 2);
    assert_eq!(winner.progress.coins, chars[0].stats["coins"] + 12.);
    // Reward is committed before it is sent to its owner.
    let saved_stats: serde_json::Value =
        sqlx::query_scalar("SELECT stats FROM characters WHERE id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(saved_stats["level"], 2.);
    assert_eq!(saved_stats["kills"], 1.);
    let saved_loot: serde_json::Value =
        sqlx::query_scalar("SELECT loot FROM characters WHERE id=$1")
            .bind(id)
            .fetch_one(&server.pool)
            .await
            .unwrap();
    assert_eq!(saved_loot, serde_json::to_value(&winner.loot).unwrap());
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
    assert!(attack > chars[0].stats["m_attack"] + 10.);
    send(&mut s, ClientMessage::Logout).await;
    saved(&mut s).await;
    stop.send_replace(true);
    task.await.unwrap();
    server.pool.close().await;
    let (server, address, stop, task) = launch(&database).await;
    let mut s = connect(&address).await;
    send(&mut s, login(&name, "test-password-06")).await;
    let chars = list(&mut s).await;
    assert_eq!(chars[0].loot, winner.loot);
    assert_eq!(chars[0].inventory.iter().filter(|i| i.kind == 5).count(), 1);
    assert!(chars[0].inventory.iter().any(|i| i.kind == 5 && i.equipped));
    assert_eq!(chars[0].stats["m_attack"], attack);
    assert_eq!(chars[0].stats["coins"], winner.progress.coins);
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
async fn trading_and_potions_persist_without_resurrecting_sold_items() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "shop{}",
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
    sqlx::query("UPDATE characters SET loot='{\"5\":1,\"7\":2}',stats=stats || '{\"coins\":20,\"current_hp\":10}'::jsonb WHERE id=$1").bind(id).execute(&server.pool).await.unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome {
        character: mut bag, ..
    } = equipment_reply(&mut s).await
    else {
        panic!("welcome")
    };
    let sale_for = |c: &mortown_protocol::Character, kind: u8| {
        mortown_protocol::layout::bag(c)
            .into_iter()
            .find_map(|(slot, token)| {
                (mortown_protocol::layout::kind(token) == Some(kind)).then_some(
                    mortown_protocol::trade::Sale {
                        slot,
                        token,
                        owned: mortown_protocol::layout::quantity(c, kind),
                    },
                )
            })
    };
    for (mut message, ok) in [
        (
            ClientMessage::Trade {
                sale: None,
                npc: mortown_world::npcs::PLAZA_TRADER,
                item: 7,
                buy: false,
            },
            true,
        ),
        (
            ClientMessage::Trade {
                sale: None,
                npc: mortown_world::npcs::PLAZA_TRADER,
                item: 8,
                buy: true,
            },
            true,
        ),
        (ClientMessage::UsePotion, true),
        (
            ClientMessage::Trade {
                sale: None,
                npc: mortown_world::npcs::PLAZA_TRADER,
                item: 8,
                buy: true,
            },
            false,
        ),
        (
            ClientMessage::Trade {
                sale: None,
                npc: mortown_world::npcs::PLAZA_TRADER,
                item: 7,
                buy: false,
            },
            true,
        ),
        (
            ClientMessage::Trade {
                sale: None,
                npc: mortown_world::npcs::PLAZA_TRADER,
                item: 7,
                buy: false,
            },
            false,
        ),
    ] {
        if let ClientMessage::Trade {
            item,
            buy: false,
            sale,
            ..
        } = &mut message
        {
            *sale = sale_for(&bag, *item);
        }
        send(&mut s, message).await;
        let result = equipment_reply(&mut s).await;
        if let ServerMessage::EconomyUpdated { character, .. } = &result {
            bag = character.clone();
        }
        assert_eq!(
            matches!(result, ServerMessage::EconomyUpdated { .. }),
            ok,
            "{result:?}"
        );
    }
    send(
        &mut s,
        ClientMessage::SetEquipment {
            item: 5,
            equipped: true,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
        panic!("equipment")
    };
    bag = character;
    send(
        &mut s,
        ClientMessage::Trade {
            sale: Some(mortown_protocol::trade::Sale {
                slot: 0,
                token: mortown_protocol::layout::equipped(&bag, true),
                owned: mortown_protocol::layout::quantity(&bag, 5),
            }),
            npc: mortown_world::npcs::PLAZA_TRADER,
            item: 5,
            buy: false,
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
            equipped: false,
        },
    )
    .await;
    let ServerMessage::CharacterUpdated { character } = equipment_reply(&mut s).await else {
        panic!("equipment")
    };
    bag = character;
    for mut message in [
        ClientMessage::Trade {
            sale: None,
            npc: mortown_world::npcs::PLAZA_TRADER,
            item: 5,
            buy: false,
        },
        ClientMessage::Trade {
            sale: None,
            npc: mortown_world::npcs::PLAZA_TRADER,
            item: 8,
            buy: true,
        },
        ClientMessage::UsePotion,
    ] {
        if let ClientMessage::Trade {
            item,
            buy: false,
            sale,
            ..
        } = &mut message
        {
            *sale = sale_for(&bag, *item);
        }
        send(&mut s, message).await;
        let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
            panic!("trade")
        };
        bag = character;
    }
    send(&mut s, ClientMessage::UsePotion).await;
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
    assert_eq!(chars[0].stats["coins"], 2.);
    assert_eq!(chars[0].stats["current_hp"], chars[0].stats["hp"]);
    assert_eq!(chars[0].loot.get(&5), Some(&0));
    assert_eq!(chars[0].loot.get(&7), Some(&0));
    assert_eq!(chars[0].loot.get(&8), Some(&0));
    assert!(chars[0].inventory.iter().all(|i| i.kind < 4));
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
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
async fn quest_accept_kill_claim_and_restart_isolated_per_character() {
    let database = std::env::var("TEST_DATABASE_URL").unwrap();
    let (server, address, stop, task) = launch(&database).await;
    let name = format!(
        "quest{}",
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
    sqlx::query("UPDATE characters SET stats=stats || '{\"kills\":9}'::jsonb WHERE id=$1")
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Welcome { .. }
    ));
    send(
        &mut s,
        ClientMessage::Quest {
            quest: 1,
            complete: false,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated { actor, .. } = equipment_reply(&mut s).await else {
        panic!("accept expected")
    };
    assert_eq!(actor.progress.quest_kills, 0);
    send(
        &mut s,
        ClientMessage::Quest {
            quest: 1,
            complete: true,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    let mob = server.simulation.lock().await.monsters()[0].clone();
    sqlx::query("UPDATE characters SET x=$1,z=$2 WHERE id=$3")
        .bind(mob.position[0])
        .bind(mob.position[2])
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome { actor, .. } = equipment_reply(&mut s).await else {
        panic!("welcome expected")
    };
    assert_eq!(actor.progress.quest_stage, 1);
    for count in 1..=3 {
        {
            let mut simulation = server.simulation.lock().await;
            simulation.combat.mobs[0].view.max_hp = 1.;
            simulation.combat.mobs[0].view.hp = 1.;
        }
        send(
            &mut s,
            ClientMessage::Attack {
                target: Some(mob.id),
            },
        )
        .await;
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if let ServerMessage::Snapshot { actors, .. } = receive(&mut s).await
                    && actors
                        .iter()
                        .any(|a| a.id == id && a.progress.quest_kills == count)
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }
    send(
        &mut s,
        ClientMessage::Quest {
            quest: 1,
            complete: true,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::LeaveWorld).await;
    let chars = list(&mut s).await;
    assert_eq!(chars[0].stats["quest_kills"], 3.);
    sqlx::query("UPDATE characters SET x=$1,z=$2 WHERE id=$3")
        .bind(mortown_world::SPAWN[0])
        .bind(mortown_world::SPAWN[1])
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    equipment_reply(&mut s).await;
    send(
        &mut s,
        ClientMessage::Quest {
            quest: 1,
            complete: true,
        },
    )
    .await;
    let ServerMessage::EconomyUpdated { actor, .. } = equipment_reply(&mut s).await else {
        panic!("reward expected")
    };
    assert_eq!(actor.progress.coins, 611.);
    assert_eq!(actor.loot[&8], 2);
    send(
        &mut s,
        ClientMessage::Quest {
            quest: 1,
            complete: true,
        },
    )
    .await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    send(&mut s, creation(&format!("{name}b"))).await;
    let chars = list(&mut s).await;
    assert!(!chars[1].stats.contains_key("quest_stage"));
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
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
    assert_eq!(chars[0].stats["quest_stage"], 2.);
    assert_eq!(chars[0].stats["coins"], 611.);
    assert_eq!(chars[0].loot[&8], 2);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    equipment_reply(&mut s).await;
    send(
        &mut s,
        ClientMessage::Quest {
            quest: 1,
            complete: true,
        },
    )
    .await;
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
async fn skill_ranks_sp_and_character_isolation_survive_restart() {
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
    sqlx::query(
        "UPDATE characters SET stats=stats || '{\"level\":3,\"sp\":19}'::jsonb WHERE id=$1",
    )
    .bind(id)
    .execute(&server.pool)
    .await
    .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    equipment_reply(&mut s).await;
    for (rank, success, sp) in [
        (3, false, 19.),
        (2, true, 14.),
        (2, false, 14.),
        (3, true, 7.),
        (4, false, 7.),
    ] {
        send(&mut s, ClientMessage::LearnSkill { rank }).await;
        let reply = equipment_reply(&mut s).await;
        if success {
            let ServerMessage::EconomyUpdated { actor, character } = reply else {
                panic!("upgrade expected")
            };
            assert_eq!(actor.progress.skill_rank, rank);
            assert_eq!(actor.progress.sp, sp);
            assert_eq!(character.stats["skill_rank"], rank as f64);
            assert_eq!(character.stats["sp"], sp);
        } else {
            assert!(matches!(reply, ServerMessage::Error { fatal: false, .. }));
        }
    }
    send(&mut s, ClientMessage::LearnSecondary).await;
    let ServerMessage::EconomyUpdated { actor, .. } = equipment_reply(&mut s).await else {
        panic!("learn secondary expected")
    };
    assert!(actor.progress.secondary_learned);
    assert_eq!(actor.progress.sp, 0.);
    send(&mut s, ClientMessage::LearnSecondary).await;
    assert!(matches!(
        equipment_reply(&mut s).await,
        ServerMessage::Error { fatal: false, .. }
    ));
    let target;
    {
        let mut simulation = server.simulation.lock().await;
        let mob = simulation.monsters()[0].clone();
        target = mob.id;
        let mut actor = simulation.actor(id).unwrap();
        actor.position = mob.position;
        simulation.replace_actor(actor);
    }
    send(&mut s, ClientMessage::UseSecondary { target }).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let ServerMessage::Snapshot { actors, .. } = receive(&mut s).await
                && let Some(cast) = actors
                    .iter()
                    .find(|a| a.id == id && a.secondary_cooldown_ms > 0)
            {
                assert_eq!(cast.mana, actor.mana - 8.);
                break;
            }
        }
    })
    .await
    .unwrap();
    send(&mut s, ClientMessage::LeaveWorld).await;
    list(&mut s).await;
    send(&mut s, creation(&format!("{name}b"))).await;
    let chars = list(&mut s).await;
    assert!(!chars[1].stats.contains_key("secondary_learned"));
    assert!(!chars[1].stats.contains_key("skill_rank"));
    assert_eq!(chars[1].stats["sp"], 0.);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
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
    assert_eq!(chars[0].stats["skill_rank"], 3.);
    assert_eq!(chars[0].stats["sp"], 0.);
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome { actor, .. } = equipment_reply(&mut s).await else {
        panic!("welcome expected")
    };
    assert!(actor.progress.secondary_learned);
    assert_eq!(actor.progress.skill_rank, 3);
    send(&mut s, ClientMessage::LearnSkill { rank: 3 }).await;
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
    assert_eq!(chars[1].stats["stat_point"], 0.);
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
    for (slot, binding) in [(0, 0), (9, 102), (11, 1)] {
        send(&mut s, ClientMessage::BindHotbar { slot, binding }).await;
        let ServerMessage::EconomyUpdated { character, .. } = equipment_reply(&mut s).await else {
            panic!("binding expected")
        };
        assert_eq!(
            character.stats[&format!("hotbar_{slot}")],
            f64::from(binding)
        );
    }
    for (slot, binding) in [(12, 1), (0, 99), (255, 255)] {
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
    assert_eq!(chars[0].stats["hotbar_9"], 102.);
    assert_eq!(chars[0].stats["hotbar_11"], 1.);
    assert!(chars[0].inventory.iter().any(|i| i.kind == 2 && i.equipped));
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
async fn inventory_layout_copies_and_swaps_survive_restart() {
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
    sqlx::query("UPDATE characters SET loot='{\"5\":3,\"6\":2,\"8\":5}'::jsonb WHERE id=$1")
        .bind(id)
        .execute(&server.pool)
        .await
        .unwrap();
    send(&mut s, ClientMessage::SelectCharacter { character_id: id }).await;
    let ServerMessage::Welcome {
        character: mut c, ..
    } = equipment_reply(&mut s).await
    else {
        panic!("welcome")
    };
    let staff = layout::token(5, 1);
    let pos = Place::Bag(
        *layout::bag(&c)
            .iter()
            .find(|(_, v)| **v == staff)
            .unwrap()
            .0,
    );
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
    assert_eq!(layout::bag(&c).len(), 7);
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
    assert_eq!(layout::bag(&character).len(), 8);
    assert_eq!(character.loot[&5], 3);
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
