use bevy_ecs::prelude::*;
use mortown_protocol::{Actor, RUN_SPEED, TICK_SECONDS, WALK_SPEED};
use std::{collections::HashMap, sync::Arc};

#[derive(Component)]
struct Adventurer {
    actor: Actor,
    target: [f32; 2],
    walking: bool,
    seq: u64,
    last_input: u64,
}
#[derive(Resource)]
struct Terrain(Arc<mortown_world::World>);
#[derive(Resource, Default)]
struct Tick(u64);
pub struct Simulation {
    pub parties: crate::party::Parties,
    pub exchanges: crate::exchange::Exchanges,
    world: World,
    schedule: Schedule,
    entities: HashMap<i64, Entity>,
    pub residents: Vec<mortown_protocol::admin::NpcState>,
    pub next_npc: u16,
    pub combat: crate::combat::Combat,
}
impl Simulation {
    pub fn new(terrain: Arc<mortown_world::World>) -> Self {
        let combat = crate::combat::Combat::new(&terrain);
        let residents = mortown_world::npcs::RESIDENTS
            .iter()
            .map(|n| mortown_protocol::admin::NpcState {
                id: n.id,
                template: n.id,
                position: n.feet(&terrain),
                yaw: n.yaw(),
                hp: 100.,
            })
            .collect();
        let mut world = World::new();
        world.insert_resource(Terrain(terrain));
        world.init_resource::<Tick>();
        let mut schedule = Schedule::default();
        schedule.set_executor_kind(bevy_ecs::schedule::ExecutorKind::SingleThreaded);
        schedule.add_systems(advance);
        Self {
            exchanges: Default::default(),
            parties: Default::default(),
            world,
            schedule,
            entities: HashMap::new(),
            residents,
            next_npc: 8,
            combat,
        }
    }
    pub fn resident(&self, id: u8) -> Option<mortown_world::npcs::Npc> {
        let state = self.residents.iter().find(|n| n.id == id && n.hp > 0.)?;
        Some(mortown_world::npcs::definition(state.template)?.at(
            state.id,
            state.position,
            state.yaw,
        ))
    }
    pub fn teleport(&mut self, id: i64, p: [f32; 3]) -> Result<(), &'static str> {
        if !p.iter().all(|n| n.is_finite()) || !self.terrain().walkable(p[0], p[2]) {
            return Err("Недоступная точка");
        }
        let entity = *self.entities.get(&id).ok_or("Игрок не в мире")?;
        let mut a = self.world.get_mut::<Adventurer>(entity).unwrap();
        a.actor.position = p;
        a.target = [p[0], p[2]];
        a.actor.attack_target = None;
        Ok(())
    }
    pub fn click_teleport(&mut self, id: i64, target: [f32; 2]) -> Result<(), &'static str> {
        let actor = self.actor(id).ok_or("Персонаж не в мире")?;
        if !actor.powers.click_teleport {
            return Err("Режим телепорта по клику выключен");
        }
        if actor.hp <= 0. {
            return Err("Нельзя телепортироваться мёртвым");
        }
        if !target
            .iter()
            .all(|p| p.is_finite() && p.abs() < mortown_world::WORLD_HALF)
        {
            return Err("Недоступная точка");
        }
        let world = self.terrain();
        let point = [
            target[0],
            world.surface_height(target[0], target[1]),
            target[1],
        ];
        self.exchanges.cancel(id, "Обмен отменён: телепорт");
        self.teleport(id, point)
    }
    pub fn join(
        &mut self,
        id: i64,
        nickname: String,
        mut pos: [f32; 2],
        yaw: f32,
    ) -> Result<Actor, &'static str> {
        if self.entities.contains_key(&id) {
            return Err("Этот персонаж уже в игре");
        }
        if self.entities.len() >= 64 {
            return Err("Сервер прототипа заполнен");
        }
        let world = &self.world.resource::<Terrain>().0;
        if !world.walkable(pos[0], pos[1]) {
            pos = mortown_world::SPAWN;
        }
        let occupied = |point: [f32; 2]| {
            self.entities.values().any(|entity| {
                let p = &self
                    .world
                    .get::<Adventurer>(*entity)
                    .unwrap()
                    .actor
                    .position;
                (p[0] - point[0]).hypot(p[2] - point[1]) < 1.5
            })
        };
        if occupied(pos) {
            for i in 0..16 {
                let angle = i as f32 * std::f32::consts::TAU / 16.;
                let candidate = [pos[0] + angle.cos() * 3., pos[1] + angle.sin() * 3.];
                if !occupied(candidate) && world.segment_clear(pos, candidate) {
                    pos = candidate;
                    break;
                }
            }
        }
        let actor = Actor {
            id,
            nickname,
            equipment: Default::default(),
            appearance: Default::default(),
            position: [pos[0], world.surface_height(pos[0], pos[1]), pos[1]],
            yaw,
            speed: 0.,
            hp: 100.,
            max_hp: 100.,
            mana: 0.,
            stamina: 0.,
            respawn_seconds: 0,
            attack_target: None,
            actions: vec![],
            combat_status: String::new(),
            skill_cooldown_ms: 0,
            secondary_cooldown_ms: 0,
            ability_cooldowns_ms: [0; mortown_protocol::skills::EXTRA_COOLDOWNS],
            effects: vec![],
            progress: mortown_protocol::progression::Progress::from_stats(&Default::default()),
            powers: Default::default(),
            reward_status: String::new(),
            loot: Default::default(),
        };
        let entity = self
            .world
            .spawn(Adventurer {
                actor: actor.clone(),
                target: pos,
                walking: false,
                seq: 0,
                last_input: 0,
            })
            .id();
        self.entities.insert(id, entity);
        Ok(actor)
    }
    pub fn input(
        &mut self,
        id: i64,
        seq: u64,
        target: [f32; 2],
        walking: bool,
    ) -> Result<(), &'static str> {
        if !target
            .iter()
            .all(|p| p.is_finite() && p.abs() < mortown_world::WORLD_HALF)
        {
            return Err("Некорректная цель движения");
        }
        let tick = self.world.resource::<Tick>().0;
        let entity = *self.entities.get(&id).ok_or("Персонаж не в мире")?;
        let mut p = self.world.get_mut::<Adventurer>(entity).unwrap();
        if seq <= p.seq {
            return Err("Устаревшая команда движения");
        }
        p.seq = seq;
        p.target = target;
        p.walking = walking;
        p.last_input = tick;
        Ok(())
    }
    pub fn update(&mut self) {
        self.parties.expire();
        self.schedule.run(&mut self.world);
        let mut actors = self.snapshot();
        let terrain = self.world.resource::<Terrain>().0.clone();
        let tick = self.tick();
        self.combat
            .update(&mut actors, &terrain, tick, &mut self.parties);
        for actor in actors {
            let entity = self.entities[&actor.id];
            self.world.get_mut::<Adventurer>(entity).unwrap().actor = actor;
        }
    }
    pub fn tick(&self) -> u64 {
        self.world.resource::<Tick>().0
    }
    pub fn actor(&self, id: i64) -> Option<Actor> {
        self.entities
            .get(&id)
            .and_then(|e| self.world.get::<Adventurer>(*e))
            .map(|p| p.actor.clone())
    }
    pub fn snapshot(&self) -> Vec<Actor> {
        let mut actors: Vec<_> = self
            .entities
            .keys()
            .filter_map(|id| self.actor(*id))
            .collect();
        actors.sort_by_key(|a| a.id);
        actors
    }
    pub fn configure(&mut self, c: &mortown_protocol::Character) -> Actor {
        let mut actor = self.actor(c.id).unwrap();
        self.combat.configure(c, &mut actor, self.tick());
        self.world
            .get_mut::<Adventurer>(self.entities[&c.id])
            .unwrap()
            .actor = actor.clone();
        self.parties.reconnect(&actor);
        actor
    }
    /// Refresh equipment without resetting health, effects, cooldowns or attack timing.
    pub fn refresh_character(&mut self, c: &mortown_protocol::Character) {
        self.combat.refresh(c);
        if let Some(entity) = self.entities.get(&c.id) {
            self.world
                .get_mut::<Adventurer>(*entity)
                .unwrap()
                .actor
                .equipment =
                mortown_protocol::items::EquipmentAppearance::from_inventory(&c.inventory);
        }
    }
    pub fn attack(&mut self, id: i64, target: Option<u32>) -> Result<(), &'static str> {
        let mut actor = self.actor(id).ok_or("Персонаж не найден")?;
        actor.attack_target = None;
        let result = self
            .combat
            .select(&mut actor, target, &self.world.resource::<Terrain>().0);
        if let Err(message) = result {
            actor.combat_status = message.into();
        }
        self.world
            .get_mut::<Adventurer>(self.entities[&id])
            .unwrap()
            .actor = actor;
        result
    }
    pub fn use_skill(&mut self, id: i64, target: u32) -> Result<(), &'static str> {
        let mut actor = self.actor(id).ok_or("Персонаж не найден")?;
        let terrain = self.world.resource::<Terrain>().0.clone();
        let result = self
            .combat
            .use_skill(&mut actor, target, &terrain, self.tick());
        if let Err(message) = result {
            actor.combat_status = message.into();
        }
        self.world
            .get_mut::<Adventurer>(self.entities[&id])
            .unwrap()
            .actor = actor;
        self.resolve_rewards();
        result
    }
    pub fn use_secondary(&mut self, id: i64, target: u32) -> Result<(), &'static str> {
        let mut actor = self.actor(id).ok_or("Персонаж не найден")?;
        let terrain = self.world.resource::<Terrain>().0.clone();
        let result = self
            .combat
            .use_secondary(&mut actor, target, &terrain, self.tick());
        if let Err(message) = result {
            actor.combat_status = message.into();
        }
        self.replace_actor(actor);
        self.resolve_rewards();
        result
    }
    pub fn in_combat(&self, id: i64) -> bool {
        self.combat.in_combat(id, self.tick())
    }
    pub fn terrain(&self) -> std::sync::Arc<mortown_world::World> {
        self.world.resource::<Terrain>().0.clone()
    }
    pub fn use_ability(
        &mut self,
        id: i64,
        skill: u8,
        target: Option<u32>,
    ) -> Result<(), &'static str> {
        let mut actor = self.actor(id).ok_or("Персонаж не найден")?;
        let terrain = self.world.resource::<Terrain>().0.clone();
        let result = self
            .combat
            .use_ability(&mut actor, skill, target, &terrain, self.tick());
        if let Err(message) = result {
            actor.combat_status = message.into();
        }
        self.replace_actor(actor);
        self.resolve_rewards();
        result
    }
    pub fn areas(&self) -> Vec<mortown_protocol::skills::Area> {
        self.combat.areas(self.tick())
    }
    pub fn replace_actor(&mut self, actor: Actor) {
        if let Some(entity) = self.entities.get(&actor.id) {
            self.world.get_mut::<Adventurer>(*entity).unwrap().actor = actor;
        }
    }
    fn resolve_rewards(&mut self) {
        let mut actors = self.snapshot();
        self.combat.resolve_rewards(&mut actors, &mut self.parties);
        for actor in actors {
            self.replace_actor(actor);
        }
    }
    pub fn respawn(&mut self, id: i64) -> Result<(), &'static str> {
        let mut actor = self.actor(id).ok_or("Персонаж не найден")?;
        let terrain = self.world.resource::<Terrain>().0.clone();
        self.combat.respawn(&mut actor, &terrain, self.tick())?;
        let mut p = self
            .world
            .get_mut::<Adventurer>(self.entities[&id])
            .unwrap();
        p.target = mortown_world::SPAWN;
        p.actor = actor;
        Ok(())
    }
    pub fn monsters(&self) -> Vec<mortown_protocol::Monster> {
        self.combat.mobs.iter().map(|m| m.view.clone()).collect()
    }
    pub fn leave(&mut self, id: i64) {
        if let Some(actor) = self.actor(id) {
            self.parties.disconnect(&actor);
        }
        self.combat.leave(id);
        if let Some(e) = self.entities.remove(&id) {
            self.world.despawn(e);
        }
    }
}
fn advance(world: Res<Terrain>, mut tick: ResMut<Tick>, mut players: Query<&mut Adventurer>) {
    tick.0 += 1;
    for mut p in &mut players {
        p.actor.speed = 0.;
        if p.actor.hp <= 0. || tick.0.saturating_sub(p.last_input) > 10 {
            continue;
        }
        let from = [p.actor.position[0], p.actor.position[2]];
        let dx = p.target[0] - from[0];
        let dz = p.target[1] - from[1];
        let distance = dx.hypot(dz);
        if distance < 0.001 {
            continue;
        }
        let step = (if p.walking { WALK_SPEED } else { RUN_SPEED } * TICK_SECONDS).min(distance);
        let desired = [
            from[0] + dx / distance * step,
            from[1] + dz / distance * step,
        ];
        let next = if world.0.segment_clear(from, desired) {
            desired
        } else {
            [[desired[0], from[1]], [from[0], desired[1]]]
                .into_iter()
                .find(|candidate| world.0.segment_clear(from, *candidate))
                .unwrap_or(from)
        };
        let moved = (next[0] - from[0]).hypot(next[1] - from[1]);
        if moved > 0.0001 {
            p.actor.yaw = (next[0] - from[0]).atan2(next[1] - from[1]);
            p.actor.speed = moved / TICK_SECONDS;
            p.actor.position = [next[0], world.0.surface_height(next[0], next[1]), next[1]];
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flood_cannot_increase_speed_and_stale_input_stops() {
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let p = sim
            .join(1, "Test".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        for seq in 1..=1000 {
            sim.input(1, seq, [p.position[0] + 50., p.position[2]], false)
                .unwrap();
        }
        sim.update();
        assert!((sim.actor(1).unwrap().position[0] - p.position[0] - 0.5).abs() < 0.001);
        assert!(sim.input(1, 1000, [0., 0.], false).is_err());
        assert!(sim.input(1, 1001, [f32::NAN, 0.], false).is_err());
        for _ in 0..20 {
            sim.update();
        }
        let stopped = sim.actor(1).unwrap().position;
        sim.update();
        assert_eq!(stopped, sim.actor(1).unwrap().position);
    }
    #[test]
    fn authority_blocks_walls_and_double_login() {
        let world = Arc::new(mortown_world::World::new());
        let wall = world.obstacles[0];
        let start = mortown_world::rotate(wall.center, 0., 12., wall.yaw);
        let end = mortown_world::rotate(wall.center, 0., -12., wall.yaw);
        let mut sim = Simulation::new(world.clone());
        sim.join(1, "Test".into(), start, 0.).unwrap();
        assert!(sim.join(1, "Test".into(), start, 0.).is_err());
        for seq in 1..200 {
            sim.input(1, seq, end, false).unwrap();
            sim.update();
        }
        let p = sim.actor(1).unwrap().position;
        assert!(world.walkable(p[0], p[2]));
        assert!((p[0] - end[0]).hypot(p[2] - end[1]) > 5.);
    }
    #[test]
    fn click_teleport_stays_off_until_the_admin_mode_is_enabled() {
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        sim.join(1, "Gm".into(), mortown_world::SPAWN, 0.).unwrap();
        assert!(
            sim.click_teleport(1, mortown_world::SPAWN)
                .unwrap_err()
                .contains("выключен")
        );
        let mut actor = sim.actor(1).unwrap();
        actor.powers.click_teleport = true;
        sim.replace_actor(actor);
        sim.click_teleport(1, mortown_world::SPAWN).unwrap();
        let placed = sim.actor(1).unwrap().position;
        assert!((placed[0] - mortown_world::SPAWN[0]).abs() < 0.001);
        assert!((placed[2] - mortown_world::SPAWN[1]).abs() < 0.001);
        assert!(sim.click_teleport(1, [f32::NAN, 0.]).is_err());
        let mut actor = sim.actor(1).unwrap();
        actor.hp = 0.;
        sim.replace_actor(actor);
        assert!(sim.click_teleport(1, mortown_world::SPAWN).is_err());
    }
}
