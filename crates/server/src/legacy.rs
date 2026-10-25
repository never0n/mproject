//! Property left in the removed mail and alliance warehouse (migration 0017).
//! `store::convert_legacy_items` releases it into the physical world at startup; while a record
//! is still unreleased its owner cannot be erased or dissolved.
use sqlx::PgConnection;

pub const KEPT: &str = "За владельцем числится невыданное имущество из старой почты или склада \
альянса. Перезапустите сервер, чтобы выдать его, и повторите удаление";

pub async fn character_owns(conn: &mut PgConnection, id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM legacy_property WHERE character_id=$1 AND released_at IS NULL)",
    )
    .bind(id)
    .fetch_one(conn)
    .await
}

pub async fn alliance_owns(conn: &mut PgConnection, id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM legacy_property WHERE alliance_id=$1 AND released_at IS NULL)",
    )
    .bind(id)
    .fetch_one(conn)
    .await
}

pub struct Report {
    pub records: i64,
    pub items: i64,
    pub coins: i64,
    pub owners: i64,
}

pub async fn report(conn: &mut PgConnection) -> Result<Report, sqlx::Error> {
    let (records, items, coins, owners) = sqlx::query_as(
        "SELECT count(*), coalesce(sum(quantity),0)::bigint, coalesce(sum(coins),0)::bigint, \
         count(DISTINCT coalesce(character_id, -alliance_id)) \
         FROM legacy_property WHERE released_at IS NULL",
    )
    .fetch_one(conn)
    .await?;
    Ok(Report {
        records,
        items,
        coins,
        owners,
    })
}
