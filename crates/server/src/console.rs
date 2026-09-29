//! Authenticated chat commands. Client text never grants a role or chooses a sender.
mod admin;
use mortown_protocol::chat::{Dialog, DialogKind, Reply, commands};
use rand_core::{OsRng, RngCore};
use sqlx::{Connection, PgConnection, Row};
use std::time::{Duration, Instant};
pub(crate) fn db(_: sqlx::Error) -> String {
    "Команда временно недоступна".into()
}
pub struct Access {
    pub account: i64,
    pub epoch: i64,
    pub ip: Option<String>,
}
pub async fn access(
    conn: &mut PgConnection,
    account: i64,
    ip: Option<String>,
) -> Result<Access, String> {
    let epoch = sqlx::query_scalar("SELECT session_epoch FROM accounts WHERE id=$1")
        .bind(account)
        .fetch_one(&mut *conn)
        .await
        .map_err(db)?;
    let access = Access { account, epoch, ip };
    check_access(conn, &access).await?;
    sqlx::query("UPDATE accounts SET last_ip=$2::text::inet WHERE id=$1")
        .bind(account)
        .bind(&access.ip)
        .execute(conn)
        .await
        .map_err(db)?;
    Ok(access)
}
pub async fn check_access(conn: &mut PgConnection, access: &Access) -> Result<(), String> {
    let row = sqlx::query("SELECT session_epoch,coalesce(banned_until>now(),false) AS banned,ban_reason FROM accounts WHERE id=$1").bind(access.account).fetch_one(&mut *conn).await.map_err(db)?;
    if row.get::<bool, _>("banned") {
        return Err(format!(
            "Аккаунт заблокирован: {}",
            row.get::<String, _>("ban_reason")
        ));
    }
    if row.get::<i64, _>("session_epoch") != access.epoch {
        return Err("Сессия завершена администратором".into());
    }
    if let Some(ip) = &access.ip {
        let banned: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM blocked_ips WHERE ip=$1::text::inet)")
                .bind(ip)
                .fetch_one(conn)
                .await
                .map_err(db)?;
        if banned {
            return Err("Вход с этого IP заблокирован".into());
        }
    }
    Ok(())
}
pub async fn role(conn: &mut PgConnection, owner: i64) -> Result<i16, String> {
    sqlx::query_scalar(
        "SELECT a.role FROM accounts a JOIN characters c ON c.account_id=a.id WHERE c.id=$1",
    )
    .bind(owner)
    .fetch_one(conn)
    .await
    .map_err(db)
}
pub(crate) async fn target(
    conn: &mut PgConnection,
    value: &str,
) -> Result<(i64, String, i64, i16), String> {
    let id = value.parse::<i64>().ok().filter(|id| *id > 0);
    let row=sqlx::query("SELECT c.id,c.nickname,c.account_id,a.role FROM characters c JOIN accounts a ON a.id=c.account_id WHERE ($1::bigint IS NOT NULL AND c.id=$1) OR ($1::bigint IS NULL AND lower(c.nickname)=lower($2))")
        .bind(id).bind(value).fetch_optional(conn).await.map_err(db)?.ok_or("Персонаж не найден")?;
    Ok((
        row.get("id"),
        row.get("nickname"),
        row.get("account_id"),
        row.get("role"),
    ))
}
pub async fn mail_allowed(conn: &mut PgConnection, owner: i64) -> Result<(), String> {
    let muted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM global_mutes WHERE character_id=$1 AND until_at>now())",
    )
    .bind(owner)
    .fetch_one(conn)
    .await
    .map_err(db)?;
    if muted {
        Err("Почта временно заблокирована общим мутом".into())
    } else {
        Ok(())
    }
}
pub fn notice(message: impl Into<String>) -> Reply {
    Reply::Notice {
        message: message.into(),
    }
}
pub fn dialog(
    kind: DialogKind,
    title: &str,
    text: String,
    entries: Vec<(i64, String)>,
    token: Option<String>,
) -> Reply {
    Reply::Dialog {
        dialog: Dialog {
            kind,
            title: title.into(),
            text,
            entries,
            token,
            admin: None,
        },
    }
}
pub enum PendingAction {
    ClearBag(i64),
    ResetWorld,
    Remove(mortown_protocol::admin::Target),
    Heal { maximum: u32 },
    Delete { id: i64 },
}
pub struct Confirmation {
    pub token: String,
    pub until: Instant,
    pub action: PendingAction,
}
pub fn confirmation(
    state: &mut crate::chat::Session,
    title: &str,
    text: String,
    action: PendingAction,
) -> Reply {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    state.confirmation = Some(Confirmation {
        token: token.clone(),
        until: Instant::now() + Duration::from_secs(30),
        action,
    });
    dialog(DialogKind::Confirm, title, text, vec![], Some(token))
}
pub async fn ignores(conn: &mut PgConnection, owner: i64) -> Result<Reply, String> {
    let rows=sqlx::query("SELECT c.id,c.nickname FROM chat_ignores i JOIN characters c ON c.id=i.blocked_id WHERE i.owner_id=$1 ORDER BY lower(c.nickname)").bind(owner).fetch_all(conn).await.map_err(db)?;
    Ok(dialog(
        DialogKind::IgnoreList,
        "Список игнора",
        "Нажмите × возле имени, чтобы убрать игнор.".into(),
        rows.iter()
            .map(|r| (r.get("id"), r.get("nickname")))
            .collect(),
        None,
    ))
}
pub async fn execute(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    state: &mut crate::chat::Session,
    text: &str,
) -> Result<Reply, String> {
    state.confirmation = None;
    let call = commands::parse(text)?.ok_or("Это не команда")?;
    let staff = role(conn, owner).await?;
    if staff < call.spec.role {
        return Err("Недостаточно прав для этой команды".into());
    }
    let name = call.spec.name;
    let args = &call.args;
    if name.starts_with("//") && name != "//help" {
        return admin::execute(server, conn, owner, state, &call, staff).await;
    }
    match name {
        "/help" | "//help" => Ok(dialog(
            DialogKind::Help,
            "Команды чата",
            format!(
                "Ваш ID: {owner} · Роль аккаунта: {staff}\n\n{}\n\nЛичное сообщение: \"ник текст\nКоманда должна начинаться с первого символа.\nМонеты: itemid 11; золото: itemid 12.\nГруппа: до {} участников.",
                commands::help(staff, name == "//help"),
                mortown_protocol::party::LIMIT
            ),
            vec![],
            None,
        )),
        "/q" => Ok(Reply::Logout),
        ".cfg" => Ok(dialog(
            DialogKind::GameSettings,
            "Настройки игры",
            String::new(),
            vec![],
            None,
        )),
        ".chst" => Ok(dialog(
            DialogKind::ChatSettings,
            "Настройки чата",
            String::new(),
            vec![],
            None,
        )),
        "/time" => {
            let time:String=sqlx::query_scalar("SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC','DD.MM.YYYY HH24:MI:SS') || ' UTC'").fetch_one(conn).await.map_err(db)?;
            Ok(notice(format!("Время сервера: {time}")))
        }
        "/ignorelist" => ignores(conn, owner).await,
        "/ignore" | "/unignore" => {
            let (id, _, _, _) = target(conn, args[0]).await?;
            if id == owner {
                return Err("Нельзя игнорировать себя".into());
            }
            if name == "/ignore" {
                if state.ignored.len() >= 100 && !state.ignored.contains(&id) {
                    return Err("В игноре уже 100 персонажей".into());
                }
                sqlx::query("INSERT INTO chat_ignores(owner_id,blocked_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(owner).bind(id).execute(&mut *conn).await.map_err(db)?;
                state.ignored.insert(id);
            } else {
                sqlx::query("DELETE FROM chat_ignores WHERE owner_id=$1 AND blocked_id=$2")
                    .bind(owner)
                    .bind(id)
                    .execute(&mut *conn)
                    .await
                    .map_err(db)?;
                state.ignored.remove(&id);
            }
            if name == "/unignore" {
                ignores(conn, owner).await
            } else {
                Ok(notice("Игнор включён"))
            }
        }
        "/ignoreall" | "/unignoreall" | "/silence" | "/unsilence" => {
            let mut tx = conn.begin().await.map_err(db)?;
            let all = if name == "/ignoreall" {
                true
            } else if name == "/unignoreall" {
                false
            } else {
                state.ignore_all
            };
            let silence = if name == "/silence" {
                true
            } else if name == "/unsilence" {
                false
            } else {
                state.silence
            };
            if name == "/unignoreall" {
                sqlx::query("DELETE FROM chat_ignores WHERE owner_id=$1")
                    .bind(owner)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
            }
            sqlx::query("INSERT INTO chat_preferences(character_id,ignore_all,silence) VALUES($1,$2,$3) ON CONFLICT(character_id) DO UPDATE SET ignore_all=$2,silence=$3").bind(owner).bind(all).bind(silence).execute(&mut *tx).await.map_err(db)?;
            tx.commit().await.map_err(db)?;
            state.ignore_all = all;
            state.silence = silence;
            if name == "/unignoreall" {
                state.ignored.clear();
            }
            Ok(notice(format!(
                "Чат: {}. Личные сообщения: {}.",
                if all { "скрыт" } else { "виден" },
                if silence {
                    "запрещены"
                } else {
                    "разрешены"
                }
            )))
        }
        "/invite" | "/leave" | "/lead" | "/kick" => {
            use mortown_protocol::party::Command;
            let id = if name != "/leave" {
                Some(target(conn, args[0]).await?.0)
            } else {
                None
            };
            if name == "/invite" {
                let blocked: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM chat_ignores WHERE owner_id=$1 AND blocked_id=$2)",
                )
                .bind(id)
                .bind(owner)
                .fetch_one(&mut *conn)
                .await
                .map_err(db)?;
                if blocked {
                    return Err("Приглашение недоступно".into());
                }
            }
            let mut sim = server.simulation.lock().await;
            let group = sim.parties.membership(owner);
            let command = match name {
                "/invite" => Command::Invite {
                    player: id.unwrap(),
                },
                "/leave" => Command::Leave {
                    party: group.ok_or("Вы не в группе")?,
                },
                "/lead" => Command::Promote {
                    party: group.ok_or("Вы не в группе")?,
                    player: id.unwrap(),
                },
                _ => Command::Kick {
                    party: group.ok_or("Вы не в группе")?,
                    player: id.unwrap(),
                },
            };
            let actors = sim.snapshot();
            sim.parties.handle(owner, &actors, command);
            Ok(notice(
                sim.parties
                    .last_message(owner)
                    .unwrap_or("Состав группы обновлён"),
            ))
        }
        "/heal" => {
            let sim = server.simulation.lock().await;
            let a = sim.actor(owner).ok_or("Персонаж не в мире")?;
            let price = heal_price(&sim, &a)?;
            Ok(confirmation(
                state,
                "Больница",
                format!(
                    "Восстановить здоровье до {:.0} HP?\nСтоимость: {price} монет.\nПредложение действует 30 секунд.",
                    a.max_hp
                ),
                PendingAction::Heal { maximum: price },
            ))
        }
        _ => Err("Неизвестная команда".into()),
    }
}
fn heal_price(
    sim: &crate::simulation::Simulation,
    a: &mortown_protocol::Actor,
) -> Result<u32, String> {
    if sim.in_combat(a.id) || a.attack_target.is_some() {
        return Err("Лечение недоступно в бою и 10 секунд после него".into());
    }
    if a.hp <= 0. {
        return Err("Сначала возродитесь".into());
    }
    let missing = (a.max_hp - a.hp).max(0.);
    if missing < 0.01 {
        return Err("Здоровье уже полное".into());
    }
    Ok((missing * 0.2).ceil().max(1.) as u32)
}
pub async fn confirm(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    state: &mut crate::chat::Session,
    token: &str,
) -> Result<Reply, String> {
    let pending = state
        .confirmation
        .take()
        .ok_or("Нет ожидающего подтверждения")?;
    if pending.token != token || pending.until < Instant::now() {
        return Err("Подтверждение истекло. Повторите команду".into());
    }
    match pending.action {
        PendingAction::ClearBag(id) => {
            let text = format!("//takeawayall {id}");
            let call = commands::parse(&text)?.unwrap();
            admin::inventory(server, conn, owner, &call).await
        }
        PendingAction::ResetWorld => admin::control::reset(server, conn, owner).await,
        PendingAction::Remove(target) => admin::control::remove(server, conn, owner, target).await,
        PendingAction::Delete { id } => admin::delete(server, conn, owner, id).await,
        PendingAction::Heal { maximum } => {
            let mut sim = server.simulation.lock().await;
            let mut a = sim.actor(owner).ok_or("Персонаж не в мире")?;
            let price = heal_price(&sim, &a)?;
            if price > maximum {
                return Err("Цена изменилась. Повторите /heal".into());
            }
            if a.progress.coins < f64::from(price) {
                return Err("Не хватает монет".into());
            }
            a.hp = a.max_hp;
            a.progress.coins -= f64::from(price);
            sim.exchanges.cancel(owner, "Обмен отменён: лечение");
            crate::store::save(conn, &a).await?;
            sim.replace_actor(a);
            server.admin_events.send_replace(());
            Ok(notice(format!(
                "Здоровье восстановлено. Оплачено {price} монет"
            )))
        }
    }
}
