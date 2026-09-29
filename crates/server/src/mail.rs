use mortown_protocol::mail::{self, Command, Folder, Header, Letter, Page, Reply};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

fn database_error(error: sqlx::Error) -> String {
    // Do not print query parameters: letters are private player correspondence.
    eprintln!(
        "Mail database operation failed ({:?})",
        error.as_database_error().and_then(|e| e.code())
    );
    "Почта временно недоступна. Попробуйте снова".into()
}

pub async fn unread(conn: &mut PgConnection, owner: i64) -> Result<u64, String> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM character_mail WHERE recipient_id=$1 AND NOT recipient_deleted AND NOT is_read",
    ).bind(owner).fetch_one(conn).await.map_err(database_error)?;
    Ok(count as u64)
}

fn header(row: &PgRow) -> Header {
    Header {
        return_of: row.get("return_of"),
        id: row.get("id"),
        sender: row.get("sender"),
        recipient: row.get("recipient"),
        subject: row.get("subject"),
        sent_at: row.get("date"),
        read: row.get("is_read"),
        attachment: row
            .get::<Option<i16>, _>("attachment_kind")
            .map(|kind| mail::Attachment {
                kind: kind as u8,
                price: row.get::<i32, _>("attachment_price") as u32,
                quantity: row.get::<i32, _>("attachment_quantity") as u32,
                claimed: row.get("attachment_claimed"),
                returned: row.get("attachment_returned"),
            }),
    }
}

// Only the static ownership condition is selected by code; all user data is bound.
fn ownership(folder: Folder) -> &'static str {
    match folder {
        Folder::Inbox => "m.recipient_id=$1 AND NOT m.recipient_deleted",
        Folder::Sent => "m.sender_id=$1 AND NOT m.sender_deleted",
    }
}
const COLUMNS: &str = "m.id, m.return_of, m.attachment_returned, m.subject, m.is_read, m.attachment_kind, m.attachment_quantity, m.attachment_claimed, m.attachment_price, s.nickname AS sender, r.nickname AS recipient, to_char(m.sent_at AT TIME ZONE 'UTC', 'DD.MM.YYYY HH24:MI') AS date";
const JOINS: &str = "FROM character_mail m JOIN characters s ON s.id=m.sender_id JOIN characters r ON r.id=m.recipient_id";
const VISIBLE: &str = "((m.sender_id=$1 AND NOT m.sender_deleted) OR (m.recipient_id=$1 AND NOT m.recipient_deleted))";

async fn list(
    conn: &mut PgConnection,
    owner: i64,
    folder: Folder,
    page: u32,
) -> Result<Page, String> {
    // Keep the count, unread badge and page consistent while another player sends a letter.
    let mut tx = conn.begin().await.map_err(database_error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM character_mail m WHERE {}",
        ownership(folder)
    ))
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .map_err(database_error)?;
    let total = count as u64;
    let page = u64::from(page).min(total.saturating_sub(1) / mail::PAGE_SIZE as u64) as u32;
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} {JOINS} WHERE {} ORDER BY m.id DESC LIMIT $2 OFFSET $3",
        ownership(folder)
    ))
    .bind(owner)
    .bind(mail::PAGE_SIZE as i64)
    .bind(i64::from(page) * mail::PAGE_SIZE as i64)
    .fetch_all(&mut *tx)
    .await
    .map_err(database_error)?;
    let unread = unread(&mut tx, owner).await?;
    tx.commit().await.map_err(database_error)?;
    Ok(Page {
        folder,
        page,
        total,
        unread,
        entries: rows.iter().map(header).collect(),
    })
}

