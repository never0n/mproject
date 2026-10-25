//! Claimed names, private notes, covered faces, letters and the bird that carries them.
//! A note is shown only to the person who wrote it, and only while the face is open.
use mortown_protocol::{
    items::{Location, Stack},
    society::{self, Command},
    transfer::{self, Holder},
};
use sqlx::{PgConnection, PgPool, Row};

fn database(error: sqlx::Error) -> String {
    eprintln!("Society database operation failed: {error}");
    "Сведения временно недоступны".into()
}

pub async fn notes(
    conn: &mut PgConnection,
    owner: i64,
) -> Result<std::collections::HashMap<i64, (f64, String)>, String> {
    let rows = sqlx::query("SELECT person_id, identity, note FROM acquaintances WHERE owner_id=$1")
        .bind(owner)
        .fetch_all(&mut *conn)
        .await
        .map_err(database)?;
    Ok(rows
        .iter()
        .map(|row| (row.get("person_id"), (row.get("identity"), row.get("note"))))
        .collect())
}

pub async fn presentation(conn: &mut PgConnection, id: i64) -> Result<(String, bool), String> {
    let row = sqlx::query("SELECT presented, face_hidden FROM characters WHERE id=$1")
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(database)?;
    Ok((row.get("presented"), row.get("face_hidden")))
}

