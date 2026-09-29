use crate::{mail, simulation::Simulation};
use mortown_protocol::{
    Actor, Character,
    exchange::{Command, Offer, Reply, View},
};
use sqlx::Connection;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Exchanges {
    next: u64,
    sessions: HashMap<u64, Session>,
    events: HashMap<i64, Reply>,
    updated: HashSet<i64>,
}
struct Session {
    id: u64,
    revision: u64,
    players: [i64; 2],
    names: [String; 2],
    accepted: bool,
    offers: [Offer; 2],
    inventory: [Option<Character>; 2],
    created: Instant,
}
impl Exchanges {
    fn publish(&mut self, s: &Session, message: &str) {
        for i in 0..2 {
            self.events.insert(
                s.players[i],
                Reply {
                    view: Some(View {
                        session: s.id,
                        revision: s.revision,
                        peer: s.names[1 - i].clone(),
                        invited: i == 1,
                        accepted: s.accepted,
                        own: s.offers[i].clone(),
                        other: s.offers[1 - i].clone(),
                    }),
                    message: message.into(),
                    update: None,
                },
            );
        }
    }
    pub fn cancel(&mut self, player: i64, message: &str) {
        if let Some(id) = self
            .sessions
            .values()
            .find(|s| s.players.contains(&player))
            .map(|s| s.id)
        {
            let s = self.sessions.remove(&id).unwrap();
            for id in s.players {
                self.events.insert(
                    id,
                    Reply {
                        view: None,
                        message: message.into(),
                        update: None,
                    },
                );
            }
        }
    }
    pub fn leave(&mut self, player: i64) {
        self.cancel(player, "Обмен отменён: участник вышел из мира");
        self.events.remove(&player);
        self.updated.remove(&player);
    }
}
fn nearby(sim: &Simulation, players: [i64; 2]) -> bool {
    let Some(a) = sim.actor(players[0]) else {
        return false;
    };
    let Some(b) = sim.actor(players[1]) else {
        return false;
    };
    let from = [a.position[0], a.position[2]];
    let to = [b.position[0], b.position[2]];
    a.hp > 0.
        && b.hp > 0.
        && mortown_world::distance(from, to) <= 6.
        && sim.terrain().segment_clear(from, to)
}
pub async fn poll(
    sim: &mut Simulation,
    conn: &mut sqlx::PgConnection,
    player: i64,
) -> Result<Option<Reply>, String> {
    let expired: Vec<_> = sim
        .exchanges
        .sessions
        .values()
        .filter(|s| s.created.elapsed() > Duration::from_secs(120) || !nearby(sim, s.players))
        .map(|s| s.players[0])
        .collect();
    for id in expired {
        sim.exchanges.cancel(
            id,
            "Обмен отменён: время истекло, участник отошёл или погиб",
        );
    }
    let mut reply = sim.exchanges.events.remove(&player);
    if sim.exchanges.updated.contains(&player) {
        // Read the latest character under the simulation lock, not a stale completion payload.
        let actor = sim.actor(player).ok_or("Персонаж вышел из мира")?;
        let mut tx = conn
            .begin()
            .await
            .map_err(|_| "Не удалось открыть сохранение обмена")?;
        let c = mail::inventory(&mut tx, &actor).await?;
        let update = mail::persist(&mut tx, actor, c, "Обмен завершён").await?;
        tx.commit()
            .await
            .map_err(|_| "Не удалось сохранить состояние обмена")?;
        sim.exchanges.updated.remove(&player);
        reply
            .get_or_insert(Reply {
                view: None,
                message: "Обмен завершён".into(),
                update: None,
            })
            .update = Some(update);
    }
    Ok(reply)
}
async fn commit(
    sim: &mut Simulation,
    conn: &mut sqlx::PgConnection,
    s: &Session,
) -> Result<(), String> {
    if s.offers.iter().all(|o| o.item.is_none() && o.coins == 0) {
        return Err("Обмен отменён: обе стороны пусты".into());
    }
    let mut actors: [Actor; 2] = [
        sim.actor(s.players[0]).ok_or("Участник вышел")?,
        sim.actor(s.players[1]).ok_or("Участник вышел")?,
    ];
    let mut tx = conn.begin().await.map_err(|_| "База данных недоступна")?;
    let mut chars: Vec<Option<Character>> = vec![None, None];
    let order = if s.players[0] < s.players[1] {
        [0, 1]
    } else {
        [1, 0]
    };
    for i in order {
        chars[i] = Some(mail::inventory(&mut tx, &actors[i]).await?);
    }
    let mut chars = [chars[0].take().unwrap(), chars[1].take().unwrap()];
    let mut kinds = [None, None];
    for i in 0..2 {
        if let Some(a) = s.offers[i].item {
            let old = s.inventory[i].as_ref().ok_or("Выберите вещь заново")?;
            if old.inventory != chars[i].inventory
                || old.loot != chars[i].loot
                || mortown_protocol::layout::bag(old) != mortown_protocol::layout::bag(&chars[i])
            {
                return Err("Обмен отменён: содержимое рюкзака изменилось".into());
            }
            kinds[i] = Some(mortown_protocol::transfer::take(&mut chars[i], &a)?);
        }
        let balance = actors[i].progress.coins - f64::from(s.offers[i].coins);
        if !balance.is_finite() || balance < 0. {
            return Err("Обмен отменён: недостаточно монет".into());
        }
        let result = balance + f64::from(s.offers[1 - i].coins);
        if result > f64::from(u32::MAX) {
            return Err("Баланс монет получателя переполнен".into());
        }
        actors[i].progress.coins = result;
        actors[i].progress.write_stats(&mut chars[i].stats);
    }
    for i in 0..2 {
        if let Some(kind) = kinds[i] {
            mortown_protocol::transfer::give(
                &mut chars[1 - i],
                kind,
                s.offers[i].item.unwrap().quantity,
            )?;
        }
    }
    let [a, b] = chars;
    let first = mail::persist(&mut tx, actors[0].clone(), a, "Обмен завершён").await?;
    let second = mail::persist(&mut tx, actors[1].clone(), b, "Обмен завершён").await?;
    tx.commit()
        .await
        .map_err(|_| "Не удалось подтвердить сохранение обмена; переподключитесь")?;
    for update in [first, second] {
        sim.replace_actor(update.actor);
        sim.refresh_character(&update.character);
        sim.exchanges.updated.insert(update.character.id);
        sim.exchanges.events.insert(
            update.character.id,
            Reply {
                view: None,
                message: "Обмен завершён".into(),
                update: None,
            },
        );
    }
    Ok(())
}
pub async fn handle(
    sim: &mut Simulation,
    conn: &mut sqlx::PgConnection,
    player: i64,
    command: Command,
) {
    if let Err(message) = apply(sim, conn, player, command).await {
        let view = sim
            .exchanges
            .events
            .get(&player)
            .and_then(|r| r.view.clone())
            .or_else(|| {
                let s = sim
                    .exchanges
                    .sessions
                    .values()
                    .find(|s| s.players.contains(&player))?;
                let i = usize::from(s.players[1] == player);
                Some(View {
                    session: s.id,
                    revision: s.revision,
                    peer: s.names[1 - i].clone(),
                    invited: i == 1,
                    accepted: s.accepted,
                    own: s.offers[i].clone(),
                    other: s.offers[1 - i].clone(),
                })
            });
        sim.exchanges.events.insert(
            player,
            Reply {
                view,
                message,
                update: None,
            },
        );
    }
}
async fn apply(
    sim: &mut Simulation,
    conn: &mut sqlx::PgConnection,
    player: i64,
    command: Command,
) -> Result<(), String> {
    if let Command::Invite { player: peer } = command {
        if player == peer || !nearby(sim, [player, peer]) {
            return Err("Выберите другого живого игрока не дальше шести метров".into());
        }
        if sim
            .exchanges
            .sessions
            .values()
            .any(|s| s.players.contains(&player) || s.players.contains(&peer))
        {
            return Err("Один из участников уже занят обменом".into());
        }
        sim.exchanges.next += 1;
        let s = Session {
            id: sim.exchanges.next,
            revision: 0,
            players: [player, peer],
            names: [
                sim.actor(player).unwrap().nickname,
                sim.actor(peer).unwrap().nickname,
            ],
            accepted: false,
            offers: Default::default(),
            inventory: Default::default(),
            created: Instant::now(),
        };
        sim.exchanges.publish(&s, "Приглашение к обмену");
        sim.exchanges.sessions.insert(s.id, s);
        return Ok(());
    }
    let (session, revision) = match &command {
        Command::Accept { session, revision }
        | Command::Offer {
            session, revision, ..
        }
        | Command::Confirm { session, revision } => (*session, Some(*revision)),
        Command::Cancel { session } => (*session, None),
        _ => unreachable!(),
    };
    let s = sim
        .exchanges
        .sessions
        .get(&session)
        .filter(|s| s.players.contains(&player))
        .ok_or("Этот обмен уже закрыт")?;
    if matches!(command, Command::Cancel { .. }) {
        sim.exchanges.cancel(player, "Обмен отменён");
        return Ok(());
    }
    if revision != Some(s.revision) {
        return Err("Предложение изменилось. Ознакомьтесь с ним заново".into());
    }
    if s.created.elapsed() > Duration::from_secs(120) || !nearby(sim, s.players) {
        sim.exchanges
            .cancel(player, "Обмен отменён: участники должны находиться рядом");
        return Ok(());
    }
    let mut s = sim.exchanges.sessions.remove(&session).unwrap();
    let i = usize::from(s.players[1] == player);
    let result: Result<(), String> = async {
        match command {
            Command::Accept { .. } => {
                if i != 1 || s.accepted {
                    return Err("Приглашение уже обработано".into());
                }
                s.accepted = true;
            }
            Command::Offer { item, coins, .. } => {
                if !s.accepted {
                    return Err("Дождитесь принятия приглашения".into());
                }
                let actor = sim.actor(player).ok_or("Участник вышел")?;
                if f64::from(coins) > actor.progress.coins {
                    return Err("Недостаточно монет".into());
                }
                let mut tx = conn.begin().await.map_err(|_| "База данных недоступна")?;
                let c = mail::inventory(&mut tx, &actor).await?;
                if let Some(a) = item {
                    if a.price != 0 {
                        return Err("Цена задаётся монетами обмена".into());
                    }
                    mortown_protocol::transfer::take(&mut c.clone(), &a)?;
                }
                tx.commit().await.map_err(|_| "База данных недоступна")?;
                s.inventory[i] = Some(c);
                s.offers[i] = Offer {
                    item,
                    coins,
                    confirmed: false,
                };
                for offer in &mut s.offers {
                    offer.confirmed = false;
                }
            }
            Command::Confirm { .. } => {
                if !s.accepted {
                    return Err("Сначала примите приглашение".into());
                }
                s.offers[i].confirmed = true;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        sim.exchanges.sessions.insert(session, s);
        return Err(e);
    }
    s.revision += 1;
    if s.offers.iter().all(|o| o.confirmed) {
        if let Err(e) = commit(sim, conn, &s).await {
            for id in s.players {
                sim.exchanges.events.insert(
                    id,
                    Reply {
                        view: None,
                        message: e.clone(),
                        update: None,
                    },
                );
            }
            return Err(e);
        }
    } else {
        sim.exchanges
            .publish(&s, "Проверьте обе стороны перед подтверждением");
        sim.exchanges.sessions.insert(session, s);
    }
    Ok(())
}
