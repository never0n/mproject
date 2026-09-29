//! Confirmed player actions layered onto locomotion. Damage is never driven by these poses.
mod poses;
use crate::{
    appearance::{EquipmentRig, Wearer, weapon_socket},
    network::{Mode, Network},
    weapon_pose::{GripRig, global, solve},
};
use bevy::prelude::*;
use mortown_protocol::{
    actions::{Kind, Motion},
    weapons::{self, Family},
};

#[derive(Clone, Copy)]
struct Observed {
    action: Motion,
    received_at: f32,
}
#[derive(Component)]
pub struct PlayerMotion {
    actor: Option<i64>,
    base_rotation: Quat,
    moving: bool,
    events: Vec<Observed>,
}
#[derive(Clone, Copy)]
pub(crate) struct Sample {
    pub action: Motion,
    pub t: f32,
}
impl PlayerMotion {
    pub(crate) fn actor_id(&self) -> Option<i64> {
        self.actor
    }
    pub(crate) fn sample(&self, now: f32) -> Option<Sample> {
        self.events
            .iter()
            .filter_map(|e| {
                let age = e.action.elapsed_ms as f32 + (now - e.received_at).max(0.) * 1000.;
                let t = age / e.action.duration_ms.max(1) as f32;
                (t < 1.).then_some(Sample {
                    action: e.action,
                    t,
                })
            })
            .max_by_key(|s| {
                (
                    s.action.skill.is_some(),
                    s.action.started_tick,
                    s.action.sequence,
                )
            })
    }
}
pub fn prepare(
    mut commands: Commands,
    roots: Query<(Entity, &Wearer, &Transform), (With<GripRig>, Without<PlayerMotion>)>,
) {
    for (entity, wearer, transform) in &roots {
        if matches!(wearer, Wearer::Local | Wearer::Remote(_)) {
            commands.entity(entity).insert(PlayerMotion {
                actor: None,
                base_rotation: transform.rotation,
                moving: false,
                events: vec![],
            });
        }
    }
}
pub fn sync(
    network: Res<Network>,
    time: Res<Time>,
    mut roots: Query<(&Wearer, &EquipmentRig, &mut PlayerMotion)>,
) {
    for (wearer, equipment, mut state) in &mut roots {
        let actor = if network.mode != Mode::Online {
            None
        } else {
            match wearer {
                Wearer::Local => network.authoritative.as_ref(),
                Wearer::Remote(id) => network.roster.iter().find(|a| a.id == *id),
                _ => None,
            }
        };
        if state.actor != actor.map(|a| a.id) {
            state.events.clear();
            state.actor = actor.map(|a| a.id);
        }
        let Some(actor) = actor.filter(|a| a.hp > 0.) else {
            state.events.clear();
            continue;
        };
        state.moving = actor.speed > 0.1;
        state.events.retain(|old| {
            actor
                .actions
                .iter()
                .any(|a| a.sequence == old.action.sequence)
                && old.action.weapon == equipment.weapon_kind()
        });
        for action in actor
            .actions
            .iter()
            .filter(|a| a.weapon == equipment.weapon_kind())
        {
            if let Some(old) = state
                .events
                .iter_mut()
                .find(|e| e.action.sequence == action.sequence)
            {
                if old.action.elapsed_ms != action.elapsed_ms {
                    old.action = *action;
                    old.received_at = time.elapsed_secs();
                }
            } else {
                state.events.push(Observed {
                    action: *action,
                    received_at: time.elapsed_secs(),
                });
            }
        }
    }
}
/// Scene-root rotation is not an animated bone. Reset it explicitly every frame.
pub fn reset(roots: Query<(Entity, &PlayerMotion)>, mut transforms: Query<&mut Transform>) {
    for (entity, state) in &roots {
        if let Ok(mut transform) = transforms.get_mut(entity) {
            transform.rotation = state.base_rotation;
        }
    }
}
pub(crate) fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub(crate) fn bow_draw(t: f32) -> f32 {
    if t < 0.48 {
        smooth(t / 0.40) * 0.30
    } else {
        (1. - smooth((t - 0.48) / 0.065)) * 0.30
    }
}

