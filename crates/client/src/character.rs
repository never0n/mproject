use bevy::prelude::*;
use std::time::Duration;

const MODEL: &str = "characters/adventurer.glb";
#[derive(Resource)]
pub struct HeroAnimations {
    graph: Handle<AnimationGraph>,
    clips: Vec<AnimationNodeIndex>,
}
pub fn load(
    commands: &mut Commands,
    assets: &AssetServer,
    graphs: &mut Assets<AnimationGraph>,
) -> Handle<Scene> {
    let (graph, clips) = AnimationGraph::from_clips(
        [0, 2, 1]
            .into_iter()
            .map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(MODEL))),
    );
    commands.insert_resource(HeroAnimations {
        graph: graphs.add(graph),
        clips,
    });
    assets.load(GltfAssetLabel::Scene(0).from_asset(MODEL))
}
pub fn initialize(
    mut commands: Commands,
    animations: Res<HeroAnimations>,
    mut players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
) {
    for (entity, mut player) in &mut players {
        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut player, animations.clips[0], Duration::ZERO)
            .repeat();
        commands
            .entity(entity)
            .insert((AnimationGraphHandle(animations.graph.clone()), transitions));
    }
}
#[derive(Component, Default)]
pub struct AnimatedActor {
    previous: Option<Vec3>,
    held: bool,
}
/// Optional stride reference for actors whose world speed differs from the hero's.
#[derive(Component)]
pub struct Gait {
    pub walk_speed: f32,
    pub run_speed: f32,
}
#[derive(Component, Default)]
pub struct AnimationHold(pub bool);
pub fn animate(
    mut actors: Query<(
        &Transform,
        &Children,
        &mut AnimatedActor,
        Option<&Gait>,
        Option<&AnimationHold>,
    )>,
    children: Query<&Children>,
    time: Res<Time>,
    animations: Res<HeroAnimations>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (transform, descendants, mut actor, gait, hold) in &mut actors {
        let speed = actor.previous.filter(|_| !actor.held).map_or(0., |p| {
            transform.translation.distance(p) / time.delta_secs().max(0.001)
        });
        actor.previous = Some(transform.translation);
        actor.held = hold.is_some_and(|h| h.0);
        let next = if speed < 0.1 {
            0
        } else if speed < gait.map_or(7., |g| g.walk_speed * 1.5) {
            1
        } else {
            2
        };
        for root in descendants.iter() {
            for entity in std::iter::once(root).chain(children.iter_descendants(root)) {
                if let Ok((mut player, mut transitions)) = players.get_mut(entity) {
                    if hold.is_some_and(|hold| hold.0) {
                        player.pause_all();
                        continue;
                    }
                    if hold.is_some() {
                        player.resume_all();
                    }
                    if transitions.get_main_animation() != Some(animations.clips[next]) {
                        transitions
                            .play(
                                &mut player,
                                animations.clips[next],
                                Duration::from_millis(160),
                            )
                            .repeat();
                    }
                    if let Some(gait) = gait
                        && let Some(active) = player.animation_mut(animations.clips[next])
                    {
                        active.set_speed(if next == 0 {
                            1.
                        } else {
                            (speed
                                / if next == 1 {
                                    gait.walk_speed
                                } else {
                                    gait.run_speed
                                })
                            .clamp(0.5, 1.6)
                        });
                    }
                }
            }
        }
    }
}
