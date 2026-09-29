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
        "//give" | "//takeaway" | "//takeawayall" | "//learns" | "//unlearns"
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
    // Same lock order as normal equipment/trade/mail operations: simulation, then DB.
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
    let mut c = crate::store::character(&row)?;
    c.stats = serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
    let kind = if args.len() > 1 {
        Some(u8::try_from(number(args[1])?).map_err(|_| "ID вне каталога")?)
    } else {
        None
    };
    if matches!(name, "//learns" | "//unlearns") {
        let skill = kind.unwrap();
        let learned = name == "//learns";
        if skills::definition(c.class, c.race, skill).is_none() {
            return Err("Навык недоступен для класса/расы или отсутствует в каталоге 1–7".into());
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
        let kinds: Vec<u8> = if name == "//takeawayall" {
            (1..=u8::MAX)
                .filter(|k| items::definition(*k).is_some())
                .collect()
        } else {
            vec![kind.unwrap()]
        };
        let amount = args.get(2).map(|n| number(n)).transpose()?;
        for kind in kinds {
            let def = items::definition(kind).ok_or("Предмет не найден в текущем каталоге")?;
            let old = layout::quantity(&c, kind);
            let next = if name == "//give" {
                let amount = amount.ok_or("Количество обязательно")?;
                let count = u64::from(old)
                    .checked_add(amount)
                    .ok_or("Слишком большое количество")?;
                let cap = if items::equippable(kind) {
                    65536
                } else {
                    1_000_000_000
                };
                if count > cap {
                    return Err("Превышен предел количества вещей/валюты".into());
                }
                let cells: u64 = (1..=u8::MAX)
                    .map(|k| {
                        let n = layout::quantity(&c, k);
                        if items::equippable(k) {
                            u64::from(n)
                        } else {
                            u64::from(n > 0)
                        }
                    })
                    .sum();
                let extra = if items::equippable(kind) {
                    amount
                } else {
                    u64::from(old == 0)
                };
                if cells + extra > 65536 {
                    return Err("Освободите место в инвентаре".into());
                }
                count as u32
            } else {
                let remove = amount.unwrap_or(u64::from(old));
                if remove > u64::from(old) {
                    return Err(format!("Недостаточно предметов: {}", def.name));
                }
                old - remove as u32
            };
            if let Some(key) = items::currency(kind) {
                c.stats.insert(key.into(), f64::from(next));
            } else {
                c.loot.insert(kind, next);
                if let Some(item) = c.inventory.iter_mut().find(|i| i.kind == kind) {
                    let copy = c
                        .stats
                        .get(&format!("gear_copy_{kind}"))
                        .copied()
                        .unwrap_or(0.) as u32;
                    if copy >= next {
                        item.equipped = false;
                        c.stats.remove(&format!("gear_copy_{kind}"));
                    }
                }
                items::include_loot(&mut c.inventory, &c.loot);
            }
        }
        let invalid: Vec<_> = c
            .stats
            .iter()
            .filter(|(k, v)| {
                (k.starts_with("bag_") || k.starts_with("hotbar_"))
                    && !layout::valid(&c, **v as u64)
            })
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
    sqlx::query("UPDATE characters SET stats=$2,inventory=$3,loot=$4 WHERE id=$1")
        .bind(target_id)
        .bind(serde_json::to_value(&c.stats).unwrap())
        .bind(serde_json::to_value(&c.inventory).unwrap())
        .bind(serde_json::to_value(&c.loot).unwrap())
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
    let updated = sqlx::query("SELECT * FROM characters WHERE id=$1")
        .bind(target_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    let updated = crate::store::character(&updated)?;
    tx.commit().await.map_err(db)?;
    if let Some(mut actor) = current {
        sim.exchanges
            .cancel(target_id, "Обмен отменён: изменения администратором");
        actor.progress = Progress::from_stats(&c.stats);
        actor.loot = c.loot;
        actor.actions.clear();
        sim.replace_actor(actor);
        sim.refresh_character(&updated);
    }
    server.admin_events.send_replace(());
    Ok(notice(format!(
        "{name}: {nickname} (ID {target_id}) — выполнено"
    )))
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
    let escrow:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM character_mail WHERE (sender_id=$1 OR recipient_id=$1) AND attachment_kind IS NOT NULL AND NOT attachment_claimed AND NOT attachment_returned) OR EXISTS(SELECT 1 FROM mail_payments WHERE (owner_id=$1 OR mail_id IN (SELECT id FROM character_mail WHERE sender_id=$1 OR recipient_id=$1)) AND NOT settled)").bind(target_id).fetch_one(&mut *tx).await.map_err(db)?;
    if escrow {
        return Err("Сначала завершите почтовые вложения и расчёты персонажа".into());
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
