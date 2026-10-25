//! Humanoid model and locomotion animation (idle / walk / run) for every character-like actor:
//! hero, other players, enemies. Speed is measured from how far the entity moved this frame,
//! so animation follows whatever moves the `Transform` (`move_player`, `network::receive`, enemy sync).
//! Attack/skill poses are layered on top by `player_motion.rs` in PostUpdate.
use bevy::prelude::*;
use std::time::Duration;

const MODEL: &str = "characters/adventurer.glb";
/// Shared animation graph; `clips[0/1/2]` = idle / walk / run.
#[derive(Resource)]
pub struct HeroAnimations {
    graph: Handle<AnimationGraph>,
    clips: Vec<AnimationNodeIndex>,
}
/// Loads the model scene and builds `HeroAnimations` (called from `scene::setup`).
pub fn load(
    commands: &mut Commands,
    assets: &AssetServer,
    graphs: &mut Assets<AnimationGraph>,
) -> Handle<Scene> {
    // glTF animation indices reordered so the graph is idle, walk, run (see `animate`).
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
/// Hooks each newly loaded glTF `AnimationPlayer` to the shared graph and starts idle.
/// Runs every frame but only touches players added since the last run (`Added`).
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
/// Put on the root entity of anything that should play idle/walk/run; the glTF scene is a child.
/// `previous` is last frame's position, used to measure speed.
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
/// `true` freezes the animation (e.g. a fallen enemy); speed isn't measured while held.
#[derive(Component, Default)]
pub struct AnimationHold(pub bool);
/// Chooses idle/walk/run from measured speed and cross-fades to it (160 ms).
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
        // m/s thresholds: below 0.1 idle; below 7 (between WALK_SPEED 5 and RUN_SPEED 10) walk.
        let next = if speed < 0.1 {
            0
        } else if speed < gait.map_or(7., |g| g.walk_speed * 1.5) {
            1
        } else {
            2
        };
        // The AnimationPlayer sits somewhere inside the loaded glTF hierarchy; search all descendants.
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
                    // Scale playback to actual speed so feet don't slide (clamped 0.5–1.6×).
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
