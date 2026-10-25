use crate::inventory::{self, Change};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use mortown_protocol::{
    Actor, Character,
    items::{self, Location, Stack},
    starting_stats,
    transfer::{self, Holder},
    validate_character, validate_credentials,
};
use rand_core::OsRng;
use sqlx::{Connection, PgConnection, PgPool, Row};
use std::collections::BTreeMap;

pub struct Registration {
    pub login: String,
    pub password: String,
}
pub async fn register(pool: &PgPool, r: Registration) -> Result<i64, String> {
    validate_credentials(&r.login, &r.password)?;
    let hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(r.password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|h| h.to_string())
            .map_err(|_| "Ошибка хеширования".to_owned())
    })
    .await
    .map_err(|_| "Ошибка регистрации")??;
    let mut tx = pool.begin().await.map_err(database_error)?;
    let account: i64 = sqlx::query_scalar(
        "INSERT INTO accounts(login,password_hash) VALUES(lower($1),$2) RETURNING id",
    )
    .bind(&r.login)
    .bind(hash)
    .fetch_one(&mut *tx)
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(account)
}
pub async fn login(pool: &PgPool, login: String, password: String) -> Result<i64, String> {
    validate_credentials(&login, &password)?;
    let row = sqlx::query("SELECT id,password_hash FROM accounts WHERE lower(login)=lower($1)")
        .bind(login)
        .fetch_optional(pool)
        .await
        .map_err(database_error)?;
    let id = row.as_ref().map(|r| r.get::<i64, _>("id"));
    // A valid dummy hash gives unknown accounts the same password-verification work.
    let hash = row.map(|r| r.get::<String,_>("password_hash")).unwrap_or_else(|| "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzYWx0MTIzNA$wGD1Rd7dkcmHPrBFcfqsXC1v1/eUWgxPNEFKDjA8LPc".into());
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .map_err(|_| "Ошибка входа")?;
    if !valid {
        return Err("Неверный логин или пароль".into());
    }
    id.ok_or_else(|| "Неверный логин или пароль".into())
}
pub struct CharacterCreation {
    pub nickname: String,
    pub gender: u8,
}
pub async fn create_character(
    conn: &mut PgConnection,
    account: i64,
    c: CharacterCreation,
) -> Result<i64, String> {
    validate_character(&c.nickname, c.gender)?;
    let mut tx = conn.begin().await.map_err(database_error)?;
    // Serialize allocation even across independent service connections.
    sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR UPDATE")
        .bind(account)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let slot: Option<i32>=sqlx::query_scalar("SELECT n FROM generate_series(1,12) n WHERE NOT EXISTS (SELECT 1 FROM characters WHERE account_id=$1 AND slot=n) ORDER BY n LIMIT 1").bind(account).fetch_optional(&mut *tx).await.map_err(database_error)?;
    let slot = slot.ok_or("На аккаунте уже 12 персонажей")?;
    let id=sqlx::query_scalar("INSERT INTO characters(account_id,slot,nickname,gender,stats,x,z) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(account).bind(slot as i16).bind(c.nickname).bind(i16::from(c.gender))
        .bind(serde_json::to_value(starting_stats().unwrap()).unwrap()).bind(mortown_world::SPAWN[0]).bind(mortown_world::SPAWN[1])
        .fetch_one(&mut *tx).await.map_err(database_error)?;
    let mut kit: Vec<Stack> = items::STARTER
        .iter()
        .enumerate()
        .map(|(n, &kind)| Stack::new(kind, 1, Location::Bag { cell: n as u16 }))
        .collect();
    transfer::set_coins(
        &mut kit,
        Holder::Character,
        mortown_protocol::starting_coins(),
    )?;
    inventory::sync(
        &mut tx,
        &[Change {
            owner: Some(id),
            before: &[],
            after: &kit,
        }],
    )
    .await?;
    tx.commit().await.map_err(database_error)?;
    Ok(id)
}
/// Applies `legacy::humanize` once to every character archived by migration 0016.
/// One transaction: a crash leaves either all or none of them converted.
pub async fn convert_legacy_characters(conn: &mut PgConnection) -> Result<u64, sqlx::Error> {
    let mut tx = conn.begin().await?;
    let rows = sqlx::query(
        "SELECT a.id, a.character_id, a.data FROM legacy_character_archive a \
         JOIN characters c ON c.id = a.character_id \
         WHERE a.migration='0016_human_characters' AND a.converted_at IS NULL FOR UPDATE",
    )
    .fetch_all(&mut *tx)
    .await?;
    for row in &rows {
        let data: serde_json::Value = row.get("data");
        let invalid =
            |e: serde_json::Error| sqlx::Error::Decode(format!("legacy data: {e}").into());
        let small = |key: &str| data[key].as_u64().unwrap_or(0) as u8;
        let converted = mortown_protocol::legacy::humanize(
            small("race"),
            small("class"),
            serde_json::from_value(data["stats"].clone()).map_err(invalid)?,
            serde_json::from_value(data["inventory"].clone()).map_err(invalid)?,
            serde_json::from_value(data["loot"].clone()).map_err(invalid)?,
        )
        .map_err(|e| sqlx::Error::Protocol(e.into()))?;
        sqlx::query("UPDATE characters SET stats=$2, inventory=$3, loot=$4 WHERE id=$1")
            .bind(row.get::<i64, _>("character_id"))
            .bind(serde_json::to_value(converted.stats).unwrap())
            .bind(serde_json::to_value(converted.inventory).unwrap())
            .bind(serde_json::to_value(converted.loot).unwrap())
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE legacy_character_archive SET converted_at=now() WHERE id=$1")
            .bind(row.get::<i64, _>("id"))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(rows.len() as u64)
}
/// Archives nothing itself: migration 0019 already stored the pre-conversion stats.
/// Removes the old skill-point currency, keeps purchased skill flags and items, and grants
/// talent points from the current level by the new per-level rule. Unspent SP is not turned
/// into talents or class bonuses; it remains only in the archive row.
pub async fn convert_legacy_progress(conn: &mut PgConnection) -> Result<u64, String> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    let rows = sqlx::query(
        "SELECT a.id AS archive, c.id, c.stats \
         FROM legacy_character_archive a JOIN characters c ON c.id = a.character_id \
         WHERE a.migration='0019_progression' AND a.converted_at IS NULL \
         ORDER BY c.id FOR UPDATE",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(database_error)?;
    for row in &rows {
        let id: i64 = row.get("id");
        let mut stats: BTreeMap<String, f64> = serde_json::from_value(row.get("stats"))
            .map_err(|_| format!("Не удалось прочитать прогресс персонажа {id}"))?;
        stats.remove("sp");
        if !stats.contains_key("talent_point") {
            let level = stats.get("level").copied().unwrap_or(1.).max(1.) as u32;
            stats.insert(
                "talent_point".into(),
                mortown_protocol::progression::talent_points_for_level(level),
            );
        }
        sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
            .bind(id)
            .bind(serde_json::to_value(&stats).unwrap())
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        sqlx::query("UPDATE legacy_character_archive SET converted_at=now() WHERE id=$1")
            .bind(row.get::<i64, _>("archive"))
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
    }
    tx.commit().await.map_err(database_error)?;
    Ok(rows.len() as u64)
}
/// Turns every inventory archived by migration 0018 into physical stacks (`legacy::physical`)
/// and releases the property recorded by migration 0017 into the world. Runs after
/// `convert_legacy_characters`, in one transaction. Returns the number of characters converted.
pub async fn convert_legacy_items(
    conn: &mut PgConnection,
    world: &mortown_world::World,
) -> Result<u64, String> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    let rows = sqlx::query(
        "SELECT a.id AS archive, c.id, c.stats, c.inventory, c.loot \
         FROM legacy_character_archive a JOIN characters c ON c.id = a.character_id \
         WHERE a.migration='0018_physical_items' AND a.converted_at IS NULL \
         ORDER BY c.id FOR UPDATE",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(database_error)?;
    for row in &rows {
        let id: i64 = row.get("id");
        let invalid = |_| format!("Не удалось прочитать старый инвентарь персонажа {id}");
        let stats: BTreeMap<String, f64> =
            serde_json::from_value(row.get("stats")).map_err(invalid)?;
        let inventory: Vec<mortown_protocol::legacy::Item> =
            serde_json::from_value(row.get("inventory")).map_err(invalid)?;
        let loot: BTreeMap<u8, u32> = serde_json::from_value(row.get("loot")).map_err(invalid)?;
        let mut physical =
            mortown_protocol::legacy::physical(stats.clone(), inventory.clone(), &loot);
        let mut ids = Vec::new();
        for (token, stack) in &physical.stacks {
            ids.push((*token, inventory::insert(&mut tx, Some(id), stack).await?));
        }
        mortown_protocol::legacy::rebind(&mut physical.stats, &physical.copies, &ids);
        sqlx::query(
            "UPDATE characters SET stats=$2, inventory='[]'::jsonb, loot='{}'::jsonb WHERE id=$1",
        )
        .bind(id)
        .bind(serde_json::to_value(&physical.stats).unwrap())
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        sqlx::query("UPDATE legacy_character_archive SET data=$2, converted_at=now() WHERE id=$1")
            .bind(row.get::<i64, _>("archive"))
            .bind(serde_json::json!({"stats": stats, "inventory": inventory, "loot": loot}))
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
    }
    release_legacy_property(&mut tx, world).await?;
    tx.commit().await.map_err(database_error)?;
    Ok(rows.len() as u64)
}
/// Mail attachments and unpaid sale coins go to their owner's bag and purse; each alliance's
/// old warehouse becomes a locked chest by the guild hall whose only key lies in the leader's bag.
/// `released_to` records where every record went.
async fn release_legacy_property(
    conn: &mut PgConnection,
    world: &mortown_world::World,
) -> Result<(), String> {
    let rows = sqlx::query(
        "SELECT p.id, p.character_id, p.alliance_id, p.kind, p.quantity, p.coins, a.leader_id \
         FROM legacy_property p LEFT JOIN alliances a ON a.id = p.alliance_id \
         WHERE p.released_at IS NULL ORDER BY p.id FOR UPDATE OF p",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(database_error)?;
    let mut chests: BTreeMap<i64, (i64, Vec<Stack>)> = BTreeMap::new();
    let mut taken = crate::objects::placed(conn).await?;
    for row in &rows {
        let record: i64 = row.get("id");
        let stacks: Vec<Stack> = match (
            row.get::<Option<i16>, _>("kind"),
            row.get::<Option<i32>, _>("quantity"),
        ) {
            (Some(kind), Some(quantity)) => {
                let kind = kind as u8;
                let quantity = quantity as u32;
                if items::equippable(kind) {
                    (0..quantity)
                        .map(|_| Stack::new(kind, 1, Location::Purse))
                        .collect()
                } else {
                    vec![Stack::new(kind, quantity, Location::Purse)]
                }
            }
            _ => {
                let value = row.get::<Option<i64>, _>("coins").unwrap_or(0);
                let value = u32::try_from(value).map_err(|_| "Неверная сумма старого платежа")?;
                vec![Stack::new(items::COPPER, value, Location::Purse)]
            }
        };
        let released_to = if let Some(owner) = row.get::<Option<i64>, _>("character_id") {
            for s in stacks {
                give(conn, owner, s).await?;
            }
            serde_json::json!({"to": "character", "character": owner})
        } else {
            let alliance: i64 = row.get("alliance_id");
            let leader: i64 = row.get("leader_id");
            let hall = mortown_world::HALL_SPECS
                .iter()
                .find(|h| h.name == "Дом гильдий")
                .map_or(mortown_world::SPAWN, |h| h.center);
            let mut placed = None;
            for s in stacks {
                loop {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        chests.entry(alliance)
                    {
                        let at = crate::objects::spot(world, hall, 30., &taken);
                        taken.push(at);
                        let (container, key) = crate::objects::create_locked(conn, at).await?;
                        give(conn, leader, key).await?;
                        entry.insert((container, Vec::new()));
                    }
                    let (container, contents) = chests.get_mut(&alliance).unwrap();
                    let holder = Holder::Container {
                        id: *container,
                        cells: mortown_protocol::objects::CHEST_CELLS,
                    };
                    if transfer::put(contents, holder, s).is_ok() {
                        placed = Some(*container);
                        break;
                    }
                    // This chest is full: store it and place another one with its own key.
                    let (_, contents) = chests.remove(&alliance).unwrap();
                    store_contents(conn, &contents).await?;
                }
            }
            serde_json::json!({"to": "container", "container": placed, "key_holder": leader, "alliance": alliance})
        };
        sqlx::query("UPDATE legacy_property SET released_at=now(), released_to=$2 WHERE id=$1")
            .bind(record)
            .bind(released_to)
            .execute(&mut *conn)
            .await
            .map_err(database_error)?;
    }
    for (_, contents) in chests.values() {
        store_contents(conn, contents).await?;
    }
    Ok(())
}
async fn store_contents(conn: &mut PgConnection, contents: &[Stack]) -> Result<(), String> {
    inventory::sync(
        conn,
        &[Change {
            owner: None,
            before: &[],
            after: contents,
        }],
    )
    .await
}
/// Puts one stack into a character's bag or purse, regardless of the carry limit: used only for
/// property the character already owned before it became physical.
async fn give(conn: &mut PgConnection, owner: i64, s: Stack) -> Result<(), String> {
    let before = inventory::carried(conn, owner).await?;
    let mut after = before.clone();
    transfer::put(&mut after, Holder::Character, s)?;
    inventory::sync(
        conn,
        &[Change {
            owner: Some(owner),
            before: &before,
            after: &after,
        }],
    )
    .await
}
/// The sheet the client sees: stored base stats with level growth and worn-item bonuses.
pub(crate) async fn character(
    conn: &mut PgConnection,
    row: &sqlx::postgres::PgRow,
) -> Result<Character, String> {
    let id: i64 = row.get("id");
    let items = inventory::listed(conn, id).await?;
    let mut stats: BTreeMap<String, f64> =
        serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
    let level = stats.get("level").copied().unwrap_or(1.) as u32;
    mortown_protocol::progression::grow(&mut stats, level.max(1) - 1);
    for s in items
        .iter()
        .filter(|s| matches!(s.location, Location::Worn { .. }))
    {
        if let Some(def) = items::definition(s.kind) {
            *stats.entry(def.stat.into()).or_default() += items::effective_bonus(s.kind, &stats);
        }
    }
    Ok(Character {
        id,
        slot: row.get::<i16, _>("slot") as u8,
        nickname: row.get("nickname"),
        gender: row.get::<i16, _>("gender") as u8,
        stats,
        items,
    })
}
pub(crate) async fn sheet(conn: &mut PgConnection, id: i64) -> Result<Character, String> {
    let row = sqlx::query("SELECT * FROM characters WHERE id=$1")
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(database_error)?;
    character(conn, &row).await
}
pub async fn characters(conn: &mut PgConnection, account: i64) -> Result<Vec<Character>, String> {
    let rows = sqlx::query("SELECT * FROM characters WHERE account_id=$1 ORDER BY slot")
        .bind(account)
        .fetch_all(&mut *conn)
        .await
        .map_err(database_error)?;
    let mut out = Vec::new();
    for row in &rows {
        out.push(character(conn, row).await?);
    }
    Ok(out)
}
pub async fn load(
    conn: &mut PgConnection,
    account: i64,
    character_id: i64,
) -> Result<(Character, [f32; 2], f32), String> {
    let row = sqlx::query("SELECT * FROM characters WHERE account_id=$1 AND id=$2")
        .bind(account)
        .bind(character_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(database_error)?
        .ok_or("Персонаж не найден на вашем аккаунте")?;
    Ok((
        character(conn, &row).await?,
        [row.get("x"), row.get("z")],
        row.get("yaw"),
    ))
}
pub(crate) const STALE_ADMIN: &str =
    "Персонаж изменён администратором. Войдите заново для загрузки сохранённого состояния";
pub async fn save(conn: &mut PgConnection, actor: &Actor) -> Result<(), String> {
    let mut updates = BTreeMap::new();
    actor.progress.write_stats(&mut updates);
    actor.orders.write_stats(&mut updates);
    updates.insert("lives".into(), f64::from(actor.lives));
    updates.insert("bound".into(), f64::from(u8::from(actor.bound)));
    updates.insert("return_until".into(), actor.return_until as f64);
    updates.insert("hunger".into(), actor.hunger);
    updates.insert("thirst".into(), actor.thirst);
    updates.insert("current_hp".into(), actor.hp);
    updates.insert("current_mana".into(), actor.mana);
    updates.insert("current_stamina".into(), actor.stamina);
    let changed=sqlx::query("UPDATE characters SET x=$1,z=$2,yaw=$3,saved_at=now(),stats=stats || $5::jsonb WHERE id=$4 AND coalesce((stats->>'admin_revision')::double precision,0)=$6")
        .bind(actor.position[0]).bind(actor.position[2]).bind(actor.yaw).bind(actor.id)
        .bind(serde_json::to_value(updates).unwrap())
        .bind(f64::from(actor.progress.admin_revision))
        .execute(conn)
        .await
        .map_err(database_error)?;
    if changed.rows_affected() != 1 {
        return Err(STALE_ADMIN.into());
    }
    Ok(())
}
pub(crate) fn database_error(error: sqlx::Error) -> String {
    if error
        .as_database_error()
        .is_some_and(|e| e.is_unique_violation())
    {
        "Логин или имя персонажа уже заняты".into()
    } else {
        eprintln!("Database operation failed: {error}");
        "База данных недоступна. Попробуйте снова".into()
    }
}

pub enum ExchangeFailure {
    Rejected(String),
    Uncertain,
}
impl From<String> for ExchangeFailure {
    fn from(message: String) -> Self {
        Self::Rejected(message)
    }
}
impl From<&str> for ExchangeFailure {
    fn from(message: &str) -> Self {
        Self::Rejected(message.into())
    }
}
/// Hold the simulation lock while committing, then replace the in-memory actor.
pub async fn exchange(
    conn: &mut PgConnection,
    actor: Actor,
    action: crate::economy::Action,
) -> Result<(Character, Actor), ExchangeFailure> {
    use crate::economy::Action;
    inventory::transaction(conn, async |tx| {
        let mut actor = actor;
        let mut c = inventory::load(&mut *tx, &actor).await?;
        let before = c.items.clone();
        match action {
            Action::Craft { recipe, expected } => {
                let count = mortown_protocol::crafting::count(&c);
                if count != expected {
                    return Err("Ремесло уже обновлено: повторная команда отклонена".into());
                }
                let next = count.checked_add(1).ok_or("Достигнут предел ремесла")?;
                crate::economy::craft(&mut actor, &mut c, recipe)?;
                let xp = mortown_protocol::progression::craft_experience();
                let levels = actor.progress.add_experience(xp);
                actor.progress.write_stats(&mut c.stats);
                c.stats
                    .insert(mortown_protocol::crafting::COUNTER.into(), f64::from(next));
                if levels > 0 {
                    let mut derived = c.stats.clone();
                    mortown_protocol::progression::grow(&mut derived, actor.progress.level - 1);
                    actor.hp =
                        (actor.hp + (derived["hp"] - actor.max_hp).max(0.)).min(derived["hp"]);
                    actor.max_hp = derived["hp"];
                }
                actor.reward_status = format!("{} · +{xp} опыта", actor.reward_status);
            }
            Action::Arrange {
                source,
                target,
                expected,
            } => {
                mortown_protocol::layout::arrange(&mut c, source, target, expected)?;
                actor.reward_status = "Расположение сохранено".into();
            }
            // Hotbar shortcuts: 1–2 base skills, 100 + kind for a supply kind.
            Action::BindHotbar { slot, binding } => {
                let token = match binding {
                    0..=2 => u64::from(binding),
                    101..=255
                        if items::stackable(binding - 100)
                            && items::coin_value(binding - 100).is_none() =>
                    {
                        mortown_protocol::layout::kind_token(binding - 100)
                    }
                    _ => return Err("Недопустимый слот или действие".into()),
                };
                if slot >= mortown_protocol::layout::HOTBAR {
                    return Err("Недопустимый слот или действие".into());
                }
                c.stats.insert(format!("hotbar_{slot}"), token as f64);
                actor.reward_status = format!("Панель F{} сохранена", slot + 1);
            }
            Action::SpendAttribute { stat, expected } => {
                if actor.hp <= 0. {
                    return Err("Сначала возродитесь".into());
                }
                if actor.progress.stat_points < 1. {
                    return Err("Нет свободных очков характеристик".into());
                }
                let mut old = c.stats.clone();
                mortown_protocol::progression::grow(&mut old, actor.progress.level - 1);
                mortown_protocol::attributes::increase(&mut c.stats, &stat, expected)?;
                let mut derived = c.stats.clone();
                mortown_protocol::progression::grow(&mut derived, actor.progress.level - 1);
                actor.hp = (actor.hp + (derived["hp"] - actor.max_hp).max(0.)).min(derived["hp"]);
                actor.mana =
                    (actor.mana + (derived["mana"] - old["mana"]).max(0.)).min(derived["mana"]);
                actor.stamina = (actor.stamina + (derived["stamina"] - old["stamina"]).max(0.))
                    .min(derived["stamina"]);
                actor.max_hp = derived["hp"];
                actor.progress.stat_points -= 1.;
                actor.reward_status = format!(
                    "{} +1 · потрачено 1 очко",
                    mortown_protocol::attributes::ATTRIBUTES
                        .iter()
                        .find(|(key, _)| *key == stat)
                        .unwrap()
                        .1
                );
            }
            Action::SpendTalent { talent, expected } => {
                crate::economy::spend_talent(&mut actor, &mut c, &talent, expected)?
            }
            Action::Potion => crate::economy::potion(&mut actor, &mut c)?,
        }
        inventory::sync(
            &mut *tx,
            &[Change {
                owner: Some(actor.id),
                before: &before,
                after: &c.items,
            }],
        )
        .await?;
        let c = inventory::finish(&mut *tx, &mut actor, &c, "").await?;
        Ok((c, actor))
    })
    .await
}
/// Puts the first bag copy of `kind` on (or takes the worn one off into a free cell).
pub async fn set_equipment(
    conn: &mut PgConnection,
    actor: Actor,
    kind: u8,
    equipped: bool,
) -> Result<(Character, Actor), ExchangeFailure> {
    use mortown_protocol::layout::{Place, at, equipped as worn};
    let slot = items::slot(kind).ok_or("Этот предмет нельзя надеть")?;
    let weapon = slot == items::Slot::Weapon;
    let probe =
        inventory::transaction(conn, async |tx| Ok(inventory::load(tx, &actor).await?)).await?;
    let action = if equipped {
        let cell = probe.items.iter().find_map(|s| match s.location {
            Location::Bag { cell } if s.kind == kind => Some(cell),
            _ => None,
        });
        match cell {
            Some(cell) => crate::economy::Action::Arrange {
                source: Place::Bag(cell),
                target: Place::Equipment(weapon),
                expected: at(&probe, Place::Bag(cell)),
            },
            None if items::worn(&probe.items, slot).is_some_and(|s| s.kind == kind) => {
                return Ok((sheet(conn, actor.id).await?, actor));
            }
            None => return Err("У персонажа нет этого предмета".into()),
        }
    } else {
        let current = worn(&probe, weapon);
        if current == 0 || items::worn(&probe.items, slot).map(|s| s.kind) != Some(kind) {
            return Ok((sheet(conn, actor.id).await?, actor));
        }
        let free = mortown_protocol::layout::free_cell(&probe.items).ok_or("Рюкзак переполнен")?;
        crate::economy::Action::Arrange {
            source: Place::Equipment(weapon),
            target: Place::Bag(free),
            expected: current,
        }
    };
    exchange(conn, actor, action).await
}
