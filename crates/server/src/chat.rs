use mortown_protocol::chat::{self, Channel, Command, Message, Reply};
use sqlx::{Connection, Row};
use std::collections::{HashSet, VecDeque};
#[derive(Clone)]
pub struct Delivery {
    pub message: Message,
    pub recipients: Vec<i64>,
}
#[derive(Default)]
pub struct Session {
    pub ignored: HashSet<i64>,
    pub received: VecDeque<i64>,
    pub ignore_all: bool,
    pub silence: bool,
    pub selected: Option<mortown_protocol::admin::Target>,
    pub confirmation: Option<crate::console::Confirmation>,
}
fn unavailable(_: sqlx::Error) -> String {
    "Чат временно недоступен".into()
}
impl Session {
    pub fn preferences(&self) -> Reply {
        Reply::Preferences {
            ignored: self.ignored.iter().copied().collect(),
            ignore_all: self.ignore_all,
            silence: self.silence,
        }
    }

    pub async fn load(conn: &mut sqlx::PgConnection, owner: i64) -> Result<Self, String> {
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT blocked_id FROM chat_ignores WHERE owner_id=$1")
                .bind(owner)
                .fetch_all(&mut *conn)
                .await
                .map_err(unavailable)?;
        let prefs =
            sqlx::query("SELECT ignore_all,silence FROM chat_preferences WHERE character_id=$1")
                .bind(owner)
                .fetch_optional(conn)
                .await
                .map_err(unavailable)?;
        Ok(Self {
            ignored: ids.into_iter().collect(),
            received: VecDeque::new(),
            ignore_all: prefs.as_ref().is_some_and(|r| r.get("ignore_all")),
            silence: prefs.as_ref().is_some_and(|r| r.get("silence")),
            selected: None,
            confirmation: None,
        })
    }
    pub fn delivered(&mut self, id: i64) {
        self.received.push_back(id);
        if self.received.len() > 100 {
            self.received.pop_front();
        }
    }
}
pub async fn handle(
    server: &crate::Server,
    conn: &mut sqlx::PgConnection,
    owner: i64,
    state: &mut Session,
    command: Command,
) -> Option<Reply> {
    let result:Result<Reply,String>=async {
        match command {
            Command::Admin{text,target}=>{state.selected=target;crate::console::execute(server,conn,owner,state,&text).await},
            Command::Execute{text}=>{state.selected=None;crate::console::execute(server,conn,owner,state,&text).await},
            Command::Confirm{token}=>crate::console::confirm(server,conn,owner,state,&token).await,
            command @ (Command::Send{..}|Command::Whisper{..})=>{
                let (channel,text,recipient)=match command {
                    Command::Send{channel,text}=>{
                        if channel==Channel::Private{return Err("Укажите адресата: \"ник текст".into());}
                        // The server owns parsing even for older/custom clients.
                        if chat::commands::parse(&text)?.is_some(){return crate::console::execute(server,conn,owner,state,&text).await;}
                        (channel,text,None)
                    },
                    Command::Whisper{nickname,text}=>{
                        let (id,name,_,_)=crate::console::target(conn,&nickname).await?;
                        if id==owner{return Err("Нельзя писать самому себе".into());}
                        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM chat_ignores WHERE owner_id=$1 AND blocked_id=$2) OR EXISTS(SELECT 1 FROM chat_preferences WHERE character_id=$1 AND (silence OR ignore_all))").bind(id).bind(owner).fetch_one(&mut *conn).await.map_err(unavailable)?;
                        if blocked{return Err("Личные сообщения этому персонажу недоступны".into());}
                        (Channel::Private,text,Some((id,name)))
                    }, _=>unreachable!()
                };
                let body=chat::validate(&text)?;
                let (nickname,mut recipients,party)={
                    let sim=server.simulation.lock().await;
                    let actor=sim.actor(owner).ok_or("Персонаж не в мире")?;
                    let world=sim.terrain();
                    let from=[actor.position[0],actor.position[2]];
                    let hear=|other:&mortown_protocol::Actor| {
                        let at=[other.position[0],other.position[2]];
                        other.id==owner || (mortown_world::distance(from,at)<=chat::LOCAL_RADIUS && world.segment_clear(from,at))
                    };
                    let party=if channel==Channel::Group{Some(sim.parties.membership(owner).ok_or("Вы не состоите в группе")?)}else{None};
                    let recipients=if let Some((id,_))=&recipient {
                        let other=sim.actor(*id).ok_or("Персонаж сейчас не в игре")?;
                        if !hear(&other){return Err("Слишком далеко или путь закрыт. Для письма нужна птица".into());}
                        vec![owner,*id]
                    }else if channel==Channel::Group{
                        sim.parties.members(owner).unwrap().into_iter().filter(|id| sim.actor(*id).is_some_and(|other| hear(&other))).collect()
                    }else{
                        sim.snapshot().into_iter().filter(|other| hear(other)).map(|other| other.id).collect()
                    };
                    (mortown_protocol::society::public_label(&actor.presented, actor.face_hidden, None),recipients,party)
                };
                let mut tx=conn.begin().await.map_err(unavailable)?;
                let alliance=if channel==Channel::Alliance {
                    sqlx::query("SET LOCAL lock_timeout = '3s'").execute(&mut *tx).await.map_err(unavailable)?;
                    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(crate::alliance::LOCK).execute(&mut *tx).await.map_err(unavailable)?;
                    let (id,members)=crate::alliance::audience(&mut tx,owner).await?;
                    recipients.retain(|member| members.contains(member));
                    if !recipients.contains(&owner){recipients.push(owner);}
                    Some(id)
                } else {None};
                let account:i64=sqlx::query_scalar("SELECT account_id FROM characters WHERE id=$1").bind(owner).fetch_one(&mut *tx).await.map_err(unavailable)?;
                sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR NO KEY UPDATE").bind(account).fetch_one(&mut *tx).await.map_err(unavailable)?;
                let muted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM chat_mutes WHERE character_id=$1 AND until_at>now()) OR EXISTS(SELECT 1 FROM global_mutes WHERE character_id=$1 AND until_at>now())").bind(owner).fetch_one(&mut *tx).await.map_err(unavailable)?;
                if muted{return Err("Отправка сообщений ограничена модератором".into());}
                let limited:bool=sqlx::query_scalar("SELECT count(*)>=5 OR coalesce(bool_or(m.sent_at>now()-interval '1 second'),false) FROM chat_messages m JOIN characters c ON c.id=m.sender_id WHERE c.account_id=$1 AND m.sent_at>now()-interval '10 seconds'").bind(account).fetch_one(&mut *tx).await.map_err(unavailable)?;
                if limited{return Err("Не чаще одного сообщения в секунду и пяти за 10 секунд".into());}
                let id=sqlx::query_scalar("INSERT INTO chat_messages(sender_id,channel,body,alliance_id,recipient_id) VALUES($1,$2,$3,$4,$5) RETURNING id").bind(owner).bind(match channel{Channel::General=>"general",Channel::Local=>"local",Channel::Group=>"group",Channel::Alliance=>"alliance",Channel::Private=>"private"}).bind(body).bind(alliance).bind(recipient.as_ref().map(|r|r.0)).fetch_one(&mut *tx).await.map_err(unavailable)?;
                tx.commit().await.map_err(unavailable)?;
                let _=server.chat.send(Delivery{message:Message{alliance,party,id,sender:owner,nickname,channel,text:body.into(),recipient},recipients});
                Ok(Reply::Acknowledged)
            },
            Command::Ignore{nickname,blocked}=>{
                if !(2..=16).contains(&nickname.trim().chars().count()){return Err("Укажите имя персонажа".into());}
                let id:i64=sqlx::query_scalar("SELECT id FROM characters WHERE lower(nickname)=lower($1)").bind(nickname.trim()).fetch_optional(&mut *conn).await.map_err(unavailable)?.ok_or("Персонаж не найден")?;
                if id==owner{return Err("Нельзя игнорировать себя".into());}
                if blocked {
                    if state.ignored.len()>=100&&!state.ignored.contains(&id){return Err("В списке игнора уже 100 персонажей".into());}
                    sqlx::query("INSERT INTO chat_ignores(owner_id,blocked_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(owner).bind(id).execute(conn).await.map_err(unavailable)?;
                    state.ignored.insert(id);
                }else{
                    sqlx::query("DELETE FROM chat_ignores WHERE owner_id=$1 AND blocked_id=$2").bind(owner).bind(id).execute(conn).await.map_err(unavailable)?;
                    state.ignored.remove(&id);
                }
                Ok(crate::console::notice(if blocked{"Новые сообщения персонажа скрыты"}else{"Сообщения персонажа снова доступны"}))
            },
            Command::Report{id}=>{
                if !state.received.contains(&id){return Err("Можно пожаловаться на одно из последних 100 полученных сообщений".into());}
                let row=sqlx::query("SELECT sender_id FROM chat_messages WHERE id=$1").bind(id).fetch_one(&mut *conn).await.map_err(unavailable)?;
                if row.get::<i64,_>("sender_id")==owner{return Err("Это ваше сообщение".into());}
                let count:i64=sqlx::query_scalar("SELECT count(*) FROM chat_reports WHERE reporter_id=$1 AND reported_at>now()-interval '24 hours'").bind(owner).fetch_one(&mut *conn).await.map_err(unavailable)?;
                if count>=10{return Err("Лимит жалоб — 10 за сутки".into());}
                sqlx::query("INSERT INTO chat_reports(reporter_id,message_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(owner).bind(id).execute(conn).await.map_err(unavailable)?;
                Ok(crate::console::notice("Жалоба сохранена для рассмотрения модератором"))
            }
        }
    }.await;
    match result {
        Ok(reply) => Some(reply),
        Err(message) => Some(Reply::Status { message }),
    }
}
