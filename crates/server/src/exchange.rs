use crate::{
    inventory::{self, Change},
    simulation::Simulation,
    store::ExchangeFailure,
};
use mortown_protocol::{
    Actor, Character,
    exchange::{Command, Offer, Reply, View},
    items::Stack,
    transfer::{self, Holder, InventoryUpdate},
};
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
    /// The offered stack as it was when offered; any change before the commit cancels.
    offered: [Option<Stack>; 2],
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
        let character = crate::store::sheet(conn, player).await?;
        let update = Box::new(InventoryUpdate { character, actor });
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
    let actors: [Actor; 2] = [
        sim.actor(s.players[0]).ok_or("Участник вышел")?,
        sim.actor(s.players[1]).ok_or("Участник вышел")?,
    ];
    let [mut a, mut b] = actors;
    let order = if s.players[0] < s.players[1] {
        [0, 1]
    } else {
        [1, 0]
    };
    let (first, second) = inventory::transaction(conn, async |tx| {
        let mut chars: Vec<Option<Character>> = vec![None, None];
        for i in order {
            chars[i] = Some(inventory::load(tx, [&a, &b][i]).await?);
        }
        let mut chars = [chars[0].take().unwrap(), chars[1].take().unwrap()];
        let before = [chars[0].items.clone(), chars[1].items.clone()];
        let mut moved: [Option<Stack>; 2] = [None, None];
        for i in 0..2 {
            if let Some(o) = s.offers[i].item {
                let now = chars[i].items.iter().find(|x| x.id == o.item).copied();
                if now.is_none() || now != s.offered[i] {
                    return Err("Обмен отменён: содержимое рюкзака изменилось".into());
                }
                moved[i] = Some(transfer::split(
                    &mut chars[i].items,
                    Holder::Character,
                    o.item,
                    o.quantity,
                )?);
            }
            if s.offers[i].coins > 0 {
                transfer::pay(&mut chars[i].items, Holder::Character, s.offers[i].coins)
                    .map_err(|_| "Обмен отменён: недостаточно монет")?;
            }
        }
        for i in 0..2 {
            if let Some(stack) = moved[i] {
                transfer::put(&mut chars[1 - i].items, Holder::Character, stack)?;
            }
            let coins = s.offers[i].coins;
            if coins > 0 {
                let total = transfer::coins(&chars[1 - i].items, Holder::Character)
                    .checked_add(coins)
                    .ok_or("Баланс монет получателя переполнен")?;
                transfer::set_coins(&mut chars[1 - i].items, Holder::Character, total)?;
            }
        }
        for i in 0..2 {
            transfer::check_carry(&before[i], &chars[i].items)
                .map_err(|e| format!("Обмен отменён: {} — {e}", chars[i].nickname))?;
        }
        inventory::sync(
            tx,
            &[
                Change {
                    owner: Some(chars[0].id),
                    before: &before[0],
                    after: &chars[0].items,
                },
                Change {
                    owner: Some(chars[1].id),
                    before: &before[1],
                    after: &chars[1].items,
                },
            ],
        )
        .await?;
        let first = inventory::finish(tx, &mut a, &chars[0], "Обмен завершён").await?;
        let second = inventory::finish(tx, &mut b, &chars[1], "Обмен завершён").await?;
        Ok((first, second))
    })
    .await
    .map_err(|e| match e {
        ExchangeFailure::Rejected(message) => message,
        ExchangeFailure::Uncertain => {
            "Не удалось подтвердить сохранение обмена; переподключитесь".into()
        }
    })?;
    let first = InventoryUpdate {
        character: first,
        actor: a,
    };
    let second = InventoryUpdate {
        character: second,
        actor: b,
    };
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
            offered: Default::default(),
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
                let c =
                    inventory::transaction(conn, async |tx| Ok(inventory::load(tx, &actor).await?))
                        .await
                        .map_err(|e| match e {
                            ExchangeFailure::Rejected(message) => message,
                            ExchangeFailure::Uncertain => "База данных недоступна".into(),
                        })?;
                if coins > transfer::coins(&c.items, Holder::Character) {
                    return Err("Недостаточно монет".into());
                }
                let mut kind = 0;
                s.offered[i] = None;
                if let Some(a) = item {
                    let mut probe = c.items.clone();
                    let stack = transfer::split(&mut probe, Holder::Character, a.item, a.quantity)?;
                    kind = stack.kind;
                    s.offered[i] = c.items.iter().find(|x| x.id == a.item).copied();
                }
                s.offers[i] = Offer {
                    item,
                    kind,
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