pub fn pose(
    roots: Query<(Entity, &PlayerMotion, &EquipmentRig, &GripRig)>,
    time: Res<Time>,
    mut transforms: Query<&mut Transform>,
    parents: Query<&ChildOf>,
) {
    for (entity, state, equipment, rig) in &roots {
        let Some(sample) = state.sample(time.elapsed_secs()) else {
            continue;
        };
        let weight = smooth(sample.t / 0.13) * (1. - smooth((sample.t - 0.78) / 0.22));
        let Some(base) = global(entity, &transforms, &parents) else {
            continue;
        };
        let forward = base.transform_vector3(Vec3::Z);
        let delta = sample.action.yaw - forward.x.atan2(forward.z);
        let delta = delta.sin().atan2(delta.cos());
        if !state.moving
            && let Ok(mut root) = transforms.get_mut(entity)
        {
            root.rotation = state.base_rotation * Quat::from_rotation_y(delta * weight);
        }
        let family = equipment
            .weapon_kind()
            .and_then(weapons::definition)
            .map(|w| w.family);
        let pose = poses::sample(
            sample.action.kind,
            family,
            sample.t,
            sample.action.skill.is_some(),
        );
        if let Ok(mut chest) = transforms.get_mut(rig.chest) {
            chest.rotation *= Quat::from_rotation_y(if state.moving {
                delta.clamp(-1., 1.) * weight
            } else {
                0.
            });
            chest.rotation *= Quat::IDENTITY.slerp(pose.chest, weight);
        }
        let Some(chest) = global(rig.chest, &transforms, &parents) else {
            continue;
        };
        if sample.action.kind != Kind::Weapon {
            let bow = family == Some(Family::Bow);
            let mut hand = pose.hand;
            if bow {
                hand.position.x = -hand.position.x;
            }
            place(
                if bow { rig.right } else { rig.left },
                chest,
                hand,
                weight,
                if bow { -1. } else { 1. },
                &mut transforms,
                &parents,
            );
            continue;
        }
        let bow = family == Some(Family::Bow);
        let primary = if bow { rig.left } else { rig.right };
        place(
            primary,
            chest,
            pose.hand,
            weight,
            if bow { 1. } else { -1. },
            &mut transforms,
            &parents,
        );
        if equipment
            .weapon_kind()
            .and_then(weapons::definition)
            .is_some_and(|w| w.hands == 2)
        {
            let Some(palm) = global(primary[2], &transforms, &parents) else {
                continue;
            };
            let socket = palm * weapon_socket().to_matrix();
            let grip = match family {
                Some(Family::Bow) => Vec3::new(0.025, 0., -0.11 - bow_draw(sample.t)),
                Some(Family::Pike | Family::Staff) => Vec3::new(0., 0.30, 0.),
                _ => Vec3::new(0., -0.16, 0.),
            };
            let secondary = if bow { rig.right } else { rig.left };
            let Some(other) = global(secondary[2], &transforms, &parents) else {
                continue;
            };
            let (scale, _, current) = other.to_scale_rotation_translation();
            let (_, rotation, _) = palm.to_scale_rotation_translation();
            let orientation = rotation * Quat::from_rotation_y(std::f32::consts::PI);
            let target =
                socket.transform_point3(grip) - orientation * (scale * weapon_socket().translation);
            solve(
                secondary,
                current.lerp(target, weight),
                orientation,
                chest.transform_vector3(Vec3::new(if bow { -1. } else { 1. }, -0.65, -0.10)),
                &mut transforms,
                &parents,
            );
        }
    }
}
fn place(
    arm: [Entity; 3],
    chest: Mat4,
    hand: poses::Hand,
    weight: f32,
    side: f32,
    transforms: &mut Query<&mut Transform>,
    parents: &Query<&ChildOf>,
) {
    let Some(current) = global(arm[2], transforms, parents) else {
        return;
    };
    let (_, old_rotation, position) = current.to_scale_rotation_translation();
    let (_, rotation, _) = chest.to_scale_rotation_translation();
    solve(
        arm,
        position.lerp(chest.transform_point3(hand.position), weight),
        old_rotation.slerp(
            rotation * hand.rotation * weapon_socket().rotation.inverse(),
            weight,
        ),
        chest.transform_vector3(Vec3::new(side, -0.65, -0.08)),
        transforms,
        parents,
    );
}
