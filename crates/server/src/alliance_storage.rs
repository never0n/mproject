//! All mutations run inside alliance::apply's transaction and advisory lock.
use crate::alliance::database;
use mortown_protocol::{
    Actor,
    alliance::{self, Storage, StorageEntry, StorageEvent},
    items, mail, transfer,
};
use sqlx::{PgConnection, Row};

pub async fn read(conn: &mut PgConnection, id: i64) -> Result<Storage, String> {
    let revision = sqlx::query_scalar("SELECT storage_revision FROM alliances WHERE id=$1")
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(database)?;
    let rows = sqlx::query(
        "SELECT id,slot,kind,quantity FROM alliance_storage WHERE alliance_id=$1 ORDER BY slot",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(database)?;
    let events = sqlx::query("SELECT nickname,deposit,kind,quantity,to_char(created_at AT TIME ZONE 'UTC','DD.MM HH24:MI') AS time FROM alliance_storage_events WHERE alliance_id=$1 ORDER BY id DESC LIMIT 8")
        .bind(id).fetch_all(&mut *conn).await.map_err(database)?;
    Ok(Storage {
        revision,
        entries: rows
            .iter()
            .map(|r| StorageEntry {
                id: r.get("id"),
                slot: r.get::<i32, _>("slot") as usize,
                kind: r.get::<i16, _>("kind") as u8,
                quantity: r.get::<i32, _>("quantity") as u32,
            })
            .collect(),
        history: events
            .iter()
            .map(|r| StorageEvent {
                nickname: r.get("nickname"),
                deposit: r.get("deposit"),
                kind: r.get::<i16, _>("kind") as u8,
                quantity: r.get::<i32, _>("quantity") as u32,
                time: r.get("time"),
            })
            .collect(),
    })
}
async fn check(
    conn: &mut PgConnection,
    actor: &Actor,
    id: i64,
    expected: i64,
) -> Result<(), String> {
    if actor.hp <= 0. {
        return Err("Сначала возродитесь".into());
    }
    let revision: i64 = sqlx::query_scalar("SELECT storage_revision FROM alliances WHERE id=$1")
        .bind(id)
        .fetch_one(conn)
        .await
        .map_err(database)?;
    if revision != expected {
        return Err("Содержимое склада изменилось. Выберите предмет заново".into());
    }
    Ok(())
}
async fn log(
    conn: &mut PgConnection,
    actor: &Actor,
    id: i64,
    kind: u8,
    quantity: u32,
    deposit: bool,
) -> Result<(), String> {
    sqlx::query("UPDATE alliances SET storage_revision=storage_revision+1 WHERE id=$1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(database)?;
    sqlx::query("INSERT INTO alliance_storage_events(alliance_id,character_id,nickname,deposit,kind,quantity) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(actor.id).bind(&actor.nickname).bind(deposit).bind(kind as i16).bind(quantity as i32)
        .execute(conn).await.map_err(database)?;
    Ok(())
}
pub async fn deposit(
    conn: &mut PgConnection,
    actor: Actor,
    id: i64,
    revision: i64,
    item: mail::AttachmentRequest,
) -> Result<Box<mail::InventoryUpdate>, String> {
    check(conn, &actor, id, revision).await?;
    if item.price != 0 {
        return Err("У вклада на склад нет цены".into());
    }
    let mut c = crate::mail::inventory(conn, &actor).await?;
    let kind = transfer::take(&mut c, &item)?;
    let existing = if items::equippable(kind) {
        None
    } else {
        sqlx::query("SELECT id,quantity FROM alliance_storage WHERE alliance_id=$1 AND kind=$2")
            .bind(id)
            .bind(kind as i16)
            .fetch_optional(&mut *conn)
            .await
            .map_err(database)?
    };
    if let Some(row) = existing {
        let quantity = row.get::<i32, _>("quantity") as u32 + item.quantity;
        if quantity > transfer::MAX_AMOUNT {
            return Err("На складе не больше 9999 предметов в одной стопке".into());
        }
        sqlx::query("UPDATE alliance_storage SET quantity=$2 WHERE id=$1")
            .bind(row.get::<i64, _>("id"))
            .bind(quantity as i32)
            .execute(&mut *conn)
            .await
            .map_err(database)?;
    } else {
        let occupied: Vec<i32> =
            sqlx::query_scalar("SELECT slot FROM alliance_storage WHERE alliance_id=$1")
                .bind(id)
                .fetch_all(&mut *conn)
                .await
                .map_err(database)?;
        let slot = (0..alliance::STORAGE_SLOTS as i32)
            .find(|s| !occupied.contains(s))
            .ok_or("Склад заполнен: все 50 ячеек заняты")?;
        sqlx::query(
            "INSERT INTO alliance_storage(alliance_id,slot,kind,quantity) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(slot)
        .bind(kind as i16)
        .bind(item.quantity as i32)
        .execute(&mut *conn)
        .await
        .map_err(database)?;
    }
    log(conn, &actor, id, kind, item.quantity, true).await?;
    crate::mail::persist(conn, actor, c, "Предметы переданы на склад альянса").await
}
pub async fn withdraw(
    conn: &mut PgConnection,
    actor: Actor,
    id: i64,
    revision: i64,
    entry: i64,
    quantity: u32,
) -> Result<Box<mail::InventoryUpdate>, String> {
    check(conn, &actor, id, revision).await?;
    let row =
        sqlx::query("SELECT kind,quantity FROM alliance_storage WHERE id=$1 AND alliance_id=$2")
            .bind(entry)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(database)?
            .ok_or("Предмет уже забрали со склада")?;
    let available = row.get::<i32, _>("quantity") as u32;
    let kind = row.get::<i16, _>("kind") as u8;
    if quantity == 0 || quantity > available {
        return Err("На складе нет такого количества".into());
    }
    let mut c = crate::mail::inventory(conn, &actor).await?;
    transfer::give(&mut c, kind, quantity)?;
    if quantity == available {
        sqlx::query("DELETE FROM alliance_storage WHERE id=$1")
            .bind(entry)
            .execute(&mut *conn)
            .await
            .map_err(database)?;
    } else {
        sqlx::query("UPDATE alliance_storage SET quantity=quantity-$2 WHERE id=$1")
            .bind(entry)
            .bind(quantity as i32)
            .execute(&mut *conn)
            .await
            .map_err(database)?;
    }
    log(conn, &actor, id, kind, quantity, false).await?;
    crate::mail::persist(conn, actor, c, "Предметы получены со склада альянса").await
}
