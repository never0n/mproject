//! Timber blocks and the one-time grant of the starting plots.
//! A block stands only on walkable ground. Hitting it reduces integrity until it is gone.
//! Nothing decays by itself, and a restart does not put a destroyed block back.
use crate::inventory::database_error;
use mortown_protocol::{
    items::{Location, Stack},
    transfer::{self, Holder},
    work::{self, Block},
};
use sqlx::{PgConnection, PgPool, Row};

pub async fn grant_starting_plots(conn: &mut PgConnection) -> Result<(), String> {
    let granted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM world_grants WHERE name = 'starting_plots')",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(database_error)?;
    if granted {
        return Ok(());
    }
    let owner: Option<i64> = sqlx::query_scalar(
        "SELECT c.id FROM characters c JOIN accounts a ON a.id = c.account_id \
         ORDER BY a.role DESC, c.id ASC LIMIT 1",
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(database_error)?;
    let Some(owner) = owner else {
        return Ok(());
    };
    sqlx::query("UPDATE plots SET owner_id = $1 WHERE owner_id IS NULL")
        .bind(owner)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;
    sqlx::query("INSERT INTO world_grants(name, character_id) VALUES ('starting_plots', $1)")
        .bind(owner)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;
    Ok(())
}

pub async fn grant_pool(pool: &PgPool) -> Result<(), String> {
    let mut conn = pool.acquire().await.map_err(crate::store::database_error)?;
    grant_starting_plots(&mut conn).await
}

pub async fn nearby(conn: &mut PgConnection, at: [f32; 2]) -> Result<Vec<Block>, String> {
    let radius = mortown_protocol::objects::VIEW_RANGE;
    let rows = sqlx::query(
        "SELECT id, x, z, integrity, level FROM blocks \
         WHERE x BETWEEN $1-$3 AND $1+$3 AND z BETWEEN $2-$3 AND $2+$3 ORDER BY id LIMIT 200",
    )
    .bind(at[0])
    .bind(at[1])
    .bind(radius)
    .fetch_all(&mut *conn)
    .await
    .map_err(database_error)?;
    Ok(rows
        .iter()
        .map(|row| Block {
            id: row.get("id"),
            position: [row.get("x"), row.get("z")],
            integrity: row.get::<i16, _>("integrity") as u8,
            level: row.get::<i16, _>("level") as u8,
        })
        .collect())
}

pub async fn all(conn: &mut PgConnection) -> Result<Vec<Block>, String> {
    let rows = sqlx::query("SELECT id, x, z, integrity, level FROM blocks ORDER BY id LIMIT 2000")
        .fetch_all(&mut *conn)
        .await
        .map_err(database_error)?;
    Ok(rows
        .iter()
        .map(|row| Block {
            id: row.get("id"),
            position: [row.get("x"), row.get("z")],
            integrity: row.get::<i16, _>("integrity") as u8,
            level: row.get::<i16, _>("level") as u8,
        })
        .collect())
}

pub fn make_timber(character: &mut mortown_protocol::Character) -> Result<String, String> {
    transfer::consume(&mut character.items, work::WOOD, 4)?;
    transfer::put(
        &mut character.items,
        Holder::Character,
        Stack::new(work::TIMBER, 1, Location::Bag { cell: 0 }),
    )?;
    Ok("Собран строительный брус".into())
}

pub fn make_cart(character: &mut mortown_protocol::Character) -> Result<String, String> {
    transfer::consume(&mut character.items, work::WOOD, 8)?;
    transfer::put(
        &mut character.items,
        Holder::Character,
        Stack::new(mortown_protocol::items::CART, 1, Location::Bag { cell: 0 }),
    )?;
    Ok("Сколочена телега".into())
}

pub async fn place(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    actor: &mortown_protocol::Actor,
    character: &mut mortown_protocol::Character,
) -> Result<String, String> {
    transfer::consume(&mut character.items, work::TIMBER, 1)?;
    let from = [actor.position[0], actor.position[2]];
    let at = [
        from[0] + actor.yaw.sin() * 1.5,
        from[1] + actor.yaw.cos() * 1.5,
    ];
    if !world.walkable(at[0], at[1]) || world.hall_at(at[0], at[1]).is_some() {
        return Err("Здесь брус не на что опереть".into());
    }
    let below = sqlx::query(
        "SELECT x, z, level FROM blocks WHERE (x-$1)*(x-$1)+(z-$2)*(z-$2) < $3 ORDER BY level DESC LIMIT 1",
    )
    .bind(at[0])
    .bind(at[1])
    .bind(work::COLUMN * work::COLUMN)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?;
    let (at, level, message) = if let Some(row) = below {
        let level = row.get::<i16, _>("level") as u8 + 1;
        if level >= work::BLOCK_LEVELS {
            return Err("Выше эту колонну не на что опереть".into());
        }
        (
            [row.get("x"), row.get("z")],
            level,
            "Брус поставлен на опору. Без неё он упадёт",
        )
    } else {
        (at, 0, "Брус поставлен. Он стоит, пока его не разобьют")
    };
    sqlx::query("INSERT INTO blocks(x, z, integrity, placed_by, level) VALUES ($1,$2,$3,$4,$5)")
        .bind(at[0])
        .bind(at[1])
        .bind(i16::from(work::BLOCK_INTEGRITY))
        .bind(actor.id)
        .bind(i16::from(level))
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    Ok(message.into())
}

