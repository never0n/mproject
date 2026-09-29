use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use mortown_protocol::{
    Actor, Character, starting_stats, validate_character, validate_credentials,
};
use rand_core::OsRng;
use sqlx::{Connection, PgConnection, PgPool, Row};

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
    pub race: u8,
    pub class: u8,
    pub gender: u8,
}
pub async fn create_character(
    conn: &mut PgConnection,
    account: i64,
    c: CharacterCreation,
) -> Result<i64, String> {
    validate_character(&c.nickname, c.race, c.class, c.gender)?;
    let mut tx = conn.begin().await.map_err(database_error)?;
    // Serialize allocation even across independent service connections.
    sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR UPDATE")
        .bind(account)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let slot: Option<i32>=sqlx::query_scalar("SELECT n FROM generate_series(1,12) n WHERE NOT EXISTS (SELECT 1 FROM characters WHERE account_id=$1 AND slot=n) ORDER BY n LIMIT 1").bind(account).fetch_optional(&mut *tx).await.map_err(database_error)?;
    let slot = slot.ok_or("На аккаунте уже 12 персонажей")?;
    let id=sqlx::query_scalar("INSERT INTO characters(account_id,slot,nickname,race,class,gender,stats,x,z,inventory) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id")
        .bind(account).bind(slot as i16).bind(c.nickname).bind(i16::from(c.race)).bind(i16::from(c.class)).bind(i16::from(c.gender))
        .bind(serde_json::to_value(starting_stats(c.race,c.class).unwrap()).unwrap()).bind(mortown_world::SPAWN[0]).bind(mortown_world::SPAWN[1]).bind(serde_json::to_value(mortown_protocol::items::starter(c.class)).unwrap())
        .fetch_one(&mut *tx).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(id)
}
pub(crate) fn character(row: &sqlx::postgres::PgRow) -> Result<Character, String> {
    let mut inventory: Vec<mortown_protocol::items::Item> =
        serde_json::from_value(row.get("inventory")).map_err(|_| "Ошибка инвентаря")?;
    let loot: std::collections::BTreeMap<u8, u32> =
        serde_json::from_value(row.get("loot")).map_err(|_| "Ошибка добычи")?;
    mortown_protocol::items::include_loot(&mut inventory, &loot);
    let mut stats: std::collections::BTreeMap<String, f64> =
        serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
    let level = stats.get("level").copied().unwrap_or(1.) as u32;
    mortown_protocol::progression::grow(&mut stats, level.clamp(1, 100) - 1);
    for item in inventory.iter().filter(|i| i.equipped) {
        if let Some(def) = mortown_protocol::items::definition(item.kind) {
            *stats.entry(def.stat.into()).or_default() += def.bonus;
        }
    }
    Ok(Character {
        id: row.get("id"),
        slot: row.get::<i16, _>("slot") as u8,
        nickname: row.get("nickname"),
        race: row.get::<i16, _>("race") as u8,
        class: row.get::<i16, _>("class") as u8,
        gender: row.get::<i16, _>("gender") as u8,
        stats,
        inventory,
        loot,
    })
}
pub async fn characters(conn: &mut PgConnection, account: i64) -> Result<Vec<Character>, String> {
    sqlx::query("SELECT * FROM characters WHERE account_id=$1 ORDER BY slot")
        .bind(account)
        .fetch_all(conn)
        .await
        .map_err(database_error)?
        .iter()
        .map(character)
        .collect()
}
pub async fn load(
    conn: &mut PgConnection,
    account: i64,
    character_id: i64,
) -> Result<(Character, [f32; 2], f32), String> {
    let row = sqlx::query("SELECT * FROM characters WHERE account_id=$1 AND id=$2")
        .bind(account)
        .bind(character_id)
        .fetch_optional(conn)
        .await
        .map_err(database_error)?
        .ok_or("Персонаж не найден на вашем аккаунте")?;
    Ok((
        character(&row)?,
        [row.get("x"), row.get("z")],
        row.get("yaw"),
    ))
}
pub(crate) const STALE_ADMIN: &str =
    "Персонаж изменён администратором. Войдите заново для загрузки сохранённого состояния";
