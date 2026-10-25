use mortown_protocol::{
    Actor, Character,
    alliance::{self, Command, Invitation, Member, Reply, Role, View},
};
use sqlx::{Connection, PgConnection, Row};

// Social mutations are infrequent. Serialize them across connections/processes so capacity,
// competing invitations and leader changes are checked against the same committed membership.
pub const LOCK: i64 = -7043001;
pub(crate) fn database(error: sqlx::Error) -> String {
    if error
        .as_database_error()
        .is_some_and(|e| e.is_unique_violation())
    {
        "Название занято или состав уже изменился. Обновите окно альянса".into()
    } else {
        eprintln!("Alliance database operation failed: {error}");
        "Альянсы временно недоступны".into()
    }
}
fn role(leader: i64, owner: i64, officer: bool) -> Role {
    if leader == owner {
        Role::Leader
    } else if officer {
        Role::Officer
    } else {
        Role::Member
    }
}
async fn seats(conn: &mut PgConnection, owner: i64) -> Result<Vec<(i64, i16, Role)>, String> {
    let rows = sqlx::query(
        "SELECT a.id, a.kind, a.leader_id, m.officer FROM alliance_members m \
         JOIN alliances a ON a.id = m.alliance_id \
         JOIN characters c ON c.id = m.character_id \
         WHERE m.character_id=$1 AND (m.identity = COALESCE((c.stats->>'identity')::float8, 1) \
            OR a.leader_id = m.character_id) \
         ORDER BY a.kind, a.id",
    )
    .bind(owner)
    .fetch_all(&mut *conn)
    .await
    .map_err(database)?;
    Ok(rows
        .iter()
        .map(|row| {
            (
                row.get("id"),
                row.get("kind"),
                role(row.get("leader_id"), owner, row.get("officer")),
            )
        })
        .collect())
}
pub async fn audience(conn: &mut PgConnection, owner: i64) -> Result<(i64, Vec<i64>), String> {
    let owned = seats(conn, owner).await?;
    let id = owned
        .first()
        .map(|seat| seat.0)
        .ok_or("Вы не состоите в организации")?;
    let mut members = Vec::new();
    for (org, _, _) in &owned {
        let rows: Vec<i64> =
            sqlx::query_scalar("SELECT character_id FROM alliance_members WHERE alliance_id=$1")
                .bind(org)
                .fetch_all(&mut *conn)
                .await
                .map_err(database)?;
        members.extend(rows);
    }
    members.sort_unstable();
    members.dedup();
    Ok((id, members))
}
pub async fn view(conn: &mut PgConnection, owner: i64, actors: &[Actor]) -> Result<Reply, String> {
    let mut tx = conn.begin().await.map_err(database)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    let mut reply = Reply::default();
    let owned = seats(&mut tx, owner).await?;
    for (id, kind, _) in &owned {
        let a = sqlx::query("SELECT name,leader_id FROM alliances WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        let leader: i64 = a.get("leader_id");
        let rows=sqlx::query("SELECT c.id,c.nickname,coalesce((c.stats->>'level')::double precision,1) AS level,m.officer FROM alliance_members m JOIN characters c ON c.id=m.character_id WHERE m.alliance_id=$1 ORDER BY (c.id=$2) DESC,m.officer DESC,m.joined_at,c.id")
            .bind(id).bind(leader).fetch_all(&mut *tx).await.map_err(database)?;
        reply.organizations.push(View {
            id: *id,
            kind: alliance::Kind::parse(*kind),
            name: a.get("name"),
            members: rows
                .iter()
                .map(|r| {
                    let id = r.get("id");
                    let actor = actors.iter().find(|a| a.id == id);
                    Member {
                        id,
                        nickname: r.get("nickname"),
                        level: actor.map_or(r.get::<f64, _>("level") as u32, |a| a.progress.level),
                        role: role(leader, id, r.get("officer")),
                        online: actor.is_some(),
                    }
                })
                .collect(),
        });
    }
    reply.alliance = reply
        .organizations
        .iter()
        .find(|org| org.kind == alliance::Kind::Guild)
        .cloned()
        .or_else(|| reply.organizations.first().cloned());
    if reply.alliance.is_none()
        && let Some(r)=sqlx::query("SELECT i.id,i.alliance_id,a.name,c.nickname,greatest(0,ceil(extract(epoch FROM i.expires_at-now())))::int AS seconds FROM alliance_invitations i JOIN alliances a ON a.id=i.alliance_id JOIN characters c ON c.id=i.inviter_id JOIN alliance_members m ON m.character_id=i.inviter_id AND m.alliance_id=a.id WHERE i.recipient_id=$1 AND i.expires_at>now() AND (a.leader_id=i.inviter_id OR m.officer)")
        .bind(owner).fetch_optional(&mut *tx).await.map_err(database)? {
        reply.invitation=Some(Invitation{id:r.get("id"),alliance:r.get("alliance_id"),name:r.get("name"),inviter:r.get("nickname"),seconds_left:r.get::<i32,_>("seconds") as u32});
    }
    tx.commit().await.map_err(database)?;
    Ok(reply)
}
pub enum Failure {
    Rejected(String),
    // An uncertain commit must never be overwritten with the old inventory or wallet.
    UncertainInventory,
}
impl From<String> for Failure {
    fn from(s: String) -> Self {
        Self::Rejected(s)
    }
}
impl From<&str> for Failure {
    fn from(s: &str) -> Self {
        Self::Rejected(s.into())
    }
}
pub struct Change {
    pub message: String,
    pub update: Option<(Character, Actor)>,
}
/// Caller holds the simulation lock and replaces the actor only after this transaction commits.
pub async fn apply(
    conn: &mut PgConnection,
    actor: Actor,
    command: Command,
) -> Result<Change, Failure> {
    let mut tx = conn.begin().await.map_err(database)?;
    let change = match change(&mut tx, actor, command).await {
        Ok(change) => change,
        Err(e) => {
            // Roll back now: a dropped transaction would keep the advisory and row locks
            // until this session's connection runs its next query.
            tx.rollback().await.map_err(database)?;
            return Err(e);
        }
    };
    if let Err(e) = tx.commit().await {
        if change.update.is_some() {
            return Err(Failure::UncertainInventory);
        }
        return Err(database(e).into());
    }
    Ok(change)
}
async fn change(
    tx: &mut PgConnection,
    mut actor: Actor,
    command: Command,
) -> Result<Change, Failure> {
    let owner = actor.id;
    sqlx::query("SET LOCAL lock_timeout = '3s'")
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(LOCK)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    sqlx::query("DELETE FROM alliance_invitations WHERE expires_at<=now()")
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    let own = seats(&mut *tx, owner).await?;
    let mut update = None;
    let message = match command {
        Command::Refresh => "Состав обновлён",
        Command::Create { name, kind } => {
            let name = alliance::validate_name(&name)?;
            if own.iter().any(|seat| seat.1 == kind.code()) {
                return Err("Вы уже состоите в организации этого типа".into());
            }
            if actor.hp <= 0. {
                return Err("Сначала возродитесь".into());
            }
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO alliances(name,leader_id,kind) VALUES($1,$2,$3) RETURNING id",
            )
            .bind(name)
            .bind(owner)
            .bind(kind.code())
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            sqlx::query(
                "INSERT INTO alliance_members(alliance_id,character_id,kind,identity) \
                 VALUES($1,$2,$3,COALESCE((SELECT (stats->>'identity')::float8 FROM characters WHERE id=$2),1))",
            )
            .bind(id)
            .bind(owner)
            .bind(kind.code())
            .execute(&mut *tx)
            .await
            .map_err(database)?;
            sqlx::query("DELETE FROM alliance_invitations WHERE recipient_id=$1")
                .bind(owner)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            let price = mortown_protocol::items::format_coins(alliance::CREATE_COST);
            let character = crate::inventory::pay(
                &mut *tx,
                &mut actor,
                alliance::CREATE_COST,
                &format!("Создан альянс · −{price}"),
            )
            .await
            .map_err(|e| {
                if e == "Недостаточно монет" {
                    format!("Создание альянса стоит {price}")
                } else {
                    e
                }
            })?;
            update = Some((character, actor));
            "Альянс создан"
        }
        Command::Invite { alliance, nickname } => {
            let (_, _, r) = *own
                .iter()
                .find(|seat| seat.0 == alliance)
                .ok_or("Вы больше не в этой организации")?;
            if !r.can_invite() {
                return Err("Приглашать могут лидер и офицеры".into());
            }
            if !(2..=16).contains(&nickname.trim().chars().count()) {
                return Err("Укажите имя персонажа".into());
            }
            let target: i64 =
                sqlx::query_scalar("SELECT id FROM characters WHERE lower(nickname)=lower($1)")
                    .bind(nickname.trim())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(database)?
                    .ok_or("Персонаж не найден")?;
            let kind: i16 = sqlx::query_scalar("SELECT kind FROM alliances WHERE id=$1")
                .bind(alliance)
                .fetch_one(&mut *tx)
                .await
                .map_err(database)?;
            if target == owner
                || seats(&mut *tx, target)
                    .await?
                    .iter()
                    .any(|seat| seat.1 == kind)
            {
                return Err("Персонаж уже состоит в организации этого типа".into());
            }
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM chat_ignores WHERE owner_id=$1 AND blocked_id=$2)",
            )
            .bind(target)
            .bind(owner)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            if blocked {
                return Err("Приглашение недоступно".into());
            }
            let count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM alliance_members WHERE alliance_id=$1)+(SELECT count(*) FROM alliance_invitations WHERE alliance_id=$1)").bind(alliance).fetch_one(&mut *tx).await.map_err(database)?;
            if count >= alliance::LIMIT as i64 {
                return Err("В альянсе 20 мест, включая действующие приглашения".into());
            }
            let pending: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM alliance_invitations WHERE recipient_id=$1 AND kind=$2)",
            )
            .bind(target)
            .bind(kind)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            if pending {
                return Err("У персонажа уже есть действующее приглашение".into());
            }
            sqlx::query("INSERT INTO alliance_invitations(recipient_id,alliance_id,inviter_id,expires_at,kind) VALUES($1,$2,$3,now()+interval '120 seconds',$4)").bind(target).bind(alliance).bind(owner).bind(kind).execute(&mut *tx).await.map_err(database)?;
            "Приглашение отправлено на две минуты"
        }
        Command::Accept { invitation } => {
            let i=sqlx::query("SELECT alliance_id,inviter_id,kind FROM alliance_invitations WHERE id=$1 AND recipient_id=$2 AND expires_at>now()").bind(invitation).bind(owner).fetch_optional(&mut *tx).await.map_err(database)?.ok_or("Приглашение истекло или отменено")?;
            let id: i64 = i.get("alliance_id");
            let inviter: i64 = i.get("inviter_id");
            let kind: i16 = i.get("kind");
            if own.iter().any(|seat| seat.1 == kind) {
                return Err("Вы уже состоите в организации этого типа".into());
            }
            if !seats(&mut *tx, inviter)
                .await?
                .iter()
                .any(|seat| seat.0 == id && seat.2.can_invite())
            {
                return Err("Пригласивший больше не управляет этим альянсом".into());
            }
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM alliance_members WHERE alliance_id=$1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(database)?;
            if count >= alliance::LIMIT as i64 {
                return Err("Альянс заполнен".into());
            }
            sqlx::query(
                "INSERT INTO alliance_members(alliance_id,character_id,kind,identity) \
                 VALUES($1,$2,$3,COALESCE((SELECT (stats->>'identity')::float8 FROM characters WHERE id=$2),1))",
            )
            .bind(id)
            .bind(owner)
            .bind(kind)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
            sqlx::query("DELETE FROM alliance_invitations WHERE recipient_id=$1 AND kind=$2")
                .bind(owner)
                .bind(kind)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            "Вы вступили в организацию"
        }
        Command::Decline { invitation } => {
            let result =
                sqlx::query("DELETE FROM alliance_invitations WHERE id=$1 AND recipient_id=$2")
                    .bind(invitation)
                    .bind(owner)
                    .execute(&mut *tx)
                    .await
                    .map_err(database)?;
            if result.rows_affected() == 0 {
                return Err("Приглашение уже недействительно".into());
            }
            "Приглашение отклонено"
        }
        Command::Leave { alliance } => {
            let (_, _, r) = *own
                .iter()
                .find(|seat| seat.0 == alliance)
                .ok_or("Вы больше не в этой организации")?;
            if r == Role::Leader {
                return Err("Сначала передайте лидерство или распустите организацию".into());
            }
            remove_member(&mut *tx, owner, alliance).await?;
            "Вы вышли из альянса"
        }
        Command::Kick { alliance, player } => {
            let (_, _, r) = *own
                .iter()
                .find(|seat| seat.0 == alliance)
                .ok_or("Вы больше не в этой организации")?;
            let (_, _, target) = *seats(&mut *tx, player)
                .await?
                .iter()
                .find(|seat| seat.0 == alliance)
                .ok_or("Участник уже покинул организацию")?;
            if player == owner || !r.can_kick(target) {
                return Err("Недостаточно прав для исключения этого участника".into());
            }
            remove_member(&mut *tx, player, alliance).await?;
            "Участник исключён"
        }
        Command::SetOfficer {
            alliance,
            player,
            officer,
        } => {
            if own
                .iter()
                .find(|seat| seat.0 == alliance)
                .is_none_or(|seat| seat.2 != Role::Leader)
                || player == owner
            {
                return Err("Назначать офицеров может только лидер".into());
            }
            if seats(&mut *tx, player)
                .await?
                .iter()
                .all(|seat| seat.0 != alliance)
            {
                return Err("Участник уже покинул организацию".into());
            }
            sqlx::query(
                "UPDATE alliance_members SET officer=$2 WHERE character_id=$1 AND alliance_id=$3",
            )
            .bind(player)
            .bind(officer)
            .bind(alliance)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
            if !officer {
                sqlx::query("DELETE FROM alliance_invitations WHERE inviter_id=$1")
                    .bind(player)
                    .execute(&mut *tx)
                    .await
                    .map_err(database)?;
            }
            if officer {
                "Офицер назначен"
            } else {
                "Участник понижен"
            }
        }
        Command::Transfer { alliance, player } => {
            if own
                .iter()
                .find(|seat| seat.0 == alliance)
                .is_none_or(|seat| seat.2 != Role::Leader)
                || player == owner
            {
                return Err("Лидерство передаёт действующий лидер другому участнику".into());
            }
            if seats(&mut *tx, player)
                .await?
                .iter()
                .all(|seat| seat.0 != alliance)
            {
                return Err("Участник уже покинул организацию".into());
            }
            sqlx::query("UPDATE alliances SET leader_id=$2 WHERE id=$1")
                .bind(alliance)
                .bind(player)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            sqlx::query("UPDATE alliance_members SET officer=(character_id=$1) WHERE alliance_id=$3 AND character_id IN ($1,$2)").bind(owner).bind(player).bind(alliance).execute(&mut *tx).await.map_err(database)?;
            "Лидерство передано; вы стали офицером"
        }
        Command::Dissolve { alliance } => {
            if own
                .iter()
                .find(|seat| seat.0 == alliance)
                .is_none_or(|seat| seat.2 != Role::Leader)
            {
                return Err("Распустить организацию может только лидер".into());
            }
            if crate::legacy::alliance_owns(&mut *tx, alliance)
                .await
                .map_err(database)?
            {
                return Err(crate::legacy::KEPT.into());
            }
            sqlx::query("DELETE FROM alliances WHERE id=$1")
                .bind(alliance)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            "Альянс распущен; стоимость создания не возвращается"
        }
    };
    Ok(Change {
        message: message.into(),
        update,
    })
}
async fn remove_member(conn: &mut PgConnection, owner: i64, alliance: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM alliance_invitations WHERE inviter_id=$1 AND alliance_id=$2")
        .bind(owner)
        .bind(alliance)
        .execute(&mut *conn)
        .await
        .map_err(database)?;
    sqlx::query("DELETE FROM alliance_members WHERE character_id=$1 AND alliance_id=$2")
        .bind(owner)
        .bind(alliance)
        .execute(&mut *conn)
        .await
        .map_err(database)?;
    Ok(())
}
