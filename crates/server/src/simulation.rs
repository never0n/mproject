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
    offline_age: u64,
}
#[derive(Resource)]
struct Terrain(Arc<mortown_world::World>);
#[derive(Resource, Default)]
struct Tick(u64);
#[derive(Resource, Default, Clone)]
pub struct Blocks(pub Vec<mortown_protocol::work::Block>);
pub struct Simulation {
    pub parties: crate::party::Parties,
    pub exchanges: crate::exchange::Exchanges,
    world: World,
    schedule: Schedule,
    entities: HashMap<i64, Entity>,
    pub combat: crate::combat::Combat,
}
impl Simulation {
    pub fn new(terrain: Arc<mortown_world::World>) -> Self {
        let combat = crate::combat::Combat::new(&terrain);
        let mut world = World::new();
        world.insert_resource(Terrain(terrain));
        world.init_resource::<Tick>();
        world.init_resource::<Blocks>();
        let mut schedule = Schedule::default();
        schedule.set_executor_kind(bevy_ecs::schedule::ExecutorKind::SingleThreaded);
        schedule.add_systems(advance);
        Self {
            exchanges: Default::default(),
            parties: Default::default(),
            world,
            schedule,
            entities: HashMap::new(),
            combat,
        }
    }
    pub fn set_blocks(&mut self, blocks: Vec<mortown_protocol::work::Block>) {
        self.world.insert_resource(Blocks(blocks));
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
            let mut actor = self.actor(id).unwrap();
            if !actor.offline {
                return Err("Этот персонаж уже в игре");
            }
            actor.offline = false;
            let feet = [actor.position[0], actor.position[2]];
            let entity = self.entities[&id];
            let mut body = self.world.get_mut::<Adventurer>(entity).unwrap();
            body.actor = actor.clone();
            // The client starts its move counter again. The previous session's counter
            // would reject the first step and drop the connection.
            body.seq = 0;
            body.target = feet;
            body.walking = false;
            body.last_input = 0;
            body.offline_age = 0;
            return Ok(actor);
        }
        if self.entities.len() >= 64 {
            return Err("Сервер прототипа заполнен");
        }
        let world = &self.world.resource::<Terrain>().0;
        if !world.walkable(pos[0], pos[1]) {
            // [PROPOSAL] A blocked saved cell moves to the nearest walkable cell within 6 m,
            // not across the city. Spawn is only the last resort.
            let mut found = None;
            for ring in 1..12 {
                for step in 0..16 {
                    let angle = step as f32 * std::f32::consts::TAU / 16.;
                    let candidate = [
                        pos[0] + angle.cos() * ring as f32 * 0.5,
                        pos[1] + angle.sin() * ring as f32 * 0.5,
                    ];
                    if world.walkable(candidate[0], candidate[1]) {
                        found = Some(candidate);
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            pos = found.unwrap_or(mortown_world::SPAWN);
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
            downed: false,
            bound: false,
            offline: false,
            lives: mortown_protocol::life::lives(),
            return_seconds: 0,
            death_mark: 0,
            pending_death: false,
            orders: mortown_protocol::life::Orders::default(),
            post: pos,
            return_until: 0,
            hunger: 0.,
            thirst: 0.,
            presented: String::new(),
            face_hidden: false,
            identity: 1.,
        };
        let entity = self
            .world
            .spawn(Adventurer {
                actor: actor.clone(),
                target: pos,
                walking: false,
                seq: 0,
                last_input: 0,
                offline_age: 0,
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
    /// Drops and coins earned since the last call, to be stored as stacks by the owner's session.
    pub fn take_grants(&mut self, id: i64) -> Vec<(u8, u32)> {
        self.combat.grants.remove(&id).unwrap_or_default()
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
                .equipment = mortown_protocol::items::EquipmentAppearance::from_items(&c.items);
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
    /// Puts the body at `at` and keeps it there. Used when a test must stand on a source.
    pub fn pin(&mut self, id: i64, at: [f32; 2]) {
        let tick = self.tick();
        let height = self
            .world
            .resource::<Terrain>()
            .0
            .surface_height(at[0], at[1]);
        let entity = self.entities[&id];
        let mut body = self.world.get_mut::<Adventurer>(entity).unwrap();
        body.actor.position = [at[0], height, at[1]];
        body.target = at;
        body.last_input = tick;
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
        self.combat.respawn(&mut actor, self.tick())?;
        let mut p = self
            .world
            .get_mut::<Adventurer>(self.entities[&id])
            .unwrap();
        p.actor = actor;
        Ok(())
    }
    pub fn monsters(&self) -> Vec<mortown_protocol::Monster> {
        self.combat.mobs.iter().map(|m| m.view.clone()).collect()
    }
    /// The connection ended. The same body stays, now under offline orders.
    pub fn disconnect(&mut self, id: i64) {
        if let Some(actor) = self.actor(id) {
            self.parties.disconnect(&actor);
        }
        let tick = self.tick();
        let Some(entity) = self.entities.get(&id).copied() else {
            return;
        };
        if let Some(mut body) = self.world.get_mut::<Adventurer>(entity) {
            body.actor.offline = true;
            body.actor.post = [body.actor.position[0], body.actor.position[2]];
            body.target = body.actor.post;
            body.last_input = tick;
            body.offline_age = 0;
        }
    }
    pub fn still_offline(&self, id: i64) -> bool {
        self.actor(id).is_some_and(|actor| actor.offline)
    }
    pub fn expired_offline(&self) -> Vec<Actor> {
        let limit = mortown_protocol::life::offline_ticks();
        self.entities
            .iter()
            .filter_map(|(_, entity)| {
                self.world.get::<Adventurer>(*entity).and_then(|body| {
                    (body.actor.offline && body.offline_age >= limit).then(|| body.actor.clone())
                })
            })
            .collect()
    }
    pub fn finish_death(&mut self, sheet: &mortown_protocol::Character) {
        if !self.entities.contains_key(&sheet.id) {
            return;
        }
        self.exchanges
            .cancel(sheet.id, "Обмен отменён: один из участников умер");
        self.configure(sheet);
        let point = mortown_world::SPAWN;
        let height = self
            .world
            .resource::<Terrain>()
            .0
            .surface_height(point[0], point[1]);
        let Some(entity) = self.entities.get(&sheet.id).copied() else {
            return;
        };
        let Some(mut body) = self.world.get_mut::<Adventurer>(entity) else {
            return;
        };
        if sheet.stats.get("ended").copied() == Some(1.) {
            body.actor.combat_status = "Жизни этого человека закончились.".into();
            return;
        }
        body.actor.position = [point[0], height, point[1]];
        body.target = point;
        body.actor.return_until = sheet.stats.get("return_until").copied().unwrap_or(0.) as u64;
        body.actor.identity = sheet.stats.get("identity").copied().unwrap_or(1.);
        body.actor.presented.clear();
        body.actor.face_hidden = false;
        body.actor.combat_status = "Новая личность. Пока нельзя покинуть это место.".into();
    }
    pub fn mark_pending(&mut self, id: i64) {
        let Some(entity) = self.entities.get(&id).copied() else {
            return;
        };
        if let Some(mut body) = self.world.get_mut::<Adventurer>(entity) {
            body.actor.pending_death = true;
        }
    }
    /// Drops an offline body. An online character is left alone.
    pub fn retire_offline(&mut self, ids: &[i64]) {
        let gone: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| self.actor(*id).is_some_and(|actor| actor.offline))
            .collect();
        for id in gone {
            self.leave(id);
        }
    }
    pub fn account_busy(&self, ids: &[i64]) -> bool {
        ids.iter()
            .any(|id| self.actor(*id).is_some_and(|actor| !actor.offline))
    }
    pub fn take_deaths(&mut self) -> Vec<Actor> {
        let mut pending = Vec::new();
        let ids: Vec<i64> = self.entities.keys().copied().collect();
        for id in ids {
            let Some(entity) = self.entities.get(&id) else {
                continue;
            };
            let Some(mut body) = self.world.get_mut::<Adventurer>(*entity) else {
                continue;
            };
            if body.actor.pending_death {
                body.actor.pending_death = false;
                pending.push(body.actor.clone());
            }
        }
        pending
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
fn advance(
    world: Res<Terrain>,
    blocks: Res<Blocks>,
    mut tick: ResMut<Tick>,
    mut players: Query<&mut Adventurer>,
) {
    let blocks = blocks.0.clone();
    tick.0 += 1;
    for mut p in &mut players {
        p.actor.speed = 0.;
        if p.actor.offline {
            p.offline_age = p.offline_age.saturating_add(1);
        }
        if p.actor.offline && !p.actor.bound && p.actor.hp > 0. {
            let post = if p.actor.orders.works || !p.actor.orders.returns {
                p.actor.post
            } else {
                mortown_world::SPAWN
            };
            if !p.actor.orders.guards || p.actor.attack_target.is_none() {
                p.target = post;
            }
            p.last_input = tick.0;
        }
        if p.actor.hp <= 0.
            || p.actor.bound
            || (!p.actor.offline && tick.0.saturating_sub(p.last_input) > 10)
        {
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
        let clear = |a: [f32; 2], b: [f32; 2]| {
            world.0.segment_clear(a, b) && !mortown_protocol::work::blocks_segment(&blocks, a, b)
        };
        let next = if clear(from, desired) {
            desired
        } else {
            [[desired[0], from[1]], [from[0], desired[1]]]
                .into_iter()
                .find(|candidate| clear(from, *candidate))
                .unwrap_or(from)
        };
        let home = mortown_world::SPAWN;
        let next = if p.actor.return_until > mortown_protocol::life::now_millis()
            && (next[0] - home[0]).hypot(next[1] - home[1])
                > mortown_protocol::life::return_radius()
        {
            from
        } else {
            next
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
    #[test]
    fn a_timber_block_stops_the_step() {
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let start = mortown_world::SPAWN;
        sim.join(1, "Test".into(), start, 0.).unwrap();
        sim.set_blocks(vec![mortown_protocol::work::Block {
            id: 1,
            position: [start[0] + 1., start[1]],
            integrity: 4,
            level: 0,
        }]);
        sim.input(1, 1, [start[0] + 8., start[1]], false).unwrap();
        for _ in 0..40 {
            sim.update();
        }
        let at = sim.actor(1).unwrap().position;
        assert!((at[0] - (start[0] + 1.)).abs() > mortown_protocol::work::BLOCK_RADIUS - 0.2);
        assert!(at[0] < start[0] + 1.);
    }
    #[test]
    fn returning_to_an_offline_body_accepts_a_new_move_counter() {
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let start = mortown_world::SPAWN;
        sim.join(1, "Test".into(), start, 0.).unwrap();
        sim.input(1, 40, [start[0] + 5., start[1]], false).unwrap();
        sim.disconnect(1);
        sim.join(1, "Test".into(), start, 0.).unwrap();
        assert!(sim.input(1, 1, [start[0] + 1., start[1]], false).is_ok());
        assert!(!sim.actor(1).unwrap().offline);
    }
}