pub(crate) fn check_admin_revision(c: &Character, actor: &Actor) -> Result<(), String> {
    if c.stats.get("admin_revision").copied().unwrap_or(0.)
        != f64::from(actor.progress.admin_revision)
    {
        Err(STALE_ADMIN.into())
    } else {
        Ok(())
    }
}
pub async fn save(conn: &mut PgConnection, actor: &Actor) -> Result<(), String> {
    let mut updates = std::collections::BTreeMap::new();
    actor.progress.write_stats(&mut updates);
    updates.insert("current_hp".into(), actor.hp);
    updates.insert("current_mana".into(), actor.mana);
    updates.insert("current_stamina".into(), actor.stamina);
    let changed=sqlx::query("UPDATE characters SET x=$1,z=$2,yaw=$3,saved_at=now(),stats=stats || $5::jsonb,loot=$6 WHERE id=$4 AND coalesce((stats->>'admin_revision')::double precision,0)=$7")
        .bind(actor.position[0]).bind(actor.position[2]).bind(actor.yaw).bind(actor.id)
        .bind(serde_json::to_value(updates).unwrap())
        .bind(serde_json::to_value(&actor.loot).unwrap())
        .bind(f64::from(actor.progress.admin_revision))
        .execute(conn)
        .await
        .map_err(database_error)?;
    if changed.rows_affected() != 1 {
        return Err(STALE_ADMIN.into());
    }
    Ok(())
}
fn database_error(error: sqlx::Error) -> String {
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

pub async fn set_equipment(
    conn: &mut PgConnection,
    id: i64,
    item: u8,
    equipped: bool,
) -> Result<Character, String> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    let row = sqlx::query("SELECT * FROM characters WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let mut c = character(&row)?;
    mortown_protocol::items::equip(&mut c.inventory, c.class, item, equipped)?;
    let row = sqlx::query("UPDATE characters SET inventory=$2 WHERE id=$1 RETURNING *")
        .bind(id)
        .bind(serde_json::to_value(c.inventory).unwrap())
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let result = character(&row)?;
    tx.commit().await.map_err(database_error)?;
    Ok(result)
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
    world: &mortown_world::World,
) -> Result<(Character, Actor), ExchangeFailure> {
    exchange_at(conn, actor, action, world, None).await
}
pub async fn exchange_at(
    conn: &mut PgConnection,
    mut actor: Actor,
    action: crate::economy::Action,
    world: &mortown_world::World,
    resident: Option<mortown_world::npcs::Npc>,
) -> Result<(Character, Actor), ExchangeFailure> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    let row = sqlx::query("SELECT * FROM characters WHERE id=$1 FOR UPDATE")
        .bind(actor.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let mut c = character(&row)?;
    check_admin_revision(&c, &actor)?;
    match action {
        crate::economy::Action::Specialize {
            profession,
            expected,
        } => {
            if actor.hp <= 0. {
                return Err("Сначала возродитесь".into());
            }
            let mut base =
                serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
            let cost = mortown_protocol::professions::choose(
                &mut base,
                profession,
                expected,
                actor.progress.coins,
            )?;
            actor.progress.coins -= f64::from(cost);
            actor.reward_status = format!(
                "Специализация: {}{}",
                profession.name(),
                if cost == 0 {
                    " · бесплатно".into()
                } else {
                    format!(" · −{cost} монет")
                }
            );
            sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(serde_json::to_value(&base).unwrap())
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
        crate::economy::Action::Craft {
            recipe,
            expected,
            specialization_revision,
        } => {
            if mortown_protocol::professions::revision(&c.stats) != specialization_revision {
                return Err("Специализация изменилась. Посмотрите новый расход материалов и повторите изготовление".into());
            }
            let count = mortown_protocol::crafting::count(&c);
            if count != expected {
                return Err("Ремесло уже обновлено: повторная команда отклонена".into());
            }
            let next = count.checked_add(1).ok_or("Достигнут предел ремесла")?;
            let mut bag = c.clone();
            bag.stats =
                serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
            actor.progress.write_stats(&mut bag.stats);
            bag.loot = actor.loot.clone();
            mortown_protocol::items::include_loot(&mut bag.inventory, &bag.loot);
            crate::economy::craft(&mut actor, recipe, &bag.stats)?;
            let recipe =
                mortown_protocol::crafting::definition(recipe).ok_or("Неизвестный рецепт")?;
            // Consume exact bag stacks so depleted material shortcuts cannot reappear later.
            for (kind, quantity) in recipe.requirements(&bag.stats) {
                mortown_protocol::transfer::take(
                    &mut bag,
                    &mortown_protocol::mail::AttachmentRequest {
                        token: mortown_protocol::layout::token(kind, 0),
                        quantity,
                        price: 0,
                    },
                )?;
            }
            mortown_protocol::transfer::give(&mut bag, recipe.result, 1)?;
            let gain = recipe.profession.gain(&mut bag.stats);
            if gain.experience > 0 {
                actor.reward_status.push_str(&format!(
                    " · {}: +{} опыта",
                    recipe.profession.name(),
                    gain.experience
                ));
            }
            if gain.after > gain.before {
                actor
                    .reward_status
                    .push_str(&format!(" · уровень ремесла {}!", gain.after));
            }
            bag.stats
                .insert(mortown_protocol::crafting::COUNTER.into(), f64::from(next));
            sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(serde_json::to_value(&bag.stats).unwrap())
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
            c.inventory = bag.inventory;
        }
        crate::economy::Action::Arrange {
            source,
            target,
            expected,
        } => {
            let mut base = c.clone();
            base.stats =
                serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
            actor.progress.write_stats(&mut base.stats);
            base.loot = actor.loot.clone();
            mortown_protocol::items::include_loot(&mut base.inventory, &base.loot);
            mortown_protocol::layout::arrange(&mut base, source, target, expected)?;
            sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(serde_json::to_value(&base.stats).unwrap())
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
            c.inventory = base.inventory;
            actor.reward_status = "Расположение сохранено".into();
        }
        crate::economy::Action::BindHotbar { slot, binding } => {
            if slot >= 12 || !matches!(binding, 0..=2 | 101..=108) {
                return Err("Недопустимый слот или действие".into());
            }
            sqlx::query("UPDATE characters SET stats=jsonb_set(stats, ARRAY[$2], to_jsonb($3::int)) WHERE id=$1")
                .bind(actor.id).bind(format!("hotbar_{slot}")).bind(i32::from(binding))
                .execute(&mut *tx).await.map_err(database_error)?;
            actor.reward_status = format!("Панель F{} сохранена", slot + 1);
        }
        crate::economy::Action::SpendAttribute { stat, expected } => {
            if actor.hp <= 0. {
                return Err("Сначала возродитесь".into());
            }
            if actor.progress.stat_points < 1. {
                return Err("Нет свободных очков характеристик".into());
            }
            let mut base: std::collections::BTreeMap<String, f64> =
                serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
            let old_level = c.stats["level"] as u32;
            mortown_protocol::progression::grow(
                &mut c.stats,
                actor.progress.level.saturating_sub(old_level),
            );
            actor.progress.write_stats(&mut base);
            mortown_protocol::attributes::increase(&mut base, c.race, c.class, &stat, expected)?;
            let mut derived = base.clone();
            mortown_protocol::progression::grow(&mut derived, actor.progress.level - 1);
            actor.hp = (actor.hp + (derived["hp"] - actor.max_hp).max(0.)).min(derived["hp"]);
            actor.mana =
                (actor.mana + (derived["mana"] - c.stats["mana"]).max(0.)).min(derived["mana"]);
            actor.stamina = (actor.stamina + (derived["stamina"] - c.stats["stamina"]).max(0.))
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
            sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(serde_json::to_value(base).unwrap())
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
        crate::economy::Action::LearnSecondary => crate::economy::learn_secondary(&mut actor)?,
        crate::economy::Action::LearnAbility { skill } => {
            crate::economy::learn_ability(&mut actor, c.class, c.race, skill)?
        }
        crate::economy::Action::LearnSkill { rank } => crate::economy::learn(&mut actor, rank)?,
        crate::economy::Action::Quest { quest, complete } => {
            // Delivery also changes bag cells and shortcuts. Persist these with the reward.
            c.stats =
                serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
            crate::economy::quest(&mut actor, &mut c, quest, complete, world)?;
            sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(serde_json::to_value(&c.stats).unwrap())
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
        crate::economy::Action::Potion => {
            crate::economy::apply(&mut actor, &c.inventory, None, world)?
        }
        crate::economy::Action::Trade {
            npc,
            item,
            buy,
            sale,
        } => {
            let mut bag = c.clone();
            bag.stats =
                serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
            actor.progress.write_stats(&mut bag.stats);
            bag.loot = actor.loot.clone();
            mortown_protocol::items::include_loot(&mut bag.inventory, &bag.loot);
            if buy {
                if sale.is_some() {
                    return Err("Покупка не должна содержать вещь из рюкзака".into());
                }
                // Materialize automatic cells before the new kind joins the sorted inventory.
                let cells = mortown_protocol::layout::bag(&bag);
                bag.stats.retain(|key, _| !key.starts_with("bag_"));
                for (slot, token) in cells {
                    bag.stats.insert(format!("bag_{slot}"), token as f64);
                }
                mortown_protocol::transfer::give(&mut bag, item, 1)?;
            } else {
                let sale = sale.ok_or("Сначала выберите вещь в рюкзаке")?;
                sale.validate(&bag, item)?;
                mortown_protocol::transfer::take(
                    &mut bag,
                    &mortown_protocol::mail::AttachmentRequest {
                        token: sale.token,
                        quantity: 1,
                        price: 0,
                    },
                )?;
            }
            // The existing rules determine price and proximity; the bag copy changes atomically.
            crate::economy::apply_at(
                &mut actor,
                &c.inventory,
                Some((npc, item, buy)),
                world,
                resident,
            )?;
            actor.loot = bag.loot.clone();
            c = bag;
            sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
                .bind(actor.id)
                .bind(serde_json::to_value(&c.stats).unwrap())
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
    }
    mortown_protocol::items::include_loot(&mut c.inventory, &actor.loot);
    save(&mut tx, &actor).await?;
    let row = sqlx::query("UPDATE characters SET inventory=$2 WHERE id=$1 RETURNING *")
        .bind(actor.id)
        .bind(serde_json::to_value(&c.inventory).unwrap())
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let c = character(&row)?;
    // A lost COMMIT acknowledgement cannot safely be followed by saving the old actor.
    tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
    actor.equipment = mortown_protocol::items::EquipmentAppearance::from_inventory(&c.inventory);
    Ok((c, actor))
}