pub async fn damage(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    actor: &mortown_protocol::Actor,
    block: i64,
) -> Result<String, String> {
    let row = sqlx::query("SELECT x, z, integrity FROM blocks WHERE id=$1 FOR UPDATE")
        .bind(block)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or("Бруса уже нет")?;
    let at = [row.get("x"), row.get("z")];
    let from = [actor.position[0], actor.position[2]];
    if mortown_world::distance(from, at) > mortown_protocol::objects::REACH
        || !world.segment_clear(from, at)
    {
        return Err("Слишком далеко".into());
    }
    let left = row.get::<i16, _>("integrity") - 1;
    if left <= 0 {
        let standing = sqlx::query("SELECT id, x, z, integrity, level FROM blocks ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .map_err(database_error)?;
        let blocks: Vec<Block> = standing
            .iter()
            .map(|row| Block {
                id: row.get("id"),
                position: [row.get("x"), row.get("z")],
                integrity: row.get::<i16, _>("integrity") as u8,
                level: row.get::<i16, _>("level") as u8,
            })
            .collect();
        let fallen = work::collapse(&blocks, block);
        sqlx::query("DELETE FROM blocks WHERE id=$1")
            .bind(block)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        let mut dropped = Vec::new();
        for id in &fallen {
            let spot = blocks
                .iter()
                .find(|beam| beam.id == *id)
                .map(|beam| beam.position)
                .unwrap_or(at);
            sqlx::query("DELETE FROM blocks WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
            dropped.push(Stack::new(
                work::TIMBER,
                1,
                Location::Ground {
                    x: spot[0],
                    z: spot[1],
                },
            ));
        }
        if !dropped.is_empty() {
            crate::inventory::sync(
                &mut *tx,
                &[crate::inventory::Change {
                    owner: None,
                    before: &[],
                    after: &dropped,
                }],
            )
            .await?;
        }
        Ok(if fallen.is_empty() {
            "Брус разрушен".into()
        } else {
            format!("Брус разрушен. Упало без опоры: {}", fallen.len())
        })
    } else {
        sqlx::query("UPDATE blocks SET integrity=$2 WHERE id=$1")
            .bind(block)
            .bind(left)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        Ok(format!("Брус повреждён, осталось {left}"))
    }
}

/// Pushes the nearest cart by one step. The hauler and every container within
/// [work::CARGO] move by the same delta, so a chest keeps its offset.
pub async fn haul(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    actor: &mut mortown_protocol::Actor,
) -> Result<String, String> {
    let from = [actor.position[0], actor.position[2]];
    let reach = mortown_protocol::objects::REACH;
    let cart = sqlx::query(
        "SELECT id, x, z FROM world_objects WHERE kind=$1 \
         AND (x-$2)*(x-$2)+(z-$3)*(z-$3) <= $4 ORDER BY (x-$2)*(x-$2)+(z-$3)*(z-$3) LIMIT 1 FOR UPDATE",
    )
    .bind(i16::from(mortown_protocol::items::CART))
    .bind(from[0])
    .bind(from[1])
    .bind(reach * reach)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?
    .ok_or("Рядом нет телеги")?;
    let cart_id: i64 = cart.get("id");
    let origin = [cart.get("x"), cart.get("z")];
    if mortown_world::distance(from, origin) > reach || !world.segment_clear(from, origin) {
        return Err("Слишком далеко".into());
    }
    let step = work::HAUL;
    let delta = [actor.yaw.sin() * step, actor.yaw.cos() * step];
    let cargo = sqlx::query(
        "SELECT id, x, z FROM world_objects WHERE id<>$1 AND (x-$2)*(x-$2)+(z-$3)*(z-$3) < $4 ORDER BY id FOR UPDATE",
    )
    .bind(cart_id)
    .bind(origin[0])
    .bind(origin[1])
    .bind(work::CARGO * work::CARGO)
    .fetch_all(&mut *tx)
    .await
    .map_err(database_error)?;
    let mut moves = vec![(cart_id, [origin[0] + delta[0], origin[1] + delta[1]])];
    for row in &cargo {
        let at: [f32; 2] = [row.get("x"), row.get("z")];
        moves.push((row.get("id"), [at[0] + delta[0], at[1] + delta[1]]));
    }
    let actor_at = [from[0] + delta[0], from[1] + delta[1]];
    let clear =
        |at: [f32; 2]| world.walkable(at[0], at[1]) && world.hall_at(at[0], at[1]).is_none();
    if !clear(actor_at) || moves.iter().any(|(_, at)| !clear(*at)) {
        return Err("Телеге дальше не проехать".into());
    }
    for (id, at) in &moves {
        sqlx::query("UPDATE world_objects SET x=$2, z=$3 WHERE id=$1")
            .bind(id)
            .bind(at[0])
            .bind(at[1])
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
    }
    actor.position = [
        actor_at[0],
        world.surface_height(actor_at[0], actor_at[1]),
        actor_at[1],
    ];
    Ok(if cargo.is_empty() {
        "Телега сдвинута".into()
    } else {
        "Телега сдвинута. Груз поехал вместе с ней".into()
    })
}