pub async fn apply(
    conn: &mut PgConnection,
    actor: &mortown_protocol::Actor,
    command: Command,
) -> Result<String, String> {
    match command {
        Command::Present { name } => {
            let name = society::claim(&name)?;
            sqlx::query("UPDATE characters SET presented=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(name)
                .execute(&mut *conn)
                .await
                .map_err(database)?;
            Ok("Вы назвали имя. Это не удостоверяет личность".into())
        }
        Command::Cover { hidden } => {
            sqlx::query("UPDATE characters SET face_hidden=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(hidden)
                .execute(&mut *conn)
                .await
                .map_err(database)?;
            Ok(if hidden {
                "Лицо закрыто. Знакомые не получают подсказку".into()
            } else {
                "Лицо открыто".into()
            })
        }
        Command::Remember { person, note } => {
            let note = society::note(&note)?;
            if person == actor.id {
                return Err("Себя помечать не нужно".into());
            }
            let row = sqlx::query(
                "SELECT COALESCE((stats->>'identity')::float8, 1) AS identity, face_hidden, x, z \
                 FROM characters WHERE id=$1",
            )
            .bind(person)
            .fetch_optional(&mut *conn)
            .await
            .map_err(database)?
            .ok_or("Человека рядом нет")?;
            if row.get::<bool, _>("face_hidden") {
                return Err("Лицо закрыто: заметка не запоминается сквозь маску".into());
            }
            let at = [row.get("x"), row.get("z")];
            let from = [actor.position[0], actor.position[2]];
            if mortown_world::distance(from, at) > mortown_protocol::objects::REACH {
                return Err("Подойдите ближе, чтобы запомнить лицо".into());
            }
            sqlx::query(
                "INSERT INTO acquaintances(owner_id, person_id, note, identity) VALUES($1,$2,$3,$4) \
                 ON CONFLICT(owner_id, person_id) DO UPDATE SET note=EXCLUDED.note, identity=EXCLUDED.identity",
            )
            .bind(actor.id)
            .bind(person)
            .bind(note)
            .bind(row.get::<f64, _>("identity"))
            .execute(&mut *conn)
            .await
            .map_err(database)?;
            Ok("Заметка только ваша. Она не называет человека остальным".into())
        }
    }
}

pub fn write_letter(
    character: &mut mortown_protocol::Character,
    text: &str,
) -> Result<String, String> {
    let text = mortown_protocol::chat::validate(text)?;
    transfer::consume(&mut character.items, mortown_protocol::work::WOOD, 1)?;
    transfer::put(
        &mut character.items,
        Holder::Character,
        Stack::new(society::DOCUMENT, 1, Location::Bag { cell: 0 }),
    )?;
    Ok(text.to_string())
}

pub async fn attach_letter(tx: &mut PgConnection, author: i64, text: &str) -> Result<(), String> {
    let item: Option<i64> = sqlx::query_scalar(
        "SELECT i.id FROM items i LEFT JOIN documents d ON d.item_id = i.id \
         WHERE i.character_id=$1 AND i.kind=$2 AND d.item_id IS NULL ORDER BY i.id DESC LIMIT 1",
    )
    .bind(author)
    .bind(i16::from(society::DOCUMENT))
    .fetch_optional(&mut *tx)
    .await
    .map_err(database)?;
    let item = item.ok_or("Письмо не сохранилось")?;
    let identity: f64 = sqlx::query_scalar(
        "SELECT COALESCE((stats->>'identity')::float8, 1) FROM characters WHERE id=$1",
    )
    .bind(author)
    .fetch_one(&mut *tx)
    .await
    .map_err(database)?;
    sqlx::query("INSERT INTO documents(item_id, body, author_id, identity) VALUES($1,$2,$3,$4)")
        .bind(item)
        .bind(text)
        .bind(author)
        .bind(identity)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    Ok(())
}

pub fn dispatch_letter(
    world: &mortown_world::World,
    actor: &mortown_protocol::Actor,
    character: &mut mortown_protocol::Character,
) -> Result<(i64, [f32; 2]), String> {
    let from = [actor.position[0], actor.position[2]];
    let dest = [
        from[0] + actor.yaw.sin() * society::BIRD_RANGE,
        from[1] + actor.yaw.cos() * society::BIRD_RANGE,
    ];
    if !world.walkable(dest[0], dest[1]) || !world.segment_clear(from, dest) {
        return Err("Птица не видит путь".into());
    }
    let item = character
        .items
        .iter_mut()
        .find(|stack| {
            stack.kind == society::DOCUMENT && matches!(stack.location, Location::Bag { .. })
        })
        .ok_or("В сумке нет письма")?;
    let id = item.id;
    item.location = Location::Ground {
        x: from[0],
        z: from[1],
    };
    Ok((id, dest))
}

pub async fn launch(tx: &mut PgConnection, item: i64, dest: [f32; 2]) -> Result<String, String> {
    sqlx::query("INSERT INTO couriers(item_id, dest_x, dest_z) VALUES($1,$2,$3)")
        .bind(item)
        .bind(dest[0])
        .bind(dest[1])
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    Ok("Птица несёт письмо. Его можно перехватить по дороге".into())
}

pub async fn fly(pool: &PgPool, world: &mortown_world::World) -> Result<bool, String> {
    let mut conn = pool.acquire().await.map_err(database)?;
    let rows = sqlx::query(
        "SELECT c.item_id, i.x, i.z, c.dest_x, c.dest_z FROM couriers c \
         JOIN items i ON i.id = c.item_id WHERE i.place = 'ground'",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(database)?;
    let moved = !rows.is_empty();
    for row in &rows {
        let item: i64 = row.get("item_id");
        let from = [row.get("x"), row.get("z")];
        let dest = [row.get("dest_x"), row.get("dest_z")];
        let (next, arrived) = society::hop(from, dest);
        if !world.segment_clear(from, next) {
            sqlx::query("DELETE FROM couriers WHERE item_id=$1")
                .bind(item)
                .execute(&mut *conn)
                .await
                .map_err(database)?;
            continue;
        }
        sqlx::query("UPDATE items SET x=$2, z=$3 WHERE id=$1 AND place='ground'")
            .bind(item)
            .bind(next[0])
            .bind(next[1])
            .execute(&mut *conn)
            .await
            .map_err(database)?;
        if arrived {
            sqlx::query("DELETE FROM couriers WHERE item_id=$1")
                .bind(item)
                .execute(&mut *conn)
                .await
                .map_err(database)?;
        }
    }
    sqlx::query(
        "DELETE FROM couriers WHERE item_id NOT IN (SELECT id FROM items WHERE place='ground')",
    )
    .execute(&mut *conn)
    .await
    .map_err(database)?;
    Ok(moved)
}
