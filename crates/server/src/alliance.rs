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
async fn membership(conn: &mut PgConnection, owner: i64) -> Result<Option<(i64, Role)>, String> {
    Ok(sqlx::query("SELECT a.id,a.leader_id,m.officer FROM alliance_members m JOIN alliances a ON a.id=m.alliance_id WHERE m.character_id=$1")
        .bind(owner).fetch_optional(conn).await.map_err(database)?
        .map(|r| (r.get("id"),role(r.get("leader_id"),owner,r.get("officer")))))
}
pub async fn audience(conn: &mut PgConnection, owner: i64) -> Result<(i64, Vec<i64>), String> {
    let (id, _) = membership(conn, owner)
        .await?
        .ok_or("Вы не состоите в альянсе")?;
    let members =
        sqlx::query_scalar("SELECT character_id FROM alliance_members WHERE alliance_id=$1")
            .bind(id)
            .fetch_all(conn)
            .await
            .map_err(database)?;
    Ok((id, members))
}
pub async fn view(conn: &mut PgConnection, owner: i64, actors: &[Actor]) -> Result<Reply, String> {
    let mut tx = conn.begin().await.map_err(database)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    let mut reply = Reply::default();
    if let Some((id,_))=membership(&mut tx,owner).await? {
        let a=sqlx::query("SELECT name,leader_id FROM alliances WHERE id=$1").bind(id).fetch_one(&mut *tx).await.map_err(database)?;
        let leader:i64=a.get("leader_id");
        let rows=sqlx::query("SELECT c.id,c.nickname,coalesce((c.stats->>'level')::double precision,1) AS level,m.officer FROM alliance_members m JOIN characters c ON c.id=m.character_id WHERE m.alliance_id=$1 ORDER BY (c.id=$2) DESC,m.officer DESC,m.joined_at,c.id")
            .bind(id).bind(leader).fetch_all(&mut *tx).await.map_err(database)?;
        reply.alliance=Some(View {id,name:a.get("name"),members:rows.iter().map(|r| {
            let id=r.get("id"); let actor=actors.iter().find(|a|a.id==id);
            Member{id,nickname:r.get("nickname"),level:actor.map_or(r.get::<f64,_>("level") as u32,|a|a.progress.level),role:role(leader,id,r.get("officer")),online:actor.is_some()}
        }).collect(),storage:crate::alliance_storage::read(&mut tx,id).await?});
    } else if let Some(r)=sqlx::query("SELECT i.id,i.alliance_id,a.name,c.nickname,greatest(0,ceil(extract(epoch FROM i.expires_at-now())))::int AS seconds FROM alliance_invitations i JOIN alliances a ON a.id=i.alliance_id JOIN characters c ON c.id=i.inviter_id JOIN alliance_members m ON m.character_id=i.inviter_id AND m.alliance_id=a.id WHERE i.recipient_id=$1 AND i.expires_at>now() AND (a.leader_id=i.inviter_id OR m.officer)")
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
    mut actor: Actor,
    command: Command,
) -> Result<Change, Failure> {
    let owner = actor.id;
    let mut tx = conn.begin().await.map_err(database)?;
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
    let own = membership(&mut tx, owner).await?;
    let mut update = None;
    let message = match command {
        Command::Refresh => "Состав обновлён",
        Command::Create { name } => {
            let name = alliance::validate_name(&name)?;
            if own.is_some() {
                return Err("Вы уже состоите в альянсе".into());
            }
            if actor.hp <= 0. {
                return Err("Сначала возродитесь".into());
            }
            if !actor.progress.coins.is_finite() || actor.progress.coins < alliance::CREATE_COST {
                return Err("Создание альянса стоит 1000 монет".into());
            }
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO alliances(name,leader_id) VALUES($1,$2) RETURNING id",
            )
            .bind(name)
            .bind(owner)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            sqlx::query("INSERT INTO alliance_members(alliance_id,character_id) VALUES($1,$2)")
                .bind(id)
                .bind(owner)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            sqlx::query("DELETE FROM alliance_invitations WHERE recipient_id=$1")
                .bind(owner)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            actor.progress.coins -= alliance::CREATE_COST;
            actor.reward_status = "Создан альянс · −1000 монет".into();
            crate::store::save(&mut tx, &actor).await?;
            let row = sqlx::query("SELECT * FROM characters WHERE id=$1")
                .bind(owner)
                .fetch_one(&mut *tx)
                .await
                .map_err(database)?;
            update = Some((crate::store::character(&row)?, actor));
            "Альянс создан · −1000 монет"
        }
        Command::Invite { alliance, nickname } => {
            let (_, r) = own
                .filter(|(id, _)| *id == alliance)
                .ok_or("Вы больше не в этом альянсе")?;
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
            if target == owner || membership(&mut tx, target).await?.is_some() {
                return Err("Персонаж уже состоит в альянсе".into());
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
                "SELECT EXISTS(SELECT 1 FROM alliance_invitations WHERE recipient_id=$1)",
            )
            .bind(target)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            if pending {
                return Err("У персонажа уже есть действующее приглашение".into());
            }
            sqlx::query("INSERT INTO alliance_invitations(recipient_id,alliance_id,inviter_id,expires_at) VALUES($1,$2,$3,now()+interval '120 seconds')").bind(target).bind(alliance).bind(owner).execute(&mut *tx).await.map_err(database)?;
            "Приглашение отправлено на две минуты"
        }
        Command::Accept { invitation } => {
            if own.is_some() {
                return Err("Вы уже состоите в альянсе".into());
            }
            let i=sqlx::query("SELECT alliance_id,inviter_id FROM alliance_invitations WHERE id=$1 AND recipient_id=$2 AND expires_at>now()").bind(invitation).bind(owner).fetch_optional(&mut *tx).await.map_err(database)?.ok_or("Приглашение истекло или отменено")?;
            let id: i64 = i.get("alliance_id");
            let inviter: i64 = i.get("inviter_id");
            if !membership(&mut tx, inviter)
                .await?
                .is_some_and(|(a, r)| a == id && r.can_invite())
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
            sqlx::query("INSERT INTO alliance_members(alliance_id,character_id) VALUES($1,$2)")
                .bind(id)
                .bind(owner)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            sqlx::query("DELETE FROM alliance_invitations WHERE recipient_id=$1")
                .bind(owner)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            "Вы вступили в альянс"
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
            let (_, r) = own
                .filter(|(id, _)| *id == alliance)
                .ok_or("Вы больше не в этом альянсе")?;
            if r == Role::Leader {
                return Err("Сначала передайте лидерство или распустите альянс".into());
            }
            remove_member(&mut tx, owner).await?;
            "Вы вышли из альянса"
        }
        Command::Kick { alliance, player } => {
            let (_, r) = own
                .filter(|(id, _)| *id == alliance)
                .ok_or("Вы больше не в этом альянсе")?;
            let (_, target) = membership(&mut tx, player)
                .await?
                .filter(|(id, _)| *id == alliance)
                .ok_or("Участник уже покинул альянс")?;
            if player == owner || !r.can_kick(target) {
                return Err("Недостаточно прав для исключения этого участника".into());
            }
            remove_member(&mut tx, player).await?;
            "Участник исключён"
        }
        Command::SetOfficer {
            alliance,
            player,
            officer,
        } => {
            if own != Some((alliance, Role::Leader)) || player == owner {
                return Err("Назначать офицеров может только лидер".into());
            }
            if membership(&mut tx, player)
                .await?
                .is_none_or(|(id, _)| id != alliance)
            {
                return Err("Участник уже покинул альянс".into());
            }
            sqlx::query("UPDATE alliance_members SET officer=$2 WHERE character_id=$1")
                .bind(player)
                .bind(officer)
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
            if own != Some((alliance, Role::Leader)) || player == owner {
                return Err("Лидерство передаёт действующий лидер другому участнику".into());
            }
            if membership(&mut tx, player)
                .await?
                .is_none_or(|(id, _)| id != alliance)
            {
                return Err("Участник уже покинул альянс".into());
            }
            sqlx::query("UPDATE alliances SET leader_id=$2 WHERE id=$1")
                .bind(alliance)
                .bind(player)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            sqlx::query("UPDATE alliance_members SET officer=(character_id=$1) WHERE character_id IN ($1,$2)").bind(owner).bind(player).execute(&mut *tx).await.map_err(database)?;
            "Лидерство передано; вы стали офицером"
        }
        Command::Deposit {
            alliance,
            revision,
            item,
        } => {
            own.filter(|(id, _)| *id == alliance)
                .ok_or("Вы больше не в этом альянсе")?;
            let change =
                crate::alliance_storage::deposit(&mut tx, actor, alliance, revision, item).await?;
            update = Some((change.character, change.actor));
            "Предметы переданы на склад альянса"
        }
        Command::Withdraw {
            alliance,
            revision,
            entry,
            quantity,
        } => {
            let (_, role) = own
                .filter(|(id, _)| *id == alliance)
                .ok_or("Вы больше не в этом альянсе")?;
            if role == Role::Member {
                return Err("Забирать со склада могут лидер и офицеры".into());
            }
            let change = crate::alliance_storage::withdraw(
                &mut tx, actor, alliance, revision, entry, quantity,
            )
            .await?;
            update = Some((change.character, change.actor));
            "Предметы получены со склада альянса"
        }
        Command::Dissolve { alliance } => {
            if own != Some((alliance, Role::Leader)) {
                return Err("Распустить альянс может только лидер".into());
            }
            let occupied: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM alliance_storage WHERE alliance_id=$1)",
            )
            .bind(alliance)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
            if occupied {
                return Err("Сначала освободите склад альянса".into());
            }
            sqlx::query("DELETE FROM alliances WHERE id=$1")
                .bind(alliance)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
            "Альянс распущен; стоимость создания не возвращается"
        }
    };
    if let Err(e) = tx.commit().await {
        if update.is_some() {
            return Err(Failure::UncertainInventory);
        }
        return Err(database(e).into());
    }
    Ok(Change {
        message: message.into(),
        update,
    })
}
async fn remove_member(conn: &mut PgConnection, owner: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM alliance_invitations WHERE inviter_id=$1")
        .bind(owner)
        .execute(&mut *conn)
        .await
        .map_err(database)?;
    sqlx::query("DELETE FROM alliance_members WHERE character_id=$1")
        .bind(owner)
        .execute(conn)
        .await
        .map_err(database)?;
    Ok(())
}
