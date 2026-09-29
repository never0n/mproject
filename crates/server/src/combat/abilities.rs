use super::*;
use mortown_protocol::skills::{self, Area, Effect, StatusKind};

pub(super) struct SandArea {
    pub view: Area,
    pub owner: i64,
    pub until: u64,
}

impl Combat {
    pub fn areas(&self, tick: u64) -> Vec<Area> {
        self.areas
            .iter()
            .filter(|a| a.until > tick)
            .map(|a| Area {
                remaining_ms: (a.until - tick) as u32 * 50,
                ..a.view.clone()
            })
            .collect()
    }
    pub fn use_ability(
        &mut self,
        actor: &mut Actor,
        id: u8,
        target: Option<u32>,
        world: &mortown_world::World,
        tick: u64,
    ) -> Result<(), &'static str> {
        let fighter = self
            .fighters
            .get(&actor.id)
            .ok_or("Персонаж не готов к бою")?;
        let class = fighter.class;
        let ability = skills::definition(class, fighter.race, id).ok_or("Неизвестный навык")?;
        if ability.effect == Effect::Trader {
            return Err("Пассивный навык работает постоянно");
        }
        if actor.hp <= 0. {
            return Err("Сначала возродитесь");
        }
        if !skills::learned(&actor.progress, id) {
            return Err("Сначала изучите навык в окне K");
        }
        if tick
            < self
                .ability_ready
                .get(&(actor.id, id))
                .copied()
                .unwrap_or(0)
        {
            return Err("Навык перезаряжается");
        }
        // Racial abilities use their own range; self buffs need no hostile target.
        let index = if ability.needs_target() {
            let target = target.ok_or("Выберите цель")?;
            let index = self
                .mobs
                .iter()
                .position(|m| m.view.id == target && m.view.hp > 0.)
                .ok_or("Цель недоступна")?;
            if mortown_world::distance(point(actor), pos(&self.mobs[index])) > ability.skill.range {
                return Err("Подойдите ближе к цели");
            }
            if !world.segment_clear(point(actor), pos(&self.mobs[index])) {
                return Err("Между вами и целью препятствие");
            }
            Some(index)
        } else {
            None
        };
        let immortal = actor.powers.immortal;
        let resource = if class == 2 {
            &mut actor.mana
        } else {
            &mut actor.stamina
        };
        if !super::pay(immortal, resource, ability.skill.cost) {
            return Err("Недостаточно ресурса для навыка");
        }
        let aim = index.map(|i| self.mobs[i].view.position);
        record_action(
            &mut self.next_action,
            actor,
            tick,
            if ability.effect == Effect::Smoke {
                mortown_protocol::actions::Kind::Gesture
            } else if class == 2
                || matches!(
                    ability.effect,
                    Effect::Earth | Effect::Quicksand | Effect::Acid
                )
            {
                mortown_protocol::actions::Kind::Spell
            } else {
                mortown_protocol::actions::Kind::Weapon
            },
            Some(id),
            aim,
        );
        self.ability_ready
            .insert((actor.id, id), tick + ability.skill.cooldown_ticks);
        let f = self.fighters.get_mut(&actor.id).unwrap();
        f.last_combat = tick;
        if ability.effect == Effect::Smoke {
            f.smoke_until = tick + 80;
            actor.effects = vec![skills::Status {
                kind: StatusKind::Smoke,
                remaining_ms: 4000,
            }];
            actor.combat_status = "Дымовая завеса: защита от попаданий на 4 с".into();
        } else if ability.effect == Effect::Earth {
            let center = pos(&self.mobs[index.unwrap()]);
            let prefix = if class == 2 { "m" } else { "f" };
            let s = |key: &str| stat(&f.stats, &format!("{prefix}_{key}"));
            let (power, accuracy, chance, critical_power) = (
                s("attack"),
                s("accuracy"),
                s("crit_chance"),
                s("crit_power"),
            );
            let (mut affected, mut total, mut killed) = (0, 0., 0);
            for m in &mut self.mobs {
                if m.view.hp <= 0.
                    || mortown_world::distance(center, pos(m)) > 4.
                    || !world.segment_clear(center, pos(m))
                    || !world.segment_clear(point(actor), pos(m))
                {
                    continue;
                }
                super::aggro(actor, &mut m.target);
                let (hit, _) = strike(
                    power * ability.skill.multiplier,
                    m.defence(class, tick),
                    accuracy,
                    m.view.kind.definition().evasion,
                    chance,
                    critical_power,
                    self.random,
                );
                if hit <= 0. {
                    continue;
                }
                affected += 1;
                total += hit;
                m.view.hp = (m.view.hp - hit).max(0.);
                if m.view.hp <= 0. {
                    m.defeat(tick);
                    if actor.attack_target == Some(m.view.id) {
                        actor.attack_target = None;
                    }
                    self.rewards.push(rewards::Defeat {
                        killer: actor.id,
                        position: m.view.position,
                        name: m.view.name.clone(),
                        kind: m.view.kind,
                    });
                    killed += 1;
                } else {
                    m.effects.stun_until = tick + 30;
                    m.cancel_cast(tick);
                    m.next_attack = m.next_attack.max(tick + 30);
                    m.view.effects = m.effects.views(tick);
                }
            }
            actor.combat_status =
                format!("Гнев земли: попаданий {affected}, урон {total:.0}, повержено {killed}");
        } else if ability.effect == Effect::Quicksand {
            let position = self.mobs[index.unwrap()].view.position;
            self.next_area += 1;
            self.areas.push(SandArea {
                view: Area {
                    id: self.next_area,
                    position,
                    radius: 4.,
                    remaining_ms: 6000,
                },
                owner: actor.id,
                until: tick + 120,
            });
            for m in &mut self.mobs {
                if m.view.hp > 0.
                    && mortown_world::distance([position[0], position[2]], pos(m)) <= 4.
                    && world.segment_clear([position[0], position[2]], pos(m))
                {
                    m.effects.sand_until = tick + 120;
                    m.view.effects = m.effects.views(tick);
                    super::aggro(actor, &mut m.target);
                }
            }
            actor.combat_status = "Зыбучий песок: область на 6 с".into();
        } else {
            let prefix = if class == 2 { "m" } else { "f" };
            let s = |key: &str| stat(&f.stats, &format!("{prefix}_{key}"));
            let power = s("attack");
            let m = &mut self.mobs[index.unwrap()];
            let (hit, critical) = strike(
                power * ability.skill.multiplier,
                m.defence(class, tick),
                s("accuracy"),
                m.view.kind.definition().evasion,
                s("crit_chance"),
                s("crit_power"),
                self.random,
            );
            m.view.hp = (m.view.hp - hit).max(0.);
            super::aggro(actor, &mut m.target);
            let mut extra = "";
            if hit > 0. && m.view.hp > 0. {
                match ability.effect {
                    Effect::Bleed => {
                        m.effects
                            .dot(StatusKind::Bleeding, actor.id, power * 0.10, tick, 6)
                    }
                    Effect::Cripple => {
                        if m.effects.cripple_until <= tick {
                            m.next_attack = tick + m.next_attack.saturating_sub(tick) * 3 / 2;
                        }
                        m.effects.cripple_until = tick + 120;
                    }
                    Effect::Blind => m.effects.blind_until = tick + 30,
                    Effect::Acid => m.effects.corroded_until = tick + 100,
                    Effect::Prism => {
                        let element = (self.random)();
                        if element < 1. / 3. {
                            m.effects
                                .dot(StatusKind::Burning, actor.id, power * 0.15, tick, 4);
                            extra = " · огонь";
                        } else if element < 2. / 3. {
                            m.slow_until = tick + 80;
                            m.view.slow_ms = 4000;
                            extra = " · лёд";
                        } else {
                            m.effects
                                .dot(StatusKind::Poisoned, actor.id, power * 0.10, tick, 6);
                            extra = " · яд";
                        }
                    }
                    _ => {}
                }
            }
            m.view.effects = m.effects.views(tick);
            actor.combat_status = if hit == 0. {
                format!("{}: промах", ability.skill.name)
            } else {
                format!(
                    "{}: {hit:.0} урона{}{extra}",
                    ability.skill.name,
                    if critical { " · крит!" } else { "" }
                )
            };
            if m.view.hp <= 0. {
                m.defeat(tick);
                if actor.attack_target == Some(m.view.id) {
                    actor.attack_target = None;
                }
                actor
                    .combat_status
                    .push_str(&format!(" · {} повержен", m.view.name));
                self.rewards.push(rewards::Defeat {
                    killer: actor.id,
                    position: m.view.position,
                    name: m.view.name.clone(),
                    kind: m.view.kind,
                });
            }
        }
        if ability.effect == Effect::Overload {
            let until = tick + 200;
            let first = self.skill_ready.entry(actor.id).or_default();
            *first = (*first).max(until);
            let second = self.secondary_ready.entry(actor.id).or_default();
            *second = (*second).max(until);
            for skill in 3..=skills::LAST_ACTIVE {
                let ready = self.ability_ready.entry((actor.id, skill)).or_default();
                *ready = (*ready).max(until);
            }
            actor.skill_cooldown_ms = (self.skill_ready[&actor.id] - tick) as u32 * 50;
            actor.secondary_cooldown_ms = (self.secondary_ready[&actor.id] - tick) as u32 * 50;
            actor
                .combat_status
                .push_str(" · навыки заблокированы на 10 с");
        }
        actor.ability_cooldowns_ms = std::array::from_fn(|n| {
            self.ability_ready
                .get(&(actor.id, n as u8 + 3))
                .copied()
                .unwrap_or(0)
                .saturating_sub(tick) as u32
                * 50
        });
        Ok(())
    }
}
