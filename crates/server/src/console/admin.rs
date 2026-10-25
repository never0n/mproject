pub(super) mod control;
use super::*;
use mortown_protocol::{chat::commands::Invocation, items, layout, progression::Progress, skills};
use std::net::IpAddr;

async fn audit(
    conn: &mut PgConnection,
    owner: i64,
    action: &str,
    target: &str,
    detail: &str,
) -> Result<(), String> {
    sqlx::query("INSERT INTO admin_audit(actor_id,action,target,detail) VALUES($1,$2,$3,$4)")
        .bind(owner)
        .bind(action)
        .bind(target)
        .bind(detail)
        .execute(conn)
        .await
        .map_err(db)?;
    Ok(())
}
async fn require(conn: &mut PgConnection, owner: i64, minimum: i16) -> Result<i16, String> {
    let role:i16=sqlx::query_scalar("SELECT a.role FROM accounts a JOIN characters c ON c.account_id=a.id WHERE c.id=$1 FOR SHARE OF a").bind(owner).fetch_one(conn).await.map_err(db)?;
    if role < minimum {
        Err("Недостаточно прав".into())
    } else {
        Ok(role)
    }
}
fn number(raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or("Укажите положительное целое число".into())
}
fn id(raw: &str) -> Result<i64, String> {
    i64::try_from(number(raw)?).map_err(|_| "Неверный ID".into())
}
fn reason(words: &[&str]) -> Result<String, String> {
    let text = words.join(" ");
    if text.is_empty() || text.chars().count() > 256 {
        return Err("Причина: 1–256 символов".into());
    }
    Ok(text)
}
pub async fn execute(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    state: &mut crate::chat::Session,
    call: &Invocation<'_>,
    _staff: i16,
) -> Result<Reply, String> {
    let name = call.spec.name;
    let args = &call.args;
    if matches!(
        name,
        "//clearbag"
            | "//admin"
            | "//spawn"
            | "//kill"
            | "//del"
            | "//revive"
            | "//goto"
            | "//bring"
            | "//immortal"
            | "//invisible"
            | "//clicktp"
            | "//role"
            | "//announce"
            | "//resetworld"
    ) {
        return control::execute(server, conn, owner, state, call).await;
    }

    if matches!(
        name,
        "//give" | "//takeaway" | "//takeawayall" | "//learns" | "//unlearns" | "//bronze"
    ) {
        return inventory(server, conn, owner, call).await;
    }
    if name == "//erase" {
        let target_id = id(args[0])?;
        let (_, nickname, _, target_role) = target(conn, args[0]).await?;
        if target_role >= role(conn, owner).await? {
            return Err("Нельзя удалить себя или персонажа с равными/старшими правами".into());
        }
        return Ok(confirmation(
            state,
            "Удаление персонажа",
            format!(
                "Безвозвратно удалить {nickname} (ID {target_id}) со всеми вещами и перепиской?\nАккаунт должен быть отключён.\nПодтверждение действует 30 секунд; восстановления пока нет."
            ),
            PendingAction::Delete { id: target_id },
        ));
    }
    if name == "//banip"
        && args[0].parse::<IpAddr>().is_err()
        && server.simulation.lock().await.actor(id(args[0])?).is_none()
    {
        return Err("Для блокировки по ID персонаж должен быть в мире; иначе укажите IP".into());
    }
    let mut tx = conn.begin().await.map_err(db)?;
    let staff = require(&mut tx, owner, call.spec.role).await?;
    if matches!(name, "//banip" | "//unbanip") {
        let ip = if let Ok(ip) = args[0].parse::<IpAddr>() {
            ip.to_canonical().to_string()
        } else if name == "//banip" {
            let (_, _, account, target_role) = target(&mut tx, args[0]).await?;
            if target_role >= staff {
                return Err("Нельзя блокировать равную или старшую роль".into());
            }
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT host(last_ip) FROM accounts WHERE id=$1",
            )
            .bind(account)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?
            .ok_or("IP этой сессии неизвестен")?
        } else {
            return Err("Укажите корректный IP".into());
        };
        if name == "//banip" {
            sqlx::query(
                "INSERT INTO blocked_ips(ip) VALUES($1::text::inet) ON CONFLICT DO NOTHING",
            )
            .bind(&ip)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        } else {
            sqlx::query("DELETE FROM blocked_ips WHERE ip=$1::text::inet")
                .bind(&ip)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        audit(&mut tx, owner, name, &ip, "").await?;
        tx.commit().await.map_err(db)?;
        server.admin_events.send_replace(());
        return Ok(notice("Правило доступа по IP обновлено"));
    }
    let (target_id, nickname, account, target_role) = target(&mut tx, args[0]).await?;
    if target_role >= staff {
        return Err("Нельзя применять санкции к себе или к равной/старшей роли".into());
    }
    match name {
        "//mute" | "//muteg" => {
            let seconds = args
                .get(1)
                .map(|s| number(s))
                .transpose()?
                .map(|s| i64::try_from(s).unwrap_or(i64::MAX));
            if seconds.is_some_and(|s| s > 315_360_000) {
                return Err("Максимальный конечный мут — 10 лет".into());
            }
            let table = if name == "//mute" {
                "chat_mutes"
            } else {
                "global_mutes"
            };
            sqlx::query(&format!("INSERT INTO {table}(character_id,until_at) VALUES($1,CASE WHEN $2::bigint IS NULL THEN 'infinity'::timestamptz ELSE now()+$2*interval '1 second' END) ON CONFLICT(character_id) DO UPDATE SET until_at=CASE WHEN $2::bigint IS NULL THEN 'infinity'::timestamptz ELSE GREATEST(now(),{table}.until_at)+$2*interval '1 second' END")).bind(target_id).bind(seconds).execute(&mut *tx).await.map_err(db)?;
        }
        "//unmute" | "//unmuteg" => {
            let table = if name == "//unmute" {
                "chat_mutes"
            } else {
                "global_mutes"
            };
            sqlx::query(&format!("DELETE FROM {table} WHERE character_id=$1"))
                .bind(target_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        "//ban" => {
            let numeric = args[1]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '+');
            let (days, why) = if numeric {
                let d = number(args[1])?;
                if d > 36500 {
                    return Err("Срок бана слишком велик".into());
                }
                (Some(d as i64), reason(&args[2..])?)
            } else {
                (None, reason(&args[1..])?)
            };
            sqlx::query("UPDATE accounts SET banned_until=CASE WHEN $2::bigint IS NULL THEN 'infinity'::timestamptz ELSE now()+$2*interval '1 day' END,ban_reason=$3,session_epoch=session_epoch+1 WHERE id=$1").bind(account).bind(days).bind(why).execute(&mut *tx).await.map_err(db)?;
        }
        "//unban" => {
            let why = reason(&args[1..])?;
            sqlx::query("UPDATE accounts SET banned_until=NULL,ban_reason=$2 WHERE id=$1")
                .bind(account)
                .bind(why)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        "//kick" => {
            sqlx::query("UPDATE accounts SET session_epoch=session_epoch+1 WHERE id=$1")
                .bind(account)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        _ => return Err("Неизвестная команда".into()),
    }
    audit(
        &mut tx,
        owner,
        name,
        &target_id.to_string(),
        &args[1..].join(" "),
    )
    .await?;
    tx.commit().await.map_err(db)?;
    server.admin_events.send_replace(());
    Ok(notice(format!(
        "{name}: {nickname} (ID {target_id}) — выполнено"
    )))
}

async fn lease(conn: &mut PgConnection, account: i64) -> Result<(), String> {
    let free: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(account)
        .fetch_one(conn)
        .await
        .map_err(db)?;
    if free {
        Ok(())
    } else {
        Err("Аккаунт занят другой сессией. Повторите после выхода".into())
    }
}
pub(super) async fn inventory(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    call: &Invocation<'_>,
) -> Result<Reply, String> {
    let args = &call.args;
    let target_id = id(args[0])?;
    let name = call.spec.name;
    // Same lock order as normal equipment/exchange operations: simulation, then DB.
    let mut sim = server.simulation.lock().await;
    let current = sim.actor(target_id);
    let mut tx = conn.begin().await.map_err(db)?;
    let staff = require(&mut tx, owner, call.spec.role).await?;
    let (_, nickname, account, target_role) = target(&mut tx, args[0]).await?;
    if target_id != owner && target_role >= staff {
        return Err("Нельзя менять равную или старшую роль".into());
    }
    if current.is_none() {
        lease(&mut tx, account).await?;
    }
    if let Some(actor) = &current {
        crate::store::save(&mut tx, actor).await?;
    }
    let row = sqlx::query("SELECT * FROM characters WHERE id=$1 FOR UPDATE")
        .bind(target_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    let mut c = mortown_protocol::Character {
        id: target_id,
        slot: row.get::<i16, _>("slot") as u8,
        nickname: row.get("nickname"),
        gender: row.get::<i16, _>("gender") as u8,
        stats: serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?,
        items: crate::inventory::carried(&mut tx, target_id).await?,
    };
    let before = c.items.clone();
    let kind = if args.len() > 1 {
        Some(u8::try_from(number(args[1])?).map_err(|_| "ID вне каталога")?)
    } else {
        None
    };
    if matches!(name, "//learns" | "//unlearns") {
        let skill = kind.unwrap();
        let learned = name == "//learns";
        if !(1..=skills::LAST_SKILL).contains(&skill) {
            return Err(format!(
                "Навык отсутствует в каталоге 1–{}",
                skills::LAST_SKILL
            ));
        }
        match skill {
            1 => {
                c.stats
                    .insert("primary_disabled".into(), if learned { 0. } else { 1. });
            }
            2 => {
                c.stats
                    .insert("secondary_learned".into(), if learned { 1. } else { 0. });
            }
            _ => {
                let bits = c.stats.get("extra_skills").copied().unwrap_or(0.) as u8;
                let mask = 1 << (skill - 3);
                c.stats.insert(
                    "extra_skills".into(),
                    f64::from(if learned { bits | mask } else { bits & !mask }),
                );
            }
        }
        if !learned {
            for slot in 0..12 {
                let k = format!("hotbar_{slot}");
                if c.stats.get(&k) == Some(&f64::from(skill)) {
                    c.stats.insert(k, 0.);
                }
            }
        }
    } else {
        let (kind, amount, giving) = if name == "//bronze" {
            (items::BRONZE, Some(18u64), true)
        } else {
            let amount = args.get(2).map(|n| number(n)).transpose()?;
            (kind.unwrap(), amount, name == "//give")
        };
        if name == "//takeawayall" {
            c.items.clear();
        } else {
            let def = items::definition(kind).ok_or("Предмет не найден в текущем каталоге")?;
            change_items(&mut c.items, kind, giving, amount)
                .map_err(|e| format!("{}: {e}", def.name))?;
        }
        let invalid: Vec<_> = c
            .stats
            .iter()
            .filter(|(k, v)| k.starts_with("hotbar_") && !layout::valid(&c, **v as u64))
            .map(|(k, _)| k.clone())
            .collect();
        for key in invalid {
            c.stats.insert(key, 0.);
        }
    }
    let revision = Progress::from_stats(&c.stats)
        .admin_revision
        .checked_add(1)
        .ok_or("Исчерпаны версии административных изменений")?;
    c.stats.insert("admin_revision".into(), f64::from(revision));
    crate::inventory::sync(
        &mut tx,
        &[crate::inventory::Change {
            owner: Some(target_id),
            before: &before,
            after: &c.items,
        }],
    )
    .await?;
    sqlx::query("UPDATE characters SET stats=$2 WHERE id=$1")
        .bind(target_id)
        .bind(serde_json::to_value(&c.stats).unwrap())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    audit(
        &mut tx,
        owner,
        name,
        &target_id.to_string(),
        &args[1..].join(" "),
    )
    .await?;
    let updated = crate::store::sheet(&mut tx, target_id).await?;
    tx.commit().await.map_err(db)?;
    if let Some(mut actor) = current {
        sim.exchanges
            .cancel(target_id, "Обмен отменён: изменения администратором");
        actor.progress = Progress::from_stats(&c.stats);
        actor.actions.clear();
        sim.replace_actor(actor);
        sim.refresh_character(&updated);
    }
    server.admin_events.send_replace(());
    Ok(notice(format!(
        "{name}: {nickname} (ID {target_id}) — выполнено"
    )))
}
/// Admin creation or removal of `amount` units of `kind` on a character. Creation ignores the
/// carry limit; coins of any denomination join the purse by value.
fn change_items(
    stacks: &mut Vec<items::Stack>,
    kind: u8,
    give: bool,
    amount: Option<u64>,
) -> Result<(), String> {
    use mortown_protocol::transfer::{self, Holder};
    if let Some(value) = items::coin_value(kind) {
        let have = transfer::coins(stacks, Holder::Character);
        let held = stacks
            .iter()
            .filter(|s| s.kind == kind && s.location == items::Location::Purse)
            .map(|s| u64::from(s.quantity))
            .sum::<u64>();
        let units = if give {
            amount.ok_or("Количество обязательно")?
        } else {
            amount.unwrap_or(held)
        };
        let delta = units
            .checked_mul(value)
            .ok_or("Слишком большое количество")?;
        let total = if give {
            have.checked_add(delta)
                .filter(|t| *t <= 1_000_000_000 * 10_000)
                .ok_or("Превышен предел монет")?
        } else {
            have.checked_sub(delta).ok_or("Недостаточно монет")?
        };
        return transfer::set_coins(stacks, Holder::Character, total).map_err(Into::into);
    }
    if give {
        let amount = amount.ok_or("Количество обязательно")?;
        if amount == 0 || amount > 10_000 {
            return Err("Количество: от 1 до 10000".into());
        }
        let mut left = amount as u32;
        while left > 0 {
            let part = if items::stackable(kind) {
                left.min(items::MAX_STACK)
            } else {
                1
            };
            transfer::put(
                stacks,
                Holder::Character,
                items::Stack::new(kind, part, items::Location::Purse),
            )?;
            left -= part;
        }
        return Ok(());
    }
    let held: u64 = stacks
        .iter()
        .filter(|s| s.kind == kind && s.carried())
        .map(|s| u64::from(s.quantity))
        .sum();
    let mut left = amount.unwrap_or(held);
    if left > held {
        return Err("Недостаточно предметов".into());
    }
    for s in stacks.iter_mut().filter(|s| s.kind == kind && s.carried()) {
        let used = u64::from(s.quantity).min(left);
        s.quantity -= used as u32;
        left -= used;
    }
    stacks.retain(|s| s.quantity > 0);
    Ok(())
}
pub async fn delete(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    target_id: i64,
) -> Result<Reply, String> {
    let mut sim = server.simulation.lock().await;
    if sim.actor(target_id).is_some() {
        return Err("Сначала отключите аккаунт удаляемого персонажа".into());
    }
    let mut tx = conn.begin().await.map_err(db)?;
    let staff = require(&mut tx, owner, 4).await?;
    let (_, nickname, account, target_role) = target(&mut tx, &target_id.to_string()).await?;
    if target_role >= staff {
        return Err("Нельзя удалить равную или старшую роль".into());
    }
    lease(&mut tx, account).await?;
    sqlx::query("SELECT id FROM characters WHERE id=$1 FOR UPDATE")
        .bind(target_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::alliance::LOCK)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let leader: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM alliances WHERE leader_id=$1)")
            .bind(target_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
    if leader {
        return Err("Сначала передайте лидерство альянса".into());
    }
    if crate::legacy::character_owns(&mut tx, target_id)
        .await
        .map_err(db)?
    {
        return Err(crate::legacy::KEPT.into());
    }
    sqlx::query("DELETE FROM chat_reports WHERE reporter_id=$1 OR message_id IN (SELECT id FROM chat_messages WHERE sender_id=$1 OR recipient_id=$1)").bind(target_id).execute(&mut *tx).await.map_err(db)?;
    sqlx::query("DELETE FROM chat_messages WHERE sender_id=$1 OR recipient_id=$1")
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM mail_payments WHERE owner_id=$1 OR mail_id IN (SELECT id FROM character_mail WHERE sender_id=$1 OR recipient_id=$1)").bind(target_id).execute(&mut *tx).await.map_err(db)?;
    sqlx::query("DELETE FROM character_mail WHERE sender_id=$1 OR recipient_id=$1")
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM alliance_invitations WHERE inviter_id=$1 OR recipient_id=$1")
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM alliance_members WHERE character_id=$1")
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM characters WHERE id=$1")
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    audit(&mut tx, owner, "//erase", &target_id.to_string(), &nickname).await?;
    tx.commit().await.map_err(db)?;
    sim.parties.leave(target_id);
    server.admin_events.send_replace(());
    server.alliance_events.send_replace(());
    Ok(notice(format!(
        "Персонаж {nickname} (ID {target_id}) удалён"
    )))
}
