use std::collections::BTreeMap;

use mortown_protocol::{
    items::KEY,
    life::{self, now_millis},
};
use sqlx::{PgConnection, PgPool, Row};

use crate::{inventory, store};

/// One finished life. The same `mark` a second time changes nothing, and carried stacks are
/// never copied: only keys on the body are removed.
pub async fn apply(
    pool: &PgPool,
    mark: u64,
    id: i64,
) -> Result<Option<BTreeMap<String, f64>>, String> {
    let mut connection = pool.acquire().await.map_err(store::database_error)?;
    inventory::transaction(&mut connection, async move |tx| {
        change(tx, id, mark)
            .await
            .map_err(store::ExchangeFailure::Rejected)
    })
    .await
    .map_err(|failure| match failure {
        store::ExchangeFailure::Rejected(message) => message,
        store::ExchangeFailure::Uncertain => "Не удалось подтвердить смерть".into(),
    })
}

async fn change(
    tx: &mut PgConnection,
    id: i64,
    mark: u64,
) -> Result<Option<BTreeMap<String, f64>>, String> {
    let row = sqlx::query("SELECT stats FROM characters WHERE id=$1 FOR NO KEY UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(store::database_error)?;
    let mut stats: BTreeMap<String, f64> =
        serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
    if stats.get("applied_death").copied().unwrap_or(0.) >= mark as f64 {
        return Ok(Some(stats));
    }
    let lives = stats
        .get("lives")
        .copied()
        .unwrap_or(f64::from(life::lives())) as u32;
    stats.insert("applied_death".into(), mark as f64);
    if lives <= 1 {
        stats.insert("lives".into(), 0.);
        stats.insert("ended".into(), 1.);
        stats.insert("current_hp".into(), 0.);
        write(tx, id, &stats, None).await?;
        return Ok(Some(stats));
    }
    stats.insert("lives".into(), f64::from(lives - 1));
    let previous = stats.get("identity").copied().unwrap_or(1.);
    let identity = previous + 1.;
    stats.insert("identity".into(), identity);
    sqlx::query(
        "DELETE FROM items WHERE id IN (SELECT item_id FROM documents WHERE author_id=$1 AND identity=$2)",
    )
    .bind(id)
    .bind(previous)
    .execute(&mut *tx)
    .await
    .map_err(store::database_error)?;
    life::reduce_numbers(&mut stats);
    let primary = ["STR", "DEX", "CON", "INT_", "WIT", "MEN"]
        .into_iter()
        .map(|key| (key.to_string(), stats.get(key).copied().unwrap_or(10.)))
        .collect();
    mortown_protocol::attributes::rebase(&mut stats, &primary)
        .map_err(|message| message.to_string())?;
    let level = stats.get("level").copied().unwrap_or(1.) as u32;
    let mut derived = stats.clone();
    mortown_protocol::progression::grow(&mut derived, level.saturating_sub(1));
    stats.insert(
        "current_hp".into(),
        derived.get("hp").copied().unwrap_or(1.).max(1.),
    );
    stats.insert("current_mana".into(), derived["mana"]);
    stats.insert("current_stamina".into(), derived["stamina"]);
    stats.insert(
        "return_until".into(),
        now_millis().saturating_add(life::return_millis()) as f64,
    );
    sqlx::query("UPDATE characters SET presented='', face_hidden=false WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(store::database_error)?;
    sqlx::query("DELETE FROM items WHERE character_id=$1 AND kind=$2 AND place IN ('bag', 'worn')")
        .bind(id)
        .bind(i32::from(KEY))
        .execute(&mut *tx)
        .await
        .map_err(store::database_error)?;
    sqlx::query(
        "DELETE FROM alliance_members WHERE character_id=$1 AND NOT EXISTS \
         (SELECT 1 FROM alliances WHERE leader_id=$1)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(store::database_error)?;
    write(tx, id, &stats, Some(mortown_world::SPAWN)).await?;
    Ok(Some(stats))
}

async fn write(
    tx: &mut PgConnection,
    id: i64,
    stats: &BTreeMap<String, f64>,
    point: Option<[f32; 2]>,
) -> Result<(), String> {
    let value = serde_json::to_value(stats).unwrap();
    if let Some(point) = point {
        sqlx::query("UPDATE characters SET stats=$2, x=$3, z=$4 WHERE id=$1")
            .bind(id)
            .bind(value)
            .bind(point[0])
            .bind(point[1])
            .execute(&mut *tx)
            .await
            .map_err(store::database_error)?;
    } else {
        sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
            .bind(id)
            .bind(value)
            .execute(&mut *tx)
            .await
            .map_err(store::database_error)?;
    }
    Ok(())
}