async fn read(conn: &mut PgConnection, owner: i64, id: i64) -> Result<Letter, String> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    // Opening sent mail must never mark the recipient's copy as read.
    sqlx::query("UPDATE character_mail SET is_read=true WHERE id=$2 AND recipient_id=$1 AND NOT recipient_deleted")
        .bind(owner).bind(id).execute(&mut *tx).await.map_err(database_error)?;
    let row = sqlx::query(&format!(
        "SELECT {COLUMNS}, m.body {JOINS} WHERE m.id=$2 AND {VISIBLE}"
    ))
    .bind(owner)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?
    .ok_or("Письмо не найдено")?;
    let letter = Letter {
        header: header(&row),
        body: row.get("body"),
    };
    tx.commit().await.map_err(database_error)?;
    Ok(letter)
}

pub(crate) async fn inventory(
    conn: &mut PgConnection,
    actor: &mortown_protocol::Actor,
) -> Result<mortown_protocol::Character, String> {
    let row = sqlx::query("SELECT * FROM characters WHERE id=$1 FOR NO KEY UPDATE")
        .bind(actor.id)
        .fetch_one(conn)
        .await
        .map_err(database_error)?;
    let mut c = crate::store::character(&row)?;
    crate::store::check_admin_revision(&c, actor)?;
    // Persist base stats, never effective equipment bonuses or level growth.
    c.stats = serde_json::from_value(row.get("stats")).map_err(|_| "Ошибка характеристик")?;
    actor.progress.write_stats(&mut c.stats);
    c.loot = actor.loot.clone();
    mortown_protocol::items::include_loot(&mut c.inventory, &c.loot);
    Ok(c)
}
pub(crate) async fn persist(
    conn: &mut PgConnection,
    mut actor: mortown_protocol::Actor,
    c: mortown_protocol::Character,
    status: &str,
) -> Result<Box<mail::InventoryUpdate>, String> {
    // Cached exchange/mail contents cannot undo a committed admin mutation, even if its ack was lost.
    crate::store::check_admin_revision(&c, &actor)?;
    let revision:f64=sqlx::query_scalar("SELECT coalesce((stats->>'admin_revision')::double precision,0) FROM characters WHERE id=$1 FOR NO KEY UPDATE").bind(actor.id).fetch_one(&mut *conn).await.map_err(database_error)?;
    if revision != f64::from(actor.progress.admin_revision) {
        return Err(crate::store::STALE_ADMIN.into());
    }
    actor.equipment = mortown_protocol::items::EquipmentAppearance::from_inventory(&c.inventory);
    actor.loot = c.loot;
    actor.reward_status = status.into();
    sqlx::query("UPDATE characters SET stats=$2,inventory=$3 WHERE id=$1")
        .bind(actor.id)
        .bind(serde_json::to_value(c.stats).unwrap())
        .bind(serde_json::to_value(c.inventory).unwrap())
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;
    crate::store::save(conn, &actor).await?;
    let row = sqlx::query("SELECT * FROM characters WHERE id=$1")
        .bind(actor.id)
        .fetch_one(conn)
        .await
        .map_err(database_error)?;
    Ok(Box::new(mail::InventoryUpdate {
        character: crate::store::character(&row)?,
        actor,
    }))
}
#[allow(clippy::too_many_arguments)]
async fn send(
    conn: &mut PgConnection,
    owner: i64,
    recipient: String,
    subject: String,
    body: String,
    draft: &str,
    attachment: Option<mail::AttachmentRequest>,
    actor: Option<mortown_protocol::Actor>,
) -> Result<(i64, Option<Box<mail::InventoryUpdate>>), String> {
    let recipient = recipient.trim();
    let subject = subject.trim();
    let body = body.trim();
    mail::validate(recipient, subject, body)?;
    if draft.len() != 32
        || !draft
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("Не удалось распознать черновик письма".into());
    }
    let token = attachment
        .map(|a| i64::try_from(a.token).map_err(|_| "Недопустимое вложение"))
        .transpose()?;
    let price = attachment.map_or(0, |a| a.price);
    if price > mail::MAX_PRICE {
        return Err("Цена вложения: от 0 до 1 000 000 монет".into());
    }
    let amount = attachment.map(|a| i64::from(a.quantity));
    let mut tx = conn.begin().await.map_err(database_error)?;
    // NO KEY UPDATE serializes this sender without blocking another sender's recipient FK.
    sqlx::query("SELECT id FROM characters WHERE id=$1 FOR NO KEY UPDATE")
        .bind(owner)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let receiver: i64 =
        sqlx::query_scalar("SELECT id FROM characters WHERE lower(nickname)=lower($1)")
            .bind(recipient)
            .fetch_optional(&mut *tx)
            .await
            .map_err(database_error)?
            .ok_or("Персонаж с таким именем не найден")?;
    if receiver == owner {
        return Err("Нельзя отправить письмо самому себе".into());
    }
    let blocked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM chat_ignores WHERE owner_id=$1 AND blocked_id=$2)",
    )
    .bind(receiver)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .map_err(database_error)?;
    if blocked {
        return Err("Почта этому персонажу недоступна".into());
    }
    if let Some(existing)=sqlx::query("SELECT id,recipient_id,subject,body,attachment_token,attachment_quantity,attachment_price FROM character_mail WHERE sender_id=$1 AND draft=$2 AND return_of IS NULL")
        .bind(owner).bind(draft).fetch_optional(&mut *tx).await.map_err(database_error)?{
        if existing.get::<i32,_>("attachment_price") != price as i32 || existing.get::<i64,_>("recipient_id")!=receiver||existing.get::<String,_>("subject")!=subject||existing.get::<String,_>("body")!=body
            ||existing.get::<Option<i64>,_>("attachment_token")!=token||existing.get::<Option<i32>,_>("attachment_quantity").map(i64::from)!=amount{
            return Err("Этот черновик уже отправлен. Создайте новое письмо".into());
        }
        let update=if let Some(actor)=actor {let c=inventory(&mut tx,&actor).await?;Some(persist(&mut tx,actor,c,"Письмо уже отправлено").await?)}else{None};
        let id=existing.get("id");tx.commit().await.map_err(database_error)?;return Ok((id,update));
    }
    let limited:bool=sqlx::query_scalar("SELECT count(*)>=200 OR coalesce(bool_or(sent_at>now()-interval '3 seconds'),false) FROM character_mail WHERE sender_id=$1 AND return_of IS NULL AND sent_at>now()-interval '24 hours'")
        .bind(owner).fetch_one(&mut *tx).await.map_err(database_error)?;
    if limited {
        return Err("Письма можно отправлять раз в 3 секунды, не более 200 за сутки".into());
    }
    let (kind, update) = if let Some(attachment) = attachment {
        let actor = actor.ok_or("Состояние персонажа недоступно")?;
        if actor.id != owner || actor.hp <= 0. {
            return Err("Сначала возродитесь".into());
        }
        let mut c = inventory(&mut tx, &actor).await?;
        let kind = mortown_protocol::transfer::take(&mut c, &attachment)?;
        let update = persist(
            &mut tx,
            actor,
            c,
            "Письмо отправлено, вложение передано на хранение почте",
        )
        .await?;
        (Some(i16::from(kind)), Some(update))
    } else {
        (None, None)
    };
    let id=sqlx::query_scalar("INSERT INTO character_mail(sender_id,recipient_id,subject,body,draft,attachment_kind,attachment_quantity,attachment_token,attachment_price) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id")
        .bind(owner).bind(receiver).bind(subject).bind(body).bind(draft).bind(kind).bind(amount.map(|n|n as i32)).bind(token).bind(price as i32)
        .fetch_one(&mut *tx).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok((id, update))
}
async fn claim(
    conn: &mut PgConnection,
    owner: i64,
    id: i64,
    mut actor: mortown_protocol::Actor,
    expected_price: u32,
) -> Result<Box<mail::InventoryUpdate>, String> {
    if actor.id != owner {
        return Err("Неверный получатель".into());
    }
    let mut tx = conn.begin().await.map_err(database_error)?;
    let mut c = inventory(&mut tx, &actor).await?;
    let row=sqlx::query("SELECT sender_id,attachment_price,attachment_kind,attachment_quantity,attachment_claimed,attachment_returned FROM character_mail WHERE id=$1 AND recipient_id=$2 AND NOT recipient_deleted FOR UPDATE")
        .bind(id).bind(owner).fetch_optional(&mut *tx).await.map_err(database_error)?.ok_or("Письмо не найдено")?;
    let kind = row
        .get::<Option<i16>, _>("attachment_kind")
        .ok_or("В письме нет вложения")? as u8;
    if row.get::<bool, _>("attachment_returned") {
        return Err("Вложение уже возвращено отправителю".into());
    }
    let price = row.get::<i32, _>("attachment_price") as u32;
    if price != expected_price {
        return Err("Подтвердите указанную в письме цену".into());
    }
    if !row.get::<bool, _>("attachment_claimed") {
        if !actor.progress.coins.is_finite() || actor.progress.coins < f64::from(price) {
            return Err("Недостаточно монет для получения вложения".into());
        }
        if actor.hp <= 0. {
            return Err("Сначала возродитесь".into());
        }
        mortown_protocol::transfer::give(
            &mut c,
            kind,
            row.get::<i32, _>("attachment_quantity") as u32,
        )?;
        actor.progress.coins -= f64::from(price);
        actor.progress.write_stats(&mut c.stats);
        if price > 0 {
            sqlx::query("INSERT INTO mail_payments(mail_id,owner_id,amount) VALUES($1,$2,$3)")
                .bind(id)
                .bind(row.get::<i64, _>("sender_id"))
                .bind(i64::from(price))
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
        sqlx::query("UPDATE character_mail SET attachment_claimed=true,is_read=true WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
    }
    let update = persist(&mut tx, actor, c, "Вложение получено").await?;
    tx.commit().await.map_err(database_error)?;
    Ok(update)
}
// Return transfers escrow between letters, never directly into an offline inventory.
async fn return_attachment(conn: &mut PgConnection, owner: i64, id: i64) -> Result<(), String> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    let row = sqlx::query(&format!(
        "SELECT * FROM character_mail m WHERE m.id=$2 AND {VISIBLE} FOR UPDATE"
    ))
    .bind(owner)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?
    .ok_or("Письмо не найдено")?;
    if row.get::<Option<i64>, _>("return_of").is_some() {
        return Err("Возвращённую вещь можно только забрать".into());
    }
    let kind = row
        .get::<Option<i16>, _>("attachment_kind")
        .ok_or("В письме нет вложения")?;
    if row.get::<bool, _>("attachment_claimed") {
        return Err("Вложение уже получено; возврат невозможен".into());
    }
    if !row.get::<bool, _>("attachment_returned") {
        let subject: String = format!("Возврат: {}", row.get::<String, _>("subject"))
            .chars()
            .take(mail::SUBJECT_LIMIT)
            .collect();
        sqlx::query("INSERT INTO character_mail(sender_id,recipient_id,subject,body,draft,attachment_kind,attachment_quantity,attachment_token,attachment_price,return_of,sender_deleted) VALUES($1,$2,$3,$4,$5,$6,$7,$8,0,$9,true)")
            .bind(row.get::<i64,_>("recipient_id")).bind(row.get::<i64,_>("sender_id"))
            .bind(subject).bind("Незабранное вложение возвращено отправителю. Заберите вещь бесплатно; повторный возврат недоступен.")
            .bind(row.get::<String,_>("draft")).bind(kind).bind(row.get::<i32,_>("attachment_quantity"))
            .bind(row.get::<i64,_>("attachment_token")).bind(id)
            .execute(&mut *tx).await.map_err(database_error)?;
        sqlx::query("UPDATE character_mail SET attachment_returned=true,is_read=true WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
    }
    tx.commit().await.map_err(database_error)?;
    Ok(())
}
async fn delete(conn: &mut PgConnection, owner: i64, id: i64) -> Result<(), String> {
    // Sender may hide their sent copy; the recipient must first claim the escrowed item.
    let result=sqlx::query("UPDATE character_mail SET sender_deleted=sender_deleted OR sender_id=$1,recipient_deleted=recipient_deleted OR recipient_id=$1 WHERE id=$2 AND (sender_id=$1 OR (recipient_id=$1 AND (attachment_kind IS NULL OR attachment_claimed OR attachment_returned)))")
        .bind(owner).bind(id).execute(conn).await.map_err(database_error)?;
    if result.rows_affected() == 0 {
        return Err("Письмо не найдено или сначала нужно забрать вложение".into());
    }
    Ok(())
}
pub async fn handle(
    conn: &mut PgConnection,
    owner: i64,
    command: Command,
    actor: Option<mortown_protocol::Actor>,
) -> Reply {
    let result: Result<Reply, String> = async {
        crate::console::mail_allowed(conn, owner).await?;
        Ok(match command {
            Command::List { folder, page } => Reply::List {
                page: list(conn, owner, folder, page).await?,
            },
            Command::Read { id } => Reply::Read {
                letter: read(conn, owner, id).await?,
            },
            Command::Send {
                recipient,
                subject,
                body,
                draft,
                attachment,
            } => {
                let (id, update) = send(
                    conn, owner, recipient, subject, body, &draft, attachment, actor,
                )
                .await?;
                Reply::Sent { id, draft, update }
            }
            Command::Claim { id, expected_price } => Reply::Claimed {
                id,
                update: claim(
                    conn,
                    owner,
                    id,
                    actor.ok_or("Состояние персонажа недоступно")?,
                    expected_price,
                )
                .await?,
            },
            Command::Return { id } => {
                return_attachment(conn, owner, id).await?;
                Reply::Returned { id }
            }
            Command::Delete { id } => {
                delete(conn, owner, id).await?;
                Reply::Deleted { id }
            }
        })
    }
    .await;
    result.unwrap_or_else(|message| Reply::Error { message })
}

// Apply durable credits only to the owner's authoritative actor, under the simulation lock.
// Saving coins and consuming precisely these ledger rows is one transaction.
pub async fn settle(
    conn: &mut PgConnection,
    mut actor: mortown_protocol::Actor,
) -> Result<mortown_protocol::Actor, String> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    sqlx::query("SELECT id FROM characters WHERE id=$1 FOR NO KEY UPDATE")
        .bind(actor.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    let rows = sqlx::query(
        "SELECT mail_id,amount FROM mail_payments WHERE owner_id=$1 AND NOT settled FOR UPDATE",
    )
    .bind(actor.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(database_error)?;
    let total = rows
        .iter()
        .try_fold(0_i64, |sum, row| {
            sum.checked_add(row.get::<i64, _>("amount"))
        })
        .ok_or("Переполнение почтового баланса")?;
    if total > 0 {
        let balance = actor.progress.coins + total as f64;
        // Leave payments pending instead of discarding money above the inventory currency limit.
        if balance.is_finite() && balance <= f64::from(u32::MAX) {
            actor.progress.coins = balance;
            actor.reward_status = format!("Оплата почтовых вложений: +{total} монет");
            let ids: Vec<i64> = rows.iter().map(|r| r.get("mail_id")).collect();
            sqlx::query("UPDATE mail_payments SET settled=true WHERE mail_id=ANY($1)")
                .bind(ids)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        } else {
            actor.reward_status =
                "Почтовая оплата ожидает зачисления: освободите место в балансе монет".into();
        }
    }
    crate::store::save(&mut tx, &actor).await?;
    tx.commit().await.map_err(database_error)?;
    Ok(actor)
}
