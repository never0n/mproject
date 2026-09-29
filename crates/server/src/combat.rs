use mortown_protocol::{Actor, Character, Monster, monsters::Kind};
use rand_core::{OsRng, RngCore};
use std::collections::{BTreeMap, HashMap};
mod abilities;
mod effects;
mod rewards;

pub struct Fighter {
    stats: BTreeMap<String, f64>,
    class: u8,
    race: u8,
    smoke_until: u64,
    next_attack: u64,
    last_combat: u64,
    respawn_at: u64,
}
pub struct Mob {
    pub view: Monster,
    home: [f32; 2],
    target: Option<i64>,
    next_attack: u64,
    respawn_at: u64,
    stuck: u32,
    slow_until: u64,
    effects: effects::Effects,
    casting: Option<(i64, u64)>,
    attack_sequence: u32,
    motion_started: u64,
}
impl Mob {
    fn admin_new(kind: Kind, id: u32, position: [f32; 3]) -> Self {
        let spec = kind.definition();
        Self {
            view: Monster {
                kind,
                id,
                name: spec.name.into(),
                position,
                yaw: 0.,
                hp: spec.hp,
                max_hp: spec.hp,
                level: spec.level,
                mana: None,
                stamina: None,
                cast: None,
                attack: None,
                death_ms: None,
                slow_ms: 0,
                effects: vec![],
            },
            home: [position[0], position[2]],
            target: None,
            next_attack: 0,
            respawn_at: 0,
            stuck: 0,
            slow_until: 0,
            effects: effects::Effects::default(),
            casting: None,
            attack_sequence: 0,
            motion_started: 0,
        }
    }
    fn defence(&self, class: u8, tick: u64) -> f64 {
        let spec = self.view.kind.definition();
        self.effects.defence(
            class,
            tick,
            if class == 2 {
                spec.magical_defence
            } else {
                spec.physical_defence
            },
        )
    }
    fn cancel_cast(&mut self, tick: u64) {
        if self.casting.take().is_some() {
            self.next_attack = self.next_attack.max(tick + 20);
            self.view.attack = None;
        }
        self.view.cast = None;
    }
    fn begin_attack(&mut self, target: i64, tick: u64) {
        let (windup, duration) = self.view.kind.attack_timing();
        self.attack_sequence = self.attack_sequence.wrapping_add(1);
        self.motion_started = tick;
        self.casting = Some((target, tick + windup));
        self.view.attack = Some(mortown_protocol::monsters::AttackMotion {
            sequence: self.attack_sequence,
            elapsed_ms: 0,
            impact_ms: windup as u32 * 50,
            duration_ms: duration as u32 * 50,
            released: false,
        });
    }
    fn defeat(&mut self, tick: u64) {
        self.target = None;
        self.respawn_at = tick + self.view.kind.definition().respawn_ticks;
        self.casting = None;
        self.view.cast = None;
        self.view.attack = None;
        self.view.death_ms = Some(0);
        self.stuck = 0;
        self.slow_until = 0;
        self.view.slow_ms = 0;
        self.effects = effects::Effects::default();
        self.view.effects.clear();
    }
}
pub struct Combat {
    pub next_admin_mob: u32,
    fighters: HashMap<i64, Fighter>,
    pub mobs: Vec<Mob>,
    random: fn() -> f64,
    skill_ready: HashMap<i64, u64>,
    secondary_ready: HashMap<i64, u64>,
    ability_ready: HashMap<(i64, u8), u64>,
    areas: Vec<abilities::SandArea>,
    next_area: u64,
    next_action: u64,
    rewards: Vec<rewards::Defeat>,
}
#[cfg(test)]
fn grant_reward(actor: &mut Actor, fighter: &mut Fighter, random: fn() -> f64) {
    let drops = rewards::roll_drops(fighter.class, Kind::Bandit, random);
    rewards::grant_share(
        actor,
        fighter,
        mortown_protocol::progression::KILL_EXPERIENCE,
        mortown_protocol::progression::KILL_COINS,
        &drops,
        false,
        Kind::Bandit,
    );
}
fn point(a: &Actor) -> [f32; 2] {
    [a.position[0], a.position[2]]
}
fn pos(m: &Mob) -> [f32; 2] {
    [m.view.position[0], m.view.position[2]]
}
fn record_action(
    sequence: &mut u64,
    actor: &mut Actor,
    tick: u64,
    kind: mortown_protocol::actions::Kind,
    skill: Option<u8>,
    target: Option<[f32; 3]>,
) {
    *sequence = sequence.wrapping_add(1);
    let yaw = target
        .map(|p| (p[0] - actor.position[0]).atan2(p[2] - actor.position[2]))
        .unwrap_or(actor.yaw);
    actor.actions.push(mortown_protocol::actions::Motion {
        sequence: *sequence,
        started_tick: tick,
        elapsed_ms: 0,
        duration_ms: mortown_protocol::actions::duration(kind, actor.equipment.weapon),
        kind,
        weapon: actor.equipment.weapon,
        skill,
        origin: actor.position,
        target,
        yaw,
    });
    if actor.actions.len() > mortown_protocol::actions::MAX_RECENT {
        actor.actions.remove(0);
    }
}
fn stat(stats: &BTreeMap<String, f64>, key: &str) -> f64 {
    stats.get(key).copied().unwrap_or(0.)
}
// Original formulas.js/gameConfig.js. JS Math.round on positive values matches round().
pub fn damage(attack: f64, defence: f64) -> f64 {
    (attack * 50. / (defence + 50.)).round().max(1.)
}
pub fn hit_chance(accuracy: f64, evasion: f64) -> f64 {
    let diff = accuracy - evasion;
    (((0.8 + diff * if diff >= 0. { 0.01 } else { 0.02 }) * 100.).round() / 100.).clamp(0.1, 0.99)
}
fn roll() -> f64 {
    OsRng.next_u32() as f64 / (u32::MAX as f64 + 1.)
}
fn pursued(actors: &[Actor], id: i64) -> Option<usize> {
    actors
        .iter()
        .position(|a| a.id == id && a.hp > 0. && !a.powers.invisible)
}
fn living(actors: &[Actor], id: i64) -> Option<(usize, &Actor)> {
    actors
        .iter()
        .enumerate()
        .find(|(_, a)| a.id == id && a.hp > 0.)
}
/// Keep the attacker as the mob's combat target even while invisible.
/// Pursuit and retaliation use `pursued`, so an invisible admin is not chased.
/// Without this target, an idle mob at home treats the hit as the end of combat and restores all HP.
fn aggro(actor: &Actor, target: &mut Option<i64>) {
    *target = Some(actor.id);
}
fn pay(immortal: bool, pool: &mut f64, cost: f64) -> bool {
    if immortal {
        return true;
    }
    if *pool < cost {
        return false;
    }
    *pool -= cost;
    true
}
fn strike(
    atk: f64,
    def: f64,
    acc: f64,
    eva: f64,
    chance: f64,
    power: f64,
    random: fn() -> f64,
) -> (f64, bool) {
    if random() >= hit_chance(acc, eva) {
        return (0., false);
    }
    let critical = random() < chance.clamp(0.03, 0.8);
    let base = damage(atk, def);
    (
        if critical {
            (base * power).round().max(1.)
        } else {
            base
        },
        critical,
    )
}
impl Combat {
    pub fn new(world: &mortown_world::World) -> Self {
        let gate = world.gates[2];
        let end = [390., 790.];
        let mut mobs = Vec::new();
        let along = |start: [f32; 2], finish: [f32; 2], d: f32| {
            let length = mortown_world::distance(start, finish);
            [
                start[0] + (finish[0] - start[0]) / length * d,
                start[1] + (finish[1] - start[1]) / length * d,
            ]
        };
        let camps = [
            (Kind::Bandit, along(gate, end, 90.)),
            (Kind::Bandit, along(gate, end, 170.)),
            (Kind::Bandit, along(gate, end, 250.)),
            (Kind::Marauder, along(gate, end, 350.)),
            (Kind::Marauder, along(gate, end, 440.)),
            (Kind::Apostate, along(end, [210., 1100.], 80.)),
            (Kind::Apostate, along(end, [210., 1100.], 180.)),
        ];
        for (i, (kind, home)) in camps.into_iter().enumerate() {
            assert!(
                world.walkable(home[0], home[1]),
                "camp must be on a walkable road"
            );
            let spec = kind.definition();
            mobs.push(Mob {
                view: Monster {
                    kind,
                    cast: None,
                    attack: None,
                    death_ms: None,
                    level: spec.level,
                    mana: None,
                    stamina: None,
                    id: i as u32 + 1,
                    name: spec.name.into(),
                    position: [home[0], world.surface_height(home[0], home[1]), home[1]],
                    yaw: 0.,
                    hp: spec.hp,
                    max_hp: spec.hp,
                    slow_ms: 0,
                    effects: vec![],
                },
                home,
                target: None,
                next_attack: 0,
                respawn_at: 0,
                stuck: 0,
                slow_until: 0,
                effects: effects::Effects::default(),
                casting: None,
                attack_sequence: 0,
                motion_started: 0,
            });
        }
        Self {
            next_admin_mob: 8,
            fighters: HashMap::new(),
            mobs,
            random: roll,
            skill_ready: HashMap::new(),
            secondary_ready: HashMap::new(),
            ability_ready: HashMap::new(),
            areas: vec![],
            next_area: 0,
            next_action: 0,
            rewards: vec![],
        }
    }
    pub fn spawn_admin(&mut self, kind: Kind, position: [f32; 3]) -> Result<u32, &'static str> {
        if self.mobs.len() >= 128 {
            return Err("Достигнут предел: 128 мобов");
        }
        let id = self.next_admin_mob;
        self.next_admin_mob = id.checked_add(1).ok_or("Исчерпаны ID")?;
        self.mobs.push(Mob::admin_new(kind, id, position));
        Ok(id)
    }
    pub fn kill_admin(&mut self, id: u32, tick: u64) -> Result<(), &'static str> {
        let m = self
            .mobs
            .iter_mut()
            .find(|m| m.view.id == id)
            .ok_or("Моб не найден")?;
        if m.view.hp <= 0. {
            return Err("Цель уже мертва");
        }
        m.view.hp = 0.;
        m.view.death_ms = Some(0);
        m.defeat(tick);
        Ok(())
    }
    pub fn revive_admin(&mut self, id: u32) -> Result<(), &'static str> {
        let m = self
            .mobs
            .iter_mut()
            .find(|m| m.view.id == id)
            .ok_or("Моб не найден")?;
        let home = m.home;
        *m = Mob::admin_new(m.view.kind, id, m.view.position);
        m.home = home;
        Ok(())
    }
    pub fn remove_admin(&mut self, id: u32) -> Result<(), &'static str> {
        if !self.mobs.iter().any(|m| m.view.id == id) {
            return Err("Моб уже удалён");
        }
        self.mobs.retain(|m| m.view.id != id);
        Ok(())
    }
    pub fn forget_player(&mut self, id: i64, tick: u64) {
        for m in &mut self.mobs {
            if m.target == Some(id) {
                m.target = None;
                m.cancel_cast(tick);
            }
        }
    }
    pub fn player_admin(&mut self, a: &mut Actor, kill: bool, tick: u64) {
        if let Some(f) = self.fighters.get_mut(&a.id) {
            a.hp = if kill { 0. } else { a.max_hp };
            if !kill {
                a.mana = stat(&f.stats, "mana");
                a.stamina = stat(&f.stats, "stamina");
            }
            a.respawn_seconds = if kill { 10 } else { 0 };
            f.respawn_at = if kill { tick + 200 } else { 0 };
            f.last_combat = tick;
            f.smoke_until = 0;
            a.effects.clear();
            a.actions.clear();
            a.attack_target = None;
            for m in &mut self.mobs {
                if m.target == Some(a.id) {
                    m.target = None;
                    m.cancel_cast(tick);
                }
            }
        }
    }
    pub fn reset_admin(&mut self, world: &mortown_world::World) {
        let base = Self::new(world);
        self.mobs = base.mobs;
        self.areas.clear();
        self.rewards.clear();
    }
    pub fn configure(&mut self, c: &Character, actor: &mut Actor, tick: u64) {
        actor.actions.clear();
        actor.appearance = c.into();
        actor.equipment =
            mortown_protocol::items::EquipmentAppearance::from_inventory(&c.inventory);
        actor.ability_cooldowns_ms = std::array::from_fn(|n| {
            self.ability_ready
                .get(&(c.id, n as u8 + 3))
                .copied()
                .unwrap_or(0)
                .saturating_sub(tick) as u32
                * 50
        });
        actor.skill_cooldown_ms = self
            .skill_ready
            .get(&c.id)
            .copied()
            .unwrap_or(0)
            .saturating_sub(tick) as u32
            * 50;
        actor.secondary_cooldown_ms = self
            .secondary_ready
            .get(&c.id)
            .copied()
            .unwrap_or(0)
            .saturating_sub(tick) as u32
            * 50;
        actor.loot = c.loot.clone();
        actor.effects.clear();
        actor.progress = mortown_protocol::progression::Progress::from_stats(&c.stats);
        actor.max_hp = stat(&c.stats, "hp").max(1.);
        actor.hp = stat(&c.stats, "current_hp").clamp(0., actor.max_hp);
        actor.respawn_seconds = if actor.hp <= 0. { 10 } else { 0 };
        actor.mana = stat(&c.stats, "current_mana");
        actor.stamina = stat(&c.stats, "current_stamina");
        self.fighters.insert(
            c.id,
            Fighter {
                stats: c.stats.clone(),
                class: c.class,
                race: c.race,
                smoke_until: 0,
                next_attack: tick,
                last_combat: tick,
                respawn_at: if actor.hp <= 0. { tick + 200 } else { 0 },
            },
        );
    }
    pub fn refresh(&mut self, c: &Character) {
        if let Some(f) = self.fighters.get_mut(&c.id) {
            let level = f.stats.get("level").copied().unwrap_or(1.) as u32;
            f.stats = c.stats.clone();
            mortown_protocol::progression::grow(
                &mut f.stats,
                level.saturating_sub(c.stats["level"] as u32),
            );
            f.stats.insert("level".into(), level as f64);
        }
    }
    pub fn leave(&mut self, id: i64) {
        self.fighters.remove(&id);
        self.areas.retain(|a| a.owner != id);
        for m in &mut self.mobs {
            m.effects.dots.retain(|d| d.owner != id);
        }
    }
    pub fn select(
        &self,
        actor: &mut Actor,
        target: Option<u32>,
        world: &mortown_world::World,
    ) -> Result<(), &'static str> {
        if actor.hp <= 0. {
            return Err("Вы погибли. Нажмите R после отсчёта");
        }
        if let Some(id) = target {
            let f = self
                .fighters
                .get(&actor.id)
                .ok_or("Персонаж не готов к бою")?;
            let range = mortown_protocol::weapons::attack_range(actor.equipment.weapon, f.class);
            self.validate_target(actor, id, range, world)?;
        }
        actor.attack_target = target;
        actor.combat_status = if target.is_some() {
            "Автоатака включена"
        } else {
            "Автоатака остановлена"
        }
        .into();
        Ok(())
    }
    fn validate_target(
        &self,
        actor: &Actor,
        id: u32,
        range: f32,
        world: &mortown_world::World,
    ) -> Result<(), &'static str> {
        if actor.hp <= 0. {
            return Err("Вы погибли. Нажмите R после отсчёта");
        }
        let mob = self
            .mobs
            .iter()
            .find(|m| m.view.id == id && m.view.hp > 0.)
            .ok_or("Цель недоступна")?;
        if mortown_world::distance(point(actor), pos(mob)) > range {
            return Err("Подойдите ближе к цели");
        }
        if !world.segment_clear(point(actor), pos(mob)) {
            return Err("Между вами и целью препятствие");
        }
        Ok(())
    }
    pub fn use_skill(
        &mut self,
        actor: &mut Actor,
        target: u32,
        world: &mortown_world::World,
        tick: u64,
    ) -> Result<(), &'static str> {
        self.cast(actor, target, world, tick, false)
    }
    pub fn use_secondary(
        &mut self,
        actor: &mut Actor,
        target: u32,
        world: &mortown_world::World,
        tick: u64,
    ) -> Result<(), &'static str> {
        self.cast(actor, target, world, tick, true)
    }
    pub fn in_combat(&self, id: i64, tick: u64) -> bool {
        self.fighters
            .get(&id)
            .is_some_and(|f| tick.saturating_sub(f.last_combat) <= 200)
            || self
                .mobs
                .iter()
                .any(|m| m.target == Some(id) && m.view.hp > 0.)
    }
    fn cast(
        &mut self,
        actor: &mut Actor,
        target: u32,
        world: &mortown_world::World,
        tick: u64,
        secondary: bool,
    ) -> Result<(), &'static str> {
        if !secondary && actor.progress.primary_disabled {
            return Err("Навык не изучен");
        }
        if secondary && !actor.progress.secondary_learned {
            return Err("Сначала изучите второй навык в окне K");
        }
        // Skill reach is independent of the currently equipped autoattack weapon.
        let class = self
            .fighters
            .get(&actor.id)
            .ok_or("Персонаж не готов к бою")?
            .class;
        let skill = if secondary {
            mortown_protocol::skills::secondary(class)
        } else {
            mortown_protocol::skills::ranked(class, actor.progress.skill_rank)
        }
        .ok_or("Навык недоступен")?;
        self.validate_target(actor, target, skill.range, world)?;
        let f = self.fighters.get_mut(&actor.id).unwrap();
        let ready = if secondary {
            &mut self.secondary_ready
        } else {
            &mut self.skill_ready
        };
        if tick < ready.get(&actor.id).copied().unwrap_or(0) {
            return Err("Навык перезаряжается");
        }
        let m = self
            .mobs
            .iter_mut()
            .find(|m| m.view.id == target)
            .ok_or("Цель недоступна")?;
        if mortown_world::distance(point(actor), pos(m)) > skill.range {
            return Err("Подойдите ближе к цели");
        }
        let immortal = actor.powers.immortal;
        let resource = if f.class == 2 {
            &mut actor.mana
        } else {
            &mut actor.stamina
        };
        if !pay(immortal, resource, skill.cost) {
            return Err("Недостаточно ресурса для навыка");
        }
        ready.insert(actor.id, tick + skill.cooldown_ticks);
        if secondary {
            actor.secondary_cooldown_ms = skill.cooldown_ticks as u32 * 50;
        } else {
            actor.skill_cooldown_ms = skill.cooldown_ticks as u32 * 50;
        }
        f.last_combat = tick;
        let prefix = if f.class == 2 { "m" } else { "f" };
        let s = |key: &str| stat(&f.stats, &format!("{prefix}_{key}"));
        let (hit, crit) = strike(
            s("attack") * skill.multiplier,
            m.defence(f.class, tick),
            s("accuracy"),
            m.view.kind.definition().evasion,
            s("crit_chance"),
            s("crit_power"),
            self.random,
        );
        if secondary && hit > 0. {
            m.slow_until = tick + 80;
            m.view.slow_ms = 4000;
        }
        m.view.hp = (m.view.hp - hit).max(0.);
        aggro(actor, &mut m.target);
        record_action(
            &mut self.next_action,
            actor,
            tick,
            if class == 2 {
                mortown_protocol::actions::Kind::Spell
            } else {
                mortown_protocol::actions::Kind::Weapon
            },
            Some(if secondary { 2 } else { 1 }),
            Some(m.view.position),
        );
        actor.combat_status = if hit == 0. {
            format!("{}: промах", skill.name)
        } else {
            format!(
                "{}: {:.0} урона{}",
                skill.name,
                hit,
                if crit { " · крит!" } else { "" }
            )
        };
        if m.view.hp <= 0. {
            m.defeat(tick);
            if actor.attack_target == Some(target) {
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
        Ok(())
    }
    pub fn respawn(
        &mut self,
        actor: &mut Actor,
        world: &mortown_world::World,
        tick: u64,
    ) -> Result<(), &'static str> {
        let f = self
            .fighters
            .get_mut(&actor.id)
            .ok_or("Персонаж не найден")?;
        if actor.hp > 0. {
            return Err("Персонаж жив");
        }
        if tick < f.respawn_at {
            return Err("Возрождение ещё недоступно");
        }
        let p = mortown_world::SPAWN;
        actor.position = [p[0], world.surface_height(p[0], p[1]), p[1]];
        actor.hp = actor.max_hp;
        actor.mana = stat(&f.stats, "mana");
        actor.stamina = stat(&f.stats, "stamina");
        actor.respawn_seconds = 0;
        actor.effects.clear();
        f.smoke_until = 0;
        actor.attack_target = None;
        actor.actions.clear();
        actor.combat_status = "Вы возродились на площади".into();
        f.last_combat = tick;
        Ok(())
    }
    pub fn resolve_rewards(&mut self, actors: &mut [Actor], parties: &mut crate::party::Parties) {
        rewards::distribute(
            &mut self.fighters,
            actors,
            parties,
            &mut self.rewards,
            self.random,
        );
    }
    pub fn update(
        &mut self,
        actors: &mut [Actor],
        world: &mortown_world::World,
        tick: u64,
        parties: &mut crate::party::Parties,
    ) {
        self.skill_ready.retain(|_, ready| *ready > tick);
        self.secondary_ready.retain(|_, ready| *ready > tick);
        self.ability_ready.retain(|_, ready| *ready > tick);
        self.areas.retain(|area| {
            area.until > tick && actors.iter().any(|a| a.id == area.owner && a.hp > 0.)
        });
        for index in 0..actors.len() {
            let a = &mut actors[index];
            for action in &mut a.actions {
                action.elapsed_ms =
                    (tick.saturating_sub(action.started_tick) * 50).min(u32::MAX as u64) as u32;
            }
            a.actions
                .retain(|action| action.elapsed_ms < action.duration_ms);
            a.ability_cooldowns_ms = std::array::from_fn(|n| {
                self.ability_ready
                    .get(&(a.id, n as u8 + 3))
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(tick) as u32
                    * 50
            });
            a.secondary_cooldown_ms = self
                .secondary_ready
                .get(&a.id)
                .copied()
                .unwrap_or(0)
                .saturating_sub(tick) as u32
                * 50;
            a.skill_cooldown_ms = self
                .skill_ready
                .get(&a.id)
                .copied()
                .unwrap_or(0)
                .saturating_sub(tick) as u32
                * 50;
            let Some(f) = self.fighters.get_mut(&a.id) else {
                continue;
            };
            if a.hp <= 0. {
                a.actions.clear();
                a.effects.clear();
                f.smoke_until = 0;
                a.attack_target = None;
                a.respawn_seconds = f.respawn_at.saturating_sub(tick).div_ceil(20) as u32;
                continue;
            }
            a.effects = if f.smoke_until > tick {
                vec![mortown_protocol::skills::Status {
                    kind: mortown_protocol::skills::StatusKind::Smoke,
                    remaining_ms: (f.smoke_until - tick) as u32 * 50,
                }]
            } else {
                vec![]
            };
            if tick.is_multiple_of(20) && tick.saturating_sub(f.last_combat) > 200 {
                a.hp = (a.hp + 2.).min(a.max_hp);
                a.mana = (a.mana + 3.).min(stat(&f.stats, "mana"));
                a.stamina = (a.stamina + 3.).min(stat(&f.stats, "stamina"));
            }
            let Some(target) = a.attack_target else {
                continue;
            };
            let Some(m) = self
                .mobs
                .iter_mut()
                .find(|m| m.view.id == target && m.view.hp > 0.)
            else {
                a.attack_target = None;
                continue;
            };
            let magical = f.class == 2;
            let range = mortown_protocol::weapons::attack_range(a.equipment.weapon, f.class);
            if mortown_world::distance(point(a), pos(m)) > range
                || !world.segment_clear(point(a), pos(m))
            {
                continue;
            }
            if tick < f.next_attack {
                continue;
            }
            let immortal = a.powers.immortal;
            let resource = if magical { &mut a.mana } else { &mut a.stamina };
            if !pay(immortal, resource, 2.) {
                a.attack_target = None;
                a.combat_status = "Недостаточно ресурса для атаки".into();
                continue;
            }
            let interval_ms = (2000.
                * mortown_protocol::weapons::interval_scale(a.equipment.weapon)
                / (1. + stat(&f.stats, if magical { "WIT" } else { "DEX" }) * 0.01))
                .round()
                .max(500.);
            f.next_attack = tick + (interval_ms / 50.).ceil() as u64;
            f.last_combat = tick;
            let prefix = if magical { "m" } else { "f" };
            let s = |key: &str| stat(&f.stats, &format!("{prefix}_{key}"));
            let (hit, crit) = strike(
                s("attack"),
                m.defence(f.class, tick),
                s("accuracy"),
                m.view.kind.definition().evasion,
                s("crit_chance"),
                s("crit_power"),
                self.random,
            );
            m.view.hp = (m.view.hp - hit).max(0.);
            aggro(a, &mut m.target);
            record_action(
                &mut self.next_action,
                a,
                tick,
                if magical {
                    mortown_protocol::actions::Kind::Spell
                } else {
                    mortown_protocol::actions::Kind::Weapon
                },
                None,
                Some(m.view.position),
            );
            a.combat_status = if hit == 0. {
                "Промах".into()
            } else {
                format!(
                    "{}: {:.0} урона{}",
                    m.view.name,
                    hit,
                    if crit { " · крит!" } else { "" }
                )
            };
            if m.view.hp <= 0. {
                m.defeat(tick);
                a.attack_target = None;
                a.combat_status = format!("{} повержен", m.view.name);
                self.rewards.push(rewards::Defeat {
                    killer: a.id,
                    position: m.view.position,
                    name: m.view.name.clone(),
                    kind: m.view.kind,
                });
            }
            // Award now, before another attack or retaliation can change eligibility.
            self.resolve_rewards(actors, parties);
        }
        for m in &mut self.mobs {
            let spec = m.view.kind.definition();
            let previous_position = pos(m);
            if let Some(attack) = &mut m.view.attack {
                attack.elapsed_ms =
                    (tick.saturating_sub(m.motion_started) * 50).min(u32::MAX as u64) as u32;
                if attack.elapsed_ms >= attack.duration_ms {
                    m.view.attack = None;
                }
            }
            if m.view.hp <= 0. {
                m.cancel_cast(tick);
                m.slow_until = 0;
                m.effects = effects::Effects::default();
                m.view.effects.clear();
            }
            m.view.slow_ms = m.slow_until.saturating_sub(tick) as u32 * 50;
            if m.view.hp <= 0. {
                m.view.death_ms = Some(
                    (tick.saturating_sub(m.respawn_at.saturating_sub(spec.respawn_ticks)) * 50)
                        .min(u32::MAX as u64) as u32,
                );
                if tick >= m.respawn_at {
                    m.view.hp = m.view.max_hp;
                    m.view.position = [
                        m.home[0],
                        world.surface_height(m.home[0], m.home[1]),
                        m.home[1],
                    ];
                    m.view.death_ms = None;
                    m.view.attack = None;
                }
                continue;
            }
            let engaged = m.target.and_then(|id| living(actors, id));
            let invisible_left = engaged.as_ref().is_some_and(|(_, actor)| {
                actor.powers.invisible && mortown_world::distance(pos(m), point(actor)) > 45.
            });
            if engaged.is_none()
                || invisible_left
                || mortown_world::distance(pos(m), m.home) > 45.
                || m.stuck > 100
            {
                m.target = None;
                m.cancel_cast(tick);
                m.stuck = 0;
                m.slow_until = 0;
                m.effects = effects::Effects::default();
            }
            m.effects.sand_until = self
                .areas
                .iter()
                .filter(|area| {
                    mortown_world::distance([area.view.position[0], area.view.position[2]], pos(m))
                        <= area.view.radius
                        && world
                            .segment_clear([area.view.position[0], area.view.position[2]], pos(m))
                })
                .map(|a| a.until)
                .max()
                .unwrap_or(0);
            if m.target.is_none()
                && let Some(area) = self.areas.iter().find(|area| {
                    mortown_world::distance([area.view.position[0], area.view.position[2]], pos(m))
                        <= area.view.radius
                        && world
                            .segment_clear([area.view.position[0], area.view.position[2]], pos(m))
                        && pursued(actors, area.owner).is_some()
                })
            {
                m.target = Some(area.owner);
            }
            let index = m.target.and_then(|id| pursued(actors, id));
            let index = if m.target.is_some() { index } else { None };
            let destination = index.map(|i| point(&actors[i])).unwrap_or(m.home);
            let distance = mortown_world::distance(pos(m), destination);
            let clear_shot = index.is_some() && world.segment_clear(pos(m), destination);
            let can_attack = index.is_some()
                && clear_shot
                && distance <= spec.range
                && m.effects.stun_until <= tick;
            if m.casting
                .is_some_and(|(id, _)| !can_attack || m.target != Some(id))
            {
                m.cancel_cast(tick);
            }
            let stop = if clear_shot { spec.range - 0.3 } else { 0.1 };
            if m.effects.stun_until <= tick
                && m.casting.is_none()
                && m.view.attack.is_none()
                && distance > stop
            {
                let from = pos(m);
                let factor = if tick < m.effects.sand_until {
                    0.5
                } else if tick < m.slow_until {
                    0.6
                } else {
                    1.
                };
                let step =
                    (spec.speed * mortown_protocol::TICK_SECONDS * factor).min(distance - stop);
                let next = [
                    from[0] + (destination[0] - from[0]) / distance * step,
                    from[1] + (destination[1] - from[1]) / distance * step,
                ];
                if world.segment_clear(from, next) {
                    m.view.position = [next[0], world.surface_height(next[0], next[1]), next[1]];
                    m.view.yaw = (next[0] - from[0]).atan2(next[1] - from[1]);
                    m.stuck = 0;
                } else {
                    m.stuck += 1;
                }
            }
            if let Some(i) = index {
                let a = &mut actors[i];
                if mortown_world::distance(point(a), pos(m)) <= spec.range
                    && m.effects.stun_until <= tick
                    && tick >= m.next_attack
                    && world.segment_clear(pos(m), point(a))
                {
                    m.view.yaw = (a.position[0] - m.view.position[0])
                        .atan2(a.position[2] - m.view.position[2]);
                    if m.casting.is_none() {
                        m.begin_attack(a.id, tick);
                    }
                    let (_, until) = m.casting.expect("attack preparation just initialized");
                    if spec.cast_ticks > 0 {
                        m.view.cast = Some(mortown_protocol::monsters::Cast {
                            remaining_ms: until.saturating_sub(tick) as u32 * 50,
                            duration_ms: spec.cast_ticks as u32 * 50,
                        });
                    }
                    let fire = tick >= until;
                    if fire {
                        m.casting = None;
                        m.view.cast = None;
                        if let Some(attack) = &mut m.view.attack {
                            attack.released = true;
                        }
                        let interval = if m.effects.cripple_until > tick {
                            spec.interval * 3 / 2
                        } else {
                            spec.interval
                        };
                        // Melee windup belongs to the old cadence, rather than adding
                        // another delay to every hit. Magic retains its existing cooldown.
                        m.next_attack = tick + interval
                            - if spec.cast_ticks == 0 {
                                m.view.kind.attack_timing().0
                            } else {
                                0
                            };
                        if !a.powers.immortal
                            && let Some(f) = self.fighters.get_mut(&a.id)
                        {
                            f.last_combat = tick;
                            let prefix = if spec.magical { "m" } else { "f" };
                            let (mut hit, crit) = strike(
                                spec.attack,
                                stat(&f.stats, &format!("{prefix}_defence")),
                                spec.accuracy,
                                stat(&f.stats, &format!("{prefix}_evasion")),
                                0.05,
                                1.5,
                                self.random,
                            );
                            let accuracy = m.effects.accuracy_multiplier(tick)
                                * if f.smoke_until > tick { 0.2 } else { 1. };
                            if accuracy < 1. && (self.random)() >= accuracy {
                                hit = 0.;
                            }
                            a.hp = (a.hp - hit).max(0.);
                            if hit > 0. {
                                a.combat_status = format!(
                                    "{}{}: {hit:.0} урона{}",
                                    m.view.name,
                                    if spec.magical {
                                        " · Пепельный заряд"
                                    } else {
                                        ""
                                    },
                                    if crit { " · крит" } else { "" }
                                );
                            }
                            if a.hp <= 0. {
                                a.actions.clear();
                                a.effects.clear();
                                f.smoke_until = 0;
                                a.speed = 0.;
                                a.attack_target = None;
                                f.respawn_at = tick + 200;
                                a.combat_status =
                                    "Вы погибли. R — возродиться через 10 секунд".into();
                            }
                        }
                    }
                }
            } else if m.target.is_none() && distance < 0.2 {
                m.view.hp = m.view.max_hp;
            }
            m.effects
                .dots
                .retain(|d| actors.iter().any(|a| a.id == d.owner && a.hp > 0.));
            let moved = mortown_world::distance(previous_position, pos(m));
            for (owner, hit, kind) in m.effects.advance(tick, moved) {
                if m.view.hp <= 0. {
                    break;
                }
                let Some(a) = actors.iter_mut().find(|a| a.id == owner && a.hp > 0.) else {
                    continue;
                };
                let Some(f) = self.fighters.get_mut(&owner) else {
                    continue;
                };
                f.last_combat = tick;
                m.view.hp = (m.view.hp - hit).max(0.);
                a.combat_status = format!("{}: {hit:.0} урона", kind.name());
                if m.view.hp <= 0. {
                    m.defeat(tick);
                    if a.attack_target == Some(m.view.id) {
                        a.attack_target = None;
                    }
                    a.combat_status
                        .push_str(&format!(" · {} повержен", m.view.name));
                    self.rewards.push(rewards::Defeat {
                        killer: a.id,
                        position: m.view.position,
                        name: m.view.name.clone(),
                        kind: m.view.kind,
                    });
                    rewards::distribute(
                        &mut self.fighters,
                        actors,
                        parties,
                        &mut self.rewards,
                        self.random,
                    );
                }
            }
            m.view.effects = m.effects.views(tick);
            m.view.slow_ms =
                m.slow_until.max(m.effects.sand_until).saturating_sub(tick) as u32 * 50;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::Simulation;
    use std::sync::Arc;
    fn character(hp: f64) -> Character {
        let mut stats = mortown_protocol::starting_stats(1, 1).unwrap();
        stats.insert("current_hp".into(), hp);
        Character {
            id: 1,
            slot: 1,
            nickname: "CombatTest".into(),
            race: 1,
            class: 1,
            gender: 1,
            stats,
            inventory: vec![],
            loot: Default::default(),
        }
    }
    #[test]
    fn secondary_slow_expires_miss_does_not_slow_and_cooldown_survives_rejoin() {
        for class in [1, 2] {
            let world = Arc::new(mortown_world::World::new());
            let mut sim = Simulation::new(world.clone());
            sim.combat.random = || 0.5;
            let m = sim.monsters()[0].position;
            sim.join(1, "Slow".into(), [m[0], m[2]], 0.).unwrap();
            let mut c = character(120.);
            c.class = class;
            c.stats = mortown_protocol::starting_stats(1, class).unwrap();
            sim.configure(&c);
            assert!(sim.use_secondary(1, 1).is_err());
            c.stats.insert("secondary_learned".into(), 1.);
            let before = sim.configure(&c);
            let mut empty = c.clone();
            empty.stats.insert(
                if class == 2 {
                    "current_mana"
                } else {
                    "current_stamina"
                }
                .into(),
                0.,
            );
            sim.configure(&empty);
            assert!(sim.use_secondary(1, 1).is_err());
            sim.configure(&c);
            assert!(sim.use_secondary(1, 999).is_err());
            sim.use_secondary(1, 1).unwrap();
            let after = sim.actor(1).unwrap();
            assert_eq!(after.mana, before.mana - if class == 2 { 8. } else { 0. });
            assert_eq!(
                after.stamina,
                before.stamina - if class == 1 { 8. } else { 0. }
            );
            assert_eq!(sim.monsters()[0].slow_ms, 4000);
            assert_eq!(after.secondary_cooldown_ms, 8000);
            assert!(sim.use_secondary(1, 1).is_err());
            sim.use_skill(1, 1).unwrap(); // independent cooldown
            sim.leave(1);
            sim.join(1, "Slow".into(), [m[0], m[2]], 0.).unwrap();
            sim.configure(&c);
            assert!(sim.use_secondary(1, 1).is_err());
            for _ in 0..80 {
                sim.update();
            }
            assert_eq!(sim.monsters()[0].slow_ms, 0);
            assert!(sim.use_secondary(1, 1).is_err());
            for _ in 0..80 {
                sim.update();
            }
            sim.combat.random = || 0.999;
            sim.use_secondary(1, 1).unwrap();
            assert_eq!(sim.monsters()[0].slow_ms, 0);
        }
    }
    #[test]
    fn slow_reduces_monster_movement_by_forty_percent() {
        let world = Arc::new(mortown_world::World::new());
        let mut combat = Combat::new(&world);
        let home = combat.mobs[0].home;
        // Pick a clear short segment beside the camp.
        let next = [home[0] + 1., home[1]];
        assert!(world.segment_clear(home, next));
        combat.mobs[0].view.position = [next[0], world.surface_height(next[0], next[1]), next[1]];
        // Slow applies during pursuit; returning home clears combat effects by design.
        let mut sim = Simulation::new(world.clone());
        let mut victim = sim.join(1, "Target".into(), home, 0.).unwrap();
        victim.position = [
            home[0] - 5.,
            world.surface_height(home[0] - 5., home[1]),
            home[1],
        ];
        let mut actors = [victim];
        combat.mobs[0].target = Some(1);
        combat.mobs[0].slow_until = 80;
        combat.update(&mut actors, &world, 1, &mut Default::default());
        let slowed = combat.mobs[0].view.position;
        assert!((slowed[0] - (next[0] - 0.12)).abs() < 0.001);
        combat.mobs[0].view.position = [next[0], world.surface_height(next[0], next[1]), next[1]];
        combat.update(&mut actors, &world, 80, &mut Default::default());
        assert!((combat.mobs[0].view.position[0] - (next[0] - 0.2)).abs() < 0.001);
    }
    #[test]
    fn learned_ranks_increase_damage_without_resetting_cooldown() {
        for class in [1, 2] {
            for rank in [1, 2, 3] {
                let world = Arc::new(mortown_world::World::new());
                let mut sim = Simulation::new(world);
                sim.combat.random = || 0.5;
                let mob = sim.monsters()[0].position;
                sim.join(1, "Rank".into(), [mob[0], mob[2]], 0.).unwrap();
                let mut c = character(120.);
                c.class = class;
                c.stats = mortown_protocol::starting_stats(1, class).unwrap();
                c.stats.insert("skill_rank".into(), rank as f64);
                sim.configure(&c);
                sim.use_skill(1, 1).unwrap();
                let skill = mortown_protocol::skills::ranked(class, rank).unwrap();
                let attack = c.stats[if class == 2 { "m_attack" } else { "f_attack" }];
                assert_eq!(
                    sim.monsters()[0].hp,
                    90. - damage(attack * skill.multiplier, 10.)
                );
                let mut a = sim.actor(1).unwrap();
                if rank < 3 {
                    a.progress.level = 3;
                    a.progress.sp = 12.;
                    crate::economy::learn(&mut a, rank + 1).unwrap();
                    sim.replace_actor(a);
                }
                assert!(sim.use_skill(1, 1).is_err());
                assert_eq!(sim.actor(1).unwrap().skill_cooldown_ms, 2000);
            }
        }
    }
    #[test]
    fn loot_rolls_class_weapons_and_stacks_without_equipping() {
        for class in [1, 2] {
            let world = Arc::new(mortown_world::World::new());
            let mut sim = Simulation::new(world);
            sim.join(1, "Loot".into(), mortown_world::SPAWN, 0.)
                .unwrap();
            let mut c = character(120.);
            c.class = class;
            let mut actor = sim.configure(&c);
            let fighter = sim.combat.fighters.get_mut(&1).unwrap();
            grant_reward(&mut actor, fighter, || 0.);
            grant_reward(&mut actor, fighter, || 0.);
            assert_eq!(actor.loot.len(), 4);
            assert_eq!(actor.loot[&mortown_protocol::crafting::LEATHER], 2);
            assert_eq!(actor.loot[&7], 2);
            assert_eq!(actor.loot[&6], 2);
            assert_eq!(actor.loot[&if class == 1 { 4 } else { 5 }], 2);
            let mut items = mortown_protocol::items::starter(class);
            mortown_protocol::items::include_loot(&mut items, &actor.loot);
            mortown_protocol::items::include_loot(&mut items, &actor.loot);
            assert_eq!(items.len(), 6);
            assert!(items.iter().all(|i| !i.equipped));
            assert!(mortown_protocol::items::equip(&mut items, class, 7, true).is_err());
            mortown_protocol::items::equip(&mut items, class, if class == 1 { 4 } else { 5 }, true)
                .unwrap();
            assert_eq!(items.iter().filter(|i| i.equipped).count(), 1);
        }
    }
    #[test]
    fn rewards_only_go_to_killer_once_for_skill_and_autoattack() {
        for skill in [false, true] {
            let world = Arc::new(mortown_world::World::new());
            let mut sim = Simulation::new(world);
            sim.combat.random = || 0.5;
            let m = sim.monsters()[0].position;
            for id in [1, 2] {
                sim.join(id, "Reward".into(), [m[0], m[2]], 0.).unwrap();
                let mut c = character(120.);
                c.id = id;
                c.stats.insert("current_exp".into(), 75.);
                sim.configure(&c);
            }
            sim.combat.mobs[0].view.hp = 1.;
            if skill {
                sim.use_skill(1, 1).unwrap();
            } else {
                sim.attack(1, Some(1)).unwrap();
                sim.update();
            }
            let winner = sim.actor(1).unwrap();
            assert_eq!(winner.loot.get(&7), Some(&1));
            assert_eq!(winner.progress.kills, 1);
            assert_eq!(winner.progress.level, 2);
            assert_eq!(winner.progress.coins, 512.);
            assert_eq!(winner.max_hp, 134.);
            assert_eq!(winner.hp, winner.max_hp);
            assert!(sim.actor(2).unwrap().loot.is_empty());
            assert_eq!(sim.actor(2).unwrap().progress.kills, 0);
            assert!(sim.use_skill(2, 1).is_err());
            for _ in 0..50 {
                sim.update();
            }
            assert_eq!(sim.actor(1).unwrap().loot, winner.loot);
            assert_eq!(sim.actor(1).unwrap().progress, winner.progress);
        }
    }
    #[test]
    fn skills_validate_targets_resources_and_keep_cooldown_across_rejoin() {
        for class in [1, 2] {
            let world = Arc::new(mortown_world::World::new());
            let mut sim = Simulation::new(world);
            sim.combat.random = || 0.5;
            let mut c = character(120.);
            c.class = class;
            c.stats = mortown_protocol::starting_stats(1, class).unwrap();
            sim.join(1, "Skills".into(), mortown_world::SPAWN, 0.)
                .unwrap();
            sim.configure(&c);
            assert!(sim.use_skill(1, 1).is_err()); // Out of range.
            sim.leave(1);
            let m = sim.monsters()[0].position;
            sim.join(1, "Skills".into(), [m[0], m[2]], 0.).unwrap();
            let before = sim.configure(&c);
            assert!(sim.use_skill(1, 999).is_err());
            let mut empty = c.clone();
            empty.stats.insert(
                if class == 2 {
                    "current_mana"
                } else {
                    "current_stamina"
                }
                .into(),
                0.,
            );
            sim.configure(&empty);
            assert!(sim.use_skill(1, 1).is_err());
            assert_eq!(sim.monsters()[0].hp, 90.);
            sim.configure(&c);
            sim.use_skill(1, 1).unwrap();
            let after = sim.actor(1).unwrap();
            let skill = mortown_protocol::skills::starter(class).unwrap();
            let attack = c.stats[if class == 2 { "m_attack" } else { "f_attack" }];
            assert_eq!(
                sim.monsters()[0].hp,
                90. - damage(attack * skill.multiplier, 10.)
            );
            assert_eq!(after.mana, before.mana - if class == 2 { 5. } else { 0. });
            assert_eq!(
                after.stamina,
                before.stamina - if class == 1 { 5. } else { 0. }
            );
            for _ in 0..100 {
                assert!(sim.use_skill(1, 1).is_err());
            }
            assert_eq!(sim.actor(1).unwrap().skill_cooldown_ms, 2000);
            sim.leave(1);
            sim.join(1, "Skills".into(), [m[0], m[2]], 0.).unwrap();
            sim.configure(&c);
            assert!(sim.use_skill(1, 1).is_err());
            for _ in 0..39 {
                sim.update();
            }
            assert!(sim.use_skill(1, 1).is_err());
            sim.update();
            sim.use_skill(1, 1).unwrap();
            let mut dead = c.clone();
            dead.stats.insert("current_hp".into(), 0.);
            sim.configure(&dead);
            assert!(sim.use_skill(1, 1).is_err());
        }
    }
    #[test]
    fn legacy_damage_hit_probability_and_cooldown() {
        assert_eq!(damage(28., 24.), 19.);
        assert_eq!(damage(100., 50.), 50.);
        assert_eq!(damage(1., 1000.), 1.);
        assert!((hit_chance(56., 40.) - 0.96).abs() < 1e-9);
        assert_eq!(hit_chance(0., 100.), 0.1);
        assert_eq!(hit_chance(100., 0.), 0.99);
        let world = Arc::new(mortown_world::World::new());
        let mut sim = Simulation::new(world.clone());
        sim.join(1, "CombatTest".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        sim.configure(&character(120.));
        assert!(sim.attack(1, Some(1)).is_err());
        assert!(sim.attack(1, Some(999)).is_err());
        sim.leave(1);
        let m = sim.monsters()[0].position;
        sim.join(1, "CombatTest".into(), [m[0], m[2]], 0.).unwrap();
        let before = sim.configure(&character(120.));
        for _ in 0..1000 {
            sim.attack(1, Some(1)).unwrap();
        }
        sim.update();
        assert_eq!(sim.actor(1).unwrap().stamina, before.stamina - 2.);
        for _ in 0..10 {
            sim.attack(1, Some(1)).unwrap();
            sim.update();
        }
        assert_eq!(sim.actor(1).unwrap().stamina, before.stamina - 2.);
        for m in sim.monsters() {
            assert!(world.walkable(m.position[0], m.position[2]));
        }
    }
    #[test]
    fn dead_characters_cannot_move_or_attack_and_respawn_requires_delay() {
        let world = Arc::new(mortown_world::World::new());
        let mut sim = Simulation::new(world);
        sim.join(1, "CombatTest".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        let dead = sim.configure(&character(0.));
        assert!(sim.attack(1, Some(1)).is_err());
        assert!(sim.respawn(1).is_err());
        sim.input(
            1,
            1,
            [mortown_world::SPAWN[0] + 10., mortown_world::SPAWN[1]],
            false,
        )
        .unwrap();
        for _ in 0..200 {
            sim.update();
        }
        assert_eq!(sim.actor(1).unwrap().position, dead.position);
        sim.respawn(1).unwrap();
        let alive = sim.actor(1).unwrap();
        assert_eq!(alive.hp, 120.);
        assert_eq!(alive.stamina, 115.);
        assert!(sim.respawn(1).is_err());
    }
    #[test]
    fn retaliation_kills_and_camp_has_a_route_from_the_city() {
        let world = Arc::new(mortown_world::World::new());
        let mut sim = Simulation::new(world.clone());
        sim.combat.random = || 0.5;
        let m = sim.monsters()[0].position;
        let route =
            mortown_world::navigation::find_path(&world, mortown_world::SPAWN, [m[0], m[2]])
                .unwrap();
        assert!(!route.is_empty());
        sim.join(1, "CombatTest".into(), [m[0], m[2]], 0.).unwrap();
        sim.configure(&character(1.));
        sim.attack(1, Some(1)).unwrap();
        sim.update();
        assert_eq!(sim.actor(1).unwrap().hp, 1.);
        assert!(!sim.monsters()[0].attack.unwrap().released);
        let windup = Kind::Bandit.attack_timing().0;
        for _ in 1..windup {
            sim.update();
            assert_eq!(sim.actor(1).unwrap().hp, 1.);
        }
        sim.update();
        assert!(sim.monsters()[0].attack.unwrap().released);
        assert_eq!(sim.actor(1).unwrap().hp, 0.);
        assert!(sim.respawn(1).is_err());
        for _ in 0..200 {
            sim.update();
        }
        sim.respawn(1).unwrap();
        assert_eq!(sim.actor(1).unwrap().hp, 120.);
    }
    #[test]
    fn defeated_bandit_respawns_once_and_only_after_delay() {
        let world = mortown_world::World::new();
        let mut combat = Combat::new(&world);
        combat.mobs[0].view.hp = 0.;
        combat.mobs[0].respawn_at = 600;
        combat.update(&mut [], &world, 599, &mut Default::default());
        assert_eq!(combat.mobs[0].view.hp, 0.);
        combat.update(&mut [], &world, 600, &mut Default::default());
        assert_eq!(combat.mobs[0].view.hp, 90.);
        combat.update(&mut [], &world, 601, &mut Default::default());
        assert_eq!(combat.mobs.len(), 7);
    }
    #[test]
    fn invisible_drops_pursuit_and_immortal_ignores_the_strike() {
        let world = Arc::new(mortown_world::World::new());
        let mut sim = Simulation::new(world.clone());
        sim.combat.random = || 0.;
        let mob = sim.monsters()[0].position;
        sim.join(1, "Gm".into(), [mob[0], mob[2]], 0.).unwrap();
        sim.configure(&character(100.));
        let mut actor = sim.actor(1).unwrap();
        let hp = actor.hp;
        actor.powers.invisible = true;
        sim.replace_actor(actor);
        sim.combat.mobs[0].target = Some(1);
        sim.update();
        assert_eq!(sim.combat.mobs[0].target, Some(1));
        assert_eq!(sim.actor(1).unwrap().hp, hp);
        let mut actor = sim.actor(1).unwrap();
        actor.powers.invisible = false;
        actor.powers.immortal = true;
        sim.replace_actor(actor);
        sim.combat.mobs[0].target = Some(1);
        let windup = Kind::Bandit.attack_timing().0;
        for _ in 0..=windup {
            sim.update();
        }
        assert_eq!(sim.actor(1).unwrap().hp, hp);
        assert!(sim.combat.mobs[0].target == Some(1));
    }
    #[test]
    fn invisible_hits_accumulate_until_the_attacker_leaves() {
        let world = Arc::new(mortown_world::World::new());
        let mut sim = Simulation::new(world);
        sim.combat.random = || 0.;
        let mob = sim.monsters()[0].position;
        sim.join(1, "Gm".into(), [mob[0], mob[2]], 0.).unwrap();
        let before = sim.configure(&character(120.));
        let mut actor = sim.actor(1).unwrap();
        actor.powers.invisible = true;
        sim.replace_actor(actor);
        sim.attack(1, Some(1)).unwrap();
        sim.update();
        let wounded = sim.monsters()[0].hp;
        assert!(wounded > 0. && wounded < 90.);
        assert_eq!(sim.actor(1).unwrap().stamina, before.stamina - 2.);
        let mut later = wounded;
        for _ in 0..80 {
            sim.update();
            later = sim.monsters()[0].hp;
            if later < wounded {
                break;
            }
        }
        assert!(later < wounded && later < 90.);
        let mut actor = sim.actor(1).unwrap();
        actor.position = [
            mob[0] + 50.,
            sim.terrain().surface_height(mob[0] + 50., mob[2]),
            mob[2],
        ];
        sim.replace_actor(actor);
        sim.update();
        assert!(sim.combat.mobs[0].target.is_none());
        assert_eq!(sim.monsters()[0].hp, sim.monsters()[0].max_hp);
    }
    #[test]
    fn immortal_actions_do_not_spend_mana_or_stamina() {
        let world = Arc::new(mortown_world::World::new());
        let mut sim = Simulation::new(world);
        sim.combat.random = || 0.;
        let mob = sim.monsters()[0].position;
        sim.join(1, "Gm".into(), [mob[0], mob[2]], 0.).unwrap();
        sim.configure(&character(120.));
        let mut actor = sim.actor(1).unwrap();
        actor.powers.immortal = true;
        actor.stamina = 0.;
        actor.mana = 0.;
        actor.progress.extra_skills = 1 << 3;
        sim.replace_actor(actor);
        sim.attack(1, Some(1)).unwrap();
        sim.update();
        assert!(sim.monsters()[0].hp < 90.);
        assert_eq!(sim.actor(1).unwrap().stamina, 0.);
        sim.use_skill(1, 1).unwrap();
        assert_eq!(sim.actor(1).unwrap().stamina, 0.);
        sim.use_ability(1, mortown_protocol::skills::RACIAL_SKILL, None)
            .unwrap();
        let actor = sim.actor(1).unwrap();
        assert_eq!(actor.stamina, 0.);
        assert_eq!(actor.mana, 0.);
    }
}
