use mortown_protocol::{
    Actor,
    party::{self, Command, Member, Reply, View},
};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Parties {
    next: u64,
    groups: HashMap<u64, Group>,
    invites: HashMap<i64, Invite>,
    events: HashMap<i64, String>,
    responses: HashSet<i64>,
    offline: HashMap<i64, Offline>,
}
struct Group {
    id: u64,
    leader: i64,
    members: Vec<i64>,
    reward_after: Option<i64>,
    loot_after: Option<i64>,
    last_loot: String,
}
struct Offline {
    nickname: String,
    level: u32,
    until: Instant,
}
struct Invite {
    id: u64,
    leader: i64,
    party: Option<u64>,
    expires: Instant,
}
impl Parties {
    pub fn membership(&self, id: i64) -> Option<u64> {
        self.groups
            .values()
            .find(|g| g.members.contains(&id))
            .map(|g| g.id)
    }
    pub fn members(&self, id: i64) -> Option<Vec<i64>> {
        self.groups
            .get(&self.membership(id)?)
            .map(|g| g.members.clone())
    }
    fn notify(&mut self, party: u64, message: &str) {
        if let Some(g) = self.groups.get(&party) {
            for id in &g.members {
                self.events.insert(*id, message.into());
            }
        }
    }
    fn cancel_invitations(&mut self, leader: i64) {
        let ids: Vec<_> = self
            .invites
            .iter()
            .filter(|(_, i)| i.leader == leader)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.invites.remove(&id);
            self.events
                .insert(id, "Приглашение в группу отменено".into());
        }
    }
    pub fn leave(&mut self, id: i64) {
        let was_offline = self.offline.remove(&id).is_some();
        self.cancel_invitations(id);
        if let Some(invite) = self.invites.remove(&id) {
            self.events
                .insert(invite.leader, "Приглашение больше не действует".into());
        }
        if let Some(party) = self.membership(id) {
            let g = self.groups.get_mut(&party).unwrap();
            if let Some(index) = g.members.iter().position(|p| *p == id) {
                let previous = g.members[(index + g.members.len() - 1) % g.members.len()];
                if g.reward_after == Some(id) {
                    g.reward_after = Some(previous);
                }
                if g.loot_after == Some(id) {
                    g.loot_after = Some(previous);
                }
            }
            g.members.retain(|p| *p != id);
            if g.members.is_empty() {
                self.groups.remove(&party);
            } else {
                if g.leader == id {
                    g.leader = g
                        .members
                        .iter()
                        .copied()
                        .find(|p| !self.offline.contains_key(p))
                        .unwrap_or(g.members[0]);
                }
                self.notify(party, "Участник покинул группу; лидер отмечен короной");
            }
        }
        if was_offline {
            self.events.remove(&id);
            self.responses.remove(&id);
        } else {
            self.events.insert(id, "Вы вышли из группы".into());
        }
    }
    fn elect_online_leader(&mut self, party: u64) -> bool {
        let g = self.groups.get_mut(&party).unwrap();
        if self.offline.contains_key(&g.leader)
            && let Some(leader) = g.members.iter().find(|p| !self.offline.contains_key(p))
        {
            g.leader = *leader;
            return true;
        }
        false
    }
    /// World exit reserves membership; explicit Leave/Kick still remove it immediately.
    pub fn disconnect(&mut self, actor: &Actor) {
        let id = actor.id;
        self.cancel_invitations(id);
        if let Some(invite) = self.invites.remove(&id) {
            self.events
                .insert(invite.leader, "Приглашённый вышел из мира".into());
        }
        if let Some(party) = self.membership(id) {
            // Repeated cleanup cannot extend an existing reservation.
            self.offline.entry(id).or_insert_with(|| Offline {
                nickname: actor.nickname.clone(),
                level: actor.progress.level,
                until: Instant::now() + Duration::from_secs(u64::from(party::RECONNECT_SECONDS)),
            });
            let changed = self.elect_online_leader(party);
            self.notify(
                party,
                &format!(
                    "{} не в мире · ждём {} мин.{}",
                    actor.nickname,
                    party::RECONNECT_SECONDS / 60,
                    if changed {
                        " Лидерство передано участнику в игре."
                    } else {
                        ""
                    }
                ),
            );
        }
        self.events.remove(&id);
        self.responses.remove(&id);
    }
    pub fn reconnect(&mut self, actor: &Actor) {
        // Expire first: a late login must not renew an elapsed reservation.
        self.expire();
        if self.offline.remove(&actor.id).is_some()
            && let Some(party) = self.membership(actor.id)
        {
            let changed = self.elect_online_leader(party);
            self.notify(
                party,
                &format!(
                    "{} вернулся в группу{}",
                    actor.nickname,
                    if changed {
                        " и стал лидером"
                    } else {
                        ""
                    }
                ),
            );
        }
    }
    pub fn expire(&mut self) {
        let now = Instant::now();
        let mut expired: Vec<_> = self
            .offline
            .iter()
            .filter(|(_, p)| p.until <= now)
            .map(|(id, p)| (*id, p.nickname.clone()))
            .collect();
        expired.sort_by_key(|(id, _)| *id);
        for (id, name) in expired {
            let party = self.membership(id);
            self.leave(id);
            if let Some(party) = party {
                self.notify(
                    party,
                    &format!("{} не вернулся вовремя · место освобождено", name),
                );
            }
        }
        let ids: Vec<_> = self
            .invites
            .iter()
            .filter(|(_, i)| i.expires <= Instant::now())
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(i) = self.invites.remove(&id) {
                self.events
                    .insert(id, "Приглашение в группу истекло".into());
                self.events
                    .insert(i.leader, "Приглашение в группу истекло".into());
            }
        }
    }
    pub fn view(&self, owner: i64, actors: &[Actor], message: String) -> Reply {
        let group = self
            .membership(owner)
            .and_then(|id| self.groups.get(&id))
            .map(|g| View {
                id: g.id,
                leader: g.leader,
                last_loot: g.last_loot.clone(),
                members: g
                    .members
                    .iter()
                    .filter_map(|id| {
                        if let Some(p) = self.offline.get(id) {
                            Some(Member {
                                id: *id,
                                nickname: p.nickname.clone(),
                                level: p.level,
                                online: false,
                                reconnect_seconds: p
                                    .until
                                    .saturating_duration_since(Instant::now())
                                    .as_millis()
                                    .div_ceil(1000)
                                    as u32,
                            })
                        } else {
                            actors.iter().find(|a| a.id == *id).map(|a| Member {
                                id: *id,
                                nickname: a.nickname.clone(),
                                level: a.progress.level,
                                online: true,
                                reconnect_seconds: 0,
                            })
                        }
                    })
                    .collect(),
            });
        let invitation = self.invites.get(&owner).and_then(|i| {
            actors
                .iter()
                .find(|a| a.id == i.leader)
                .map(|a| party::Invitation {
                    id: i.id,
                    inviter: a.nickname.clone(),
                })
        });
        Reply {
            response: false,
            group,
            invitation,
            message,
        }
    }
    pub fn last_message(&self, owner: i64) -> Option<&str> {
        self.events.get(&owner).map(String::as_str)
    }
    pub fn poll(&mut self, owner: i64, actors: &[Actor]) -> Option<Reply> {
        self.expire();
        let message = self.events.remove(&owner)?;
        let mut reply = self.view(owner, actors, message);
        reply.response = self.responses.remove(&owner);
        Some(reply)
    }
    pub fn handle(&mut self, owner: i64, actors: &[Actor], command: Command) {
        self.expire();
        if let Err(e) = self.apply(owner, actors, command) {
            self.events.insert(owner, e.into());
        }
        self.responses.insert(owner);
        self.events
            .entry(owner)
            .or_insert_with(|| "Состав обновлён".into());
    }
    fn apply(
        &mut self,
        owner: i64,
        actors: &[Actor],
        command: Command,
    ) -> Result<(), &'static str> {
        match command {
            Command::Invite { player } => {
                if player == owner || !actors.iter().any(|a| a.id == player) {
                    return Err("Выберите другого игрока в мире");
                }
                if self.membership(player).is_some() || self.invites.contains_key(&player) {
                    return Err("Игрок уже в группе или получил приглашение");
                }
                if self.invites.contains_key(&owner) {
                    return Err("Сначала ответьте на входящее приглашение");
                }
                let party = self.membership(owner);
                let count = if let Some(id) = party {
                    let g = &self.groups[&id];
                    if g.leader != owner {
                        return Err("Приглашать может только лидер группы");
                    }
                    g.members.len()
                } else {
                    1
                };
                let pending = self.invites.values().filter(|i| i.leader == owner).count();
                if count + pending >= party::LIMIT {
                    return Err("В группе максимум пять участников, включая приглашённых");
                }
                // A player arranging their own group cannot be pulled into another invitation.
                if self.invites.values().any(|i| i.leader == player) {
                    return Err("Игрок уже собирает группу");
                }
                self.next += 1;
                self.invites.insert(
                    player,
                    Invite {
                        id: self.next,
                        leader: owner,
                        party,
                        expires: Instant::now() + Duration::from_secs(60),
                    },
                );
                self.events
                    .insert(owner, "Приглашение отправлено на 60 секунд".into());
                self.events.insert(player, "Вас приглашают в группу".into());
            }
            Command::Accept { invitation } => {
                let invite = self
                    .invites
                    .get(&owner)
                    .filter(|i| i.id == invitation)
                    .ok_or("Приглашение уже недействительно")?;
                let leader = invite.leader;
                if self.membership(owner).is_some() || !actors.iter().any(|a| a.id == leader) {
                    return Err("Приглашение уже недействительно");
                }
                let party = if let Some(id) = invite.party {
                    let g = self.groups.get(&id).ok_or("Группа распущена")?;
                    if g.leader != leader || g.members.len() >= party::LIMIT {
                        return Err("Состав или лидер группы изменился");
                    }
                    id
                } else {
                    if self.membership(leader).is_some() {
                        return Err("Лидер уже в другой группе");
                    }
                    self.next += 1;
                    let id = self.next;
                    self.groups.insert(
                        id,
                        Group {
                            id,
                            leader,
                            members: vec![leader],
                            reward_after: None,
                            loot_after: None,
                            last_loot: String::new(),
                        },
                    );
                    for i in self.invites.values_mut().filter(|i| i.leader == leader) {
                        i.party = Some(id);
                    }
                    id
                };
                self.invites.remove(&owner);
                self.groups.get_mut(&party).unwrap().members.push(owner);
                self.notify(party, "Участник присоединился к группе");
            }
            Command::Decline { invitation } => {
                let i = self
                    .invites
                    .get(&owner)
                    .filter(|i| i.id == invitation)
                    .ok_or("Приглашение уже недействительно")?;
                let leader = i.leader;
                self.invites.remove(&owner);
                self.events.insert(owner, "Приглашение отклонено".into());
                self.events
                    .insert(leader, "Игрок отклонил приглашение".into());
            }
            Command::Leave { party } => {
                if self.membership(owner) != Some(party) {
                    return Err("Эта группа уже закрыта");
                }
                self.leave(owner);
            }
            Command::Kick { party, player } | Command::Promote { party, player } => {
                let g = self.groups.get(&party).ok_or("Группа уже закрыта")?;
                if g.leader != owner || owner == player || !g.members.contains(&player) {
                    return Err("Действие доступно лидеру для другого участника своей группы");
                }
                if matches!(command, Command::Kick { .. }) {
                    self.leave(player);
                    if actors.iter().any(|a| a.id == player) {
                        self.events
                            .insert(player, "Лидер исключил вас из группы".into());
                    }
                } else {
                    if self.offline.contains_key(&player) || !actors.iter().any(|a| a.id == player)
                    {
                        return Err("Передать лидерство можно только участнику в игре");
                    }
                    self.cancel_invitations(owner);
                    self.groups.get_mut(&party).unwrap().leader = player;
                    self.notify(party, "Назначен новый лидер группы");
                }
            }
        }
        Ok(())
    }
}

