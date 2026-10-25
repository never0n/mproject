//! Persistent stacks (`items` table) and the locked load/persist of a character's sheet around
//! a property move. Everything runs inside the caller's transaction; callers that change an
//! in-world actor hold the simulation lock until the commit.
//! Lock order: characters by ascending id, then world objects, then item rows.
use crate::store::ExchangeFailure;
use mortown_protocol::{
    Actor, Character,
    items::{Location, Slot, Stack},
};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};
use std::collections::{BTreeMap, HashMap};

pub(crate) fn database_error(error: sqlx::Error) -> String {
    eprintln!(
        "Inventory database operation failed ({:?}): {error}",
        error.as_database_error().and_then(|e| e.code())
    );
    "Инвентарь временно недоступен. Попробуйте снова".into()
}

const COLUMNS: &str = "id,kind,quantity,lock_code,place,cell,container_id,x,z,mass_mg";
fn stack(row: &PgRow) -> Result<Stack, String> {
    let cell = row
        .get::<Option<i32>, _>("cell")
        .and_then(|c| u16::try_from(c).ok());
    let location = match (row.get::<String, _>("place").as_str(), cell) {
        ("bag", Some(cell)) => Location::Bag { cell },
        ("worn", Some(0)) => Location::Worn { slot: Slot::Weapon },
        ("worn", Some(_)) => Location::Worn { slot: Slot::Armor },
        ("purse", _) => Location::Purse,
        ("container", Some(cell)) => Location::Container {
            container: row.get("container_id"),
            cell,
        },
        ("ground", _) => Location::Ground {
            x: row.get("x"),
            z: row.get("z"),
        },
        _ => return Err("Повреждённая запись предмета".into()),
    };
    Ok(Stack {
        id: row.get("id"),
        kind: u8::try_from(row.get::<i16, _>("kind")).map_err(|_| "Неизвестный предмет")?,
        quantity: u32::try_from(row.get::<i32, _>("quantity"))
            .map_err(|_| "Неверное количество")?,
        location,
        lock: row.get("lock_code"),
        mass_mg: row
            .get::<Option<i64>, _>("mass_mg")
            .and_then(|mg| u64::try_from(mg).ok()),
    })
}
async fn select(conn: &mut PgConnection, filter: &str, id: i64) -> Result<Vec<Stack>, String> {
    sqlx::query(&format!(
        "SELECT {COLUMNS} FROM items WHERE {filter}=$1 ORDER BY id FOR UPDATE"
    ))
    .bind(id)
    .fetch_all(conn)
    .await
    .map_err(database_error)?
    .iter()
    .map(stack)
    .collect()
}
/// Bag, body and purse of a character, row-locked.
pub(crate) async fn carried(conn: &mut PgConnection, character: i64) -> Result<Vec<Stack>, String> {
    select(conn, "character_id", character).await
}
/// Bag, body and purse without locks, for read-only sheets.
pub(crate) async fn listed(conn: &mut PgConnection, character: i64) -> Result<Vec<Stack>, String> {
    sqlx::query(&format!(
        "SELECT {COLUMNS} FROM items WHERE character_id=$1 ORDER BY id"
    ))
    .bind(character)
    .fetch_all(conn)
    .await
    .map_err(database_error)?
    .iter()
    .map(stack)
    .collect()
}
/// Stores a new stack and returns its id.
pub(crate) async fn insert(
    conn: &mut PgConnection,
    owner: Option<i64>,
    s: &Stack,
) -> Result<i64, String> {
    let (place, cell, container, x, z) = columns(s);
    sqlx::query_scalar("INSERT INTO items(kind,quantity,lock_code,place,character_id,cell,container_id,x,z,mass_mg) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id")
        .bind(i16::from(s.kind)).bind(i32::try_from(s.quantity).map_err(|_| "Слишком большая стопка")?)
        .bind(s.lock).bind(place).bind(owner_of(owner, s)).bind(cell).bind(container).bind(x).bind(z)
        .bind(s.mass_mg.and_then(|mg| i64::try_from(mg).ok()))
        .fetch_one(conn).await.map_err(database_error)
}
pub(crate) async fn contents(
    conn: &mut PgConnection,
    container: i64,
) -> Result<Vec<Stack>, String> {
    select(conn, "container_id", container).await
}
/// One stack lying on the ground, row-locked.
pub(crate) async fn loose(conn: &mut PgConnection, id: i64) -> Result<Option<Stack>, String> {
    sqlx::query(&format!(
        "SELECT {COLUMNS} FROM items WHERE id=$1 AND place='ground' FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(database_error)?
    .as_ref()
    .map(stack)
    .transpose()
}
/// Stacks on the ground within `radius` of a point (no locks: a read-only listing).
pub(crate) async fn ground(
    conn: &mut PgConnection,
    at: [f32; 2],
    radius: f32,
) -> Result<Vec<Stack>, String> {
    sqlx::query(&format!(
        "SELECT {COLUMNS} FROM items WHERE place='ground' \
         AND x BETWEEN $1-$3 AND $1+$3 AND z BETWEEN $2-$3 AND $2+$3 ORDER BY id LIMIT 400"
    ))
    .bind(at[0])
    .bind(at[1])
    .bind(radius)
    .fetch_all(conn)
    .await
    .map_err(database_error)?
    .iter()
    .map(stack)
    .collect()
}

/// One holder's stacks before and after a move. `owner` is the character for carried stacks.
pub(crate) struct Change<'a> {
    pub owner: Option<i64>,
    pub before: &'a [Stack],
    pub after: &'a [Stack],
}
fn columns(
    s: &Stack,
) -> (
    &'static str,
    Option<i32>,
    Option<i64>,
    Option<f32>,
    Option<f32>,
) {
    match s.location {
        Location::Bag { cell } => ("bag", Some(cell.into()), None, None, None),
        Location::Worn { slot } => (
            "worn",
            Some(i32::from(slot == Slot::Armor)),
            None,
            None,
            None,
        ),
        Location::Purse => ("purse", None, None, None, None),
        Location::Container { container, cell } => {
            ("container", Some(cell.into()), Some(container), None, None)
        }
        Location::Ground { x, z } => ("ground", None, None, Some(x), Some(z)),
    }
}
fn owner_of(owner: Option<i64>, s: &Stack) -> Option<i64> {
    match s.location {
        Location::Bag { .. } | Location::Worn { .. } | Location::Purse => owner,
        _ => None,
    }
}
/// Writes the difference of every involved holder at once: stacks moved between holders keep
/// their row, emptied stacks are deleted, split-off parts (id 0) are inserted.
pub(crate) async fn sync(conn: &mut PgConnection, changes: &[Change<'_>]) -> Result<(), String> {
    let mut before = HashMap::new();
    for change in changes {
        for s in change.before {
            before.insert(s.id, (owner_of(change.owner, s), *s));
        }
    }
    let mut kept = std::collections::HashSet::new();
    for change in changes {
        for s in change.after {
            let owner = owner_of(change.owner, s);
            if s.carried() || s.location == Location::Purse {
                owner.ok_or("Вещь без владельца")?;
            }
            if s.id == 0 {
                insert(conn, change.owner, s).await?;
                continue;
            }
            let (place, cell, container, x, z) = columns(s);
            if !kept.insert(s.id) {
                return Err("Одна вещь в двух местах".into());
            }
            match before.get(&s.id) {
                Some(old) if *old == (owner, *s) => {}
                Some(_) => {
                    sqlx::query("UPDATE items SET quantity=$2,lock_code=$3,place=$4,character_id=$5,cell=$6,container_id=$7,x=$8,z=$9,mass_mg=$10 WHERE id=$1")
                        .bind(s.id).bind(i32::try_from(s.quantity).map_err(|_| "Слишком большая стопка")?)
                        .bind(s.lock).bind(place).bind(owner).bind(cell).bind(container).bind(x).bind(z)
                        .bind(s.mass_mg.and_then(|mg| i64::try_from(mg).ok()))
                        .execute(&mut *conn).await.map_err(database_error)?;
                }
                None => return Err("Вещь не принадлежит участникам".into()),
            }
        }
    }
    let gone: Vec<i64> = before
        .keys()
        .filter(|id| !kept.contains(id))
        .copied()
        .collect();
    if !gone.is_empty() {
        sqlx::query("DELETE FROM items WHERE id = ANY($1)")
            .bind(&gone)
            .execute(&mut *conn)
            .await
            .map_err(database_error)?;
    }
    Ok(())
}

/// Runs `body` in one transaction and settles it before returning: commit on success, immediate
/// rollback on refusal. A dropped sqlx transaction only queues its ROLLBACK for the connection's
/// next query, so an idle session connection would keep its row locks and stall every player
/// waiting behind the simulation lock.
pub(crate) async fn transaction<T>(
    conn: &mut PgConnection,
    body: impl AsyncFnOnce(&mut PgConnection) -> Result<T, ExchangeFailure>,
) -> Result<T, ExchangeFailure> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    match body(&mut tx).await {
        Ok(value) => {
            tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
            Ok(value)
        }
        Err(e) => {
            tx.rollback().await.map_err(database_error)?;
            Err(e)
        }
    }
}
/// Locks the character row, checks the admin revision and returns the base sheet (no growth or
/// equipment bonuses) with the live progress of `actor` and the row-locked carried stacks.
pub(crate) async fn load(conn: &mut PgConnection, actor: &Actor) -> Result<Character, String> {
    let row = sqlx::query("SELECT * FROM characters WHERE id=$1 FOR NO KEY UPDATE")
        .bind(actor.id)
        .fetch_one(&mut *conn)
        .await
        .map_err(database_error)?;
    let mut stats: BTreeMap<String, f64> =
        serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
    if stats.get("admin_revision").copied().unwrap_or(0.)
        != f64::from(actor.progress.admin_revision)
    {
        return Err(crate::store::STALE_ADMIN.into());
    }
    actor.progress.write_stats(&mut stats);
    Ok(Character {
        id: actor.id,
        slot: row.get::<i16, _>("slot") as u8,
        nickname: row.get("nickname"),
        gender: row.get::<i16, _>("gender") as u8,
        stats,
        items: carried(conn, actor.id).await?,
    })
}
/// Takes a fee of `value` copper from the purse inside the caller's transaction. The coins
/// leave the world, as every fee did before coins became physical.
pub(crate) async fn pay(
    conn: &mut PgConnection,
    actor: &mut Actor,
    value: u64,
    status: &str,
) -> Result<Character, String> {
    let mut c = load(conn, actor).await?;
    let before = c.items.clone();
    mortown_protocol::transfer::pay(
        &mut c.items,
        mortown_protocol::transfer::Holder::Character,
        value,
    )?;
    sync(
        conn,
        &[Change {
            owner: Some(actor.id),
            before: &before,
            after: &c.items,
        }],
    )
    .await?;
    finish(conn, actor, &c, status).await
}
/// Saves the base sheet and the actor after `sync`, returning the derived sheet the client sees.
pub(crate) async fn finish(
    conn: &mut PgConnection,
    actor: &mut Actor,
    base: &Character,
    status: &str,
) -> Result<Character, String> {
    sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
        .bind(actor.id)
        .bind(serde_json::to_value(&base.stats).unwrap())
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;
    if !status.is_empty() {
        actor.reward_status = status.into();
    }
    crate::store::save(conn, actor).await?;
    let c = crate::store::sheet(conn, actor.id).await?;
    actor.equipment = mortown_protocol::items::EquipmentAppearance::from_items(&c.items);
    Ok(c)
}