pub(crate) struct RewardPlan {
    pub party: Option<u64>,
    pub members: Vec<i64>,
    pub loot: Vec<i64>,
}
fn after(members: &[i64], previous: Option<i64>) -> Vec<i64> {
    let start = previous
        .and_then(|id| members.iter().position(|p| *p == id))
        .map_or(0, |i| (i + 1) % members.len());
    members
        .iter()
        .cycle()
        .skip(start)
        .take(members.len())
        .copied()
        .collect()
}
impl Parties {
    /// Called while the simulation is locked, before any following attack or roster change.
    pub(crate) fn reward_plan(
        &mut self,
        killer: i64,
        position: [f32; 3],
        actors: &[Actor],
        drops: usize,
    ) -> RewardPlan {
        let Some(id) = self.membership(killer) else {
            return RewardPlan {
                party: None,
                members: vec![killer],
                loot: vec![killer; drops],
            };
        };
        let g = self.groups.get_mut(&id).unwrap();
        let eligible = |id: &i64| {
            !self.offline.contains_key(id)
                && actors.iter().any(|a| {
                    a.id == *id
                        && a.hp > 0.
                        && a.position
                            .iter()
                            .zip(position)
                            .map(|(a, b)| (a - b).powi(2))
                            .sum::<f32>()
                            <= party::REWARD_RANGE.powi(2)
                })
        };
        let members: Vec<_> = after(&g.members, g.reward_after)
            .into_iter()
            .filter(eligible)
            .collect();
        let mut loot = vec![];
        if let Some(first) = members.first() {
            g.reward_after = Some(*first);
            let order: Vec<_> = after(&g.members, g.loot_after)
                .into_iter()
                .filter(|id| members.contains(id))
                .collect();
            loot = order.into_iter().cycle().take(drops).collect();
            if let Some(last) = loot.last() {
                g.loot_after = Some(*last);
            }
        }
        RewardPlan {
            party: Some(id),
            members,
            loot,
        }
    }
    pub(crate) fn record_loot(&mut self, party: u64, message: String) {
        if let Some(g) = self.groups.get_mut(&party) {
            g.last_loot = message;
            // Reward updates must not replace a command response or release its pending state.
            for id in &g.members {
                self.events.entry(*id).or_default();
            }
        }
    }
}
