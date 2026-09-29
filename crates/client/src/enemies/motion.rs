//! Hand-keyed skeletal action curves. All attack timing comes from server snapshots.
use bevy::prelude::*;
use mortown_protocol::{
    Monster,
    monsters::{AttackMotion, Kind},
};
use std::f32::consts::{FRAC_PI_2, PI};

pub const CORPSE_SECONDS: f32 = 5.;
const FALL_SECONDS: f32 = 1.15;

#[derive(Default)]
pub struct Playback {
    attack: Option<AttackMotion>,
    received_at: f32,
    cancelled: Option<(Stroke, f32)>,
    death_ms: Option<u32>,
    death_received_at: f32,
}
impl Playback {
    pub fn observe(&mut self, monster: &Monster, now: f32) {
        if monster.attack != self.attack {
            // An interrupted windup relaxes back to locomotion, never completing a hit.
            if monster.attack.is_none() && self.attack.is_some() {
                self.cancelled = self.stroke(monster.kind, now).map(|pose| (pose, now));
            } else if monster.attack.is_some() {
                self.cancelled = None;
            }
            self.attack = monster.attack;
            self.received_at = now;
        }
        let death = if monster.hp <= 0. {
            Some(monster.death_ms.unwrap_or(0))
        } else {
            None
        };
        if death != self.death_ms {
            self.death_ms = death;
            self.death_received_at = now;
        }
        if death.is_some() {
            self.attack = None;
            self.cancelled = None;
        }
    }
    pub fn death_age(&self, now: f32) -> Option<f32> {
        self.death_ms
            .map(|ms| ms as f32 / 1000. + (now - self.death_received_at).max(0.))
    }
    pub fn stroke(&self, kind: Kind, now: f32) -> Option<Stroke> {
        if let Some(attack) = self.attack {
            let elapsed = attack.elapsed_ms as f32 / 1000. + (now - self.received_at).max(0.);
            let impact = attack.impact_ms as f32 / 1000.;
            let duration = attack.duration_ms as f32 / 1000.;
            // Freeze just before contact if an impact snapshot has not arrived yet.
            let t = if attack.released {
                elapsed
            } else {
                elapsed.min((impact - 0.025).max(0.))
            };
            if duration <= 0. || t >= duration {
                return None;
            }
            return Some(stroke(kind, t, impact, duration));
        }
        self.cancelled.and_then(|(mut pose, start)| {
            let fade = 1. - smooth((now - start) / 0.18);
            if fade <= 0. {
                return None;
            }
            pose.weight *= fade;
            Some(pose)
        })
    }
}

#[derive(Clone, Copy)]
pub struct Hand {
    pub position: Vec3,
    /// Orientation of the weapon in chest space, before the palm socket rotation.
    pub rotation: Quat,
}
#[derive(Clone, Copy)]
pub struct Stroke {
    pub chest: Quat,
    pub hand: Hand,
    pub weight: f32,
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Clone, Copy)]
struct Key {
    at: f32,
    hand: [f32; 3],
    weapon: [f32; 3],
    torso: [f32; 3],
}
fn key(at: f32, hand: [f32; 3], weapon: [f32; 3], torso: [f32; 3]) -> Key {
    Key {
        at,
        hand,
        weapon,
        torso,
    }
}
fn rotation(angles: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::XYZ, angles[0], angles[1], angles[2])
}
fn stroke(kind: Kind, time: f32, impact: f32, duration: f32) -> Stroke {
    let keys = match kind {
        Kind::Bandit => [
            key(0., [-0.30, -0.19, 0.22], [0.3, 0., 0.1], [0., 0., 0.]),
            key(
                impact * 0.72,
                [-0.36, 0.02, 0.13],
                [1.3, -0.1, -0.1],
                [-0.06, -0.24, -0.05],
            ),
            key(
                impact,
                [-0.16, -0.03, 0.60],
                [1.55, 0.05, 0.],
                [0.14, 0.14, 0.04],
            ),
            key(
                impact + 0.10,
                [-0.11, -0.08, 0.64],
                [1.6, 0., -0.1],
                [0.18, 0.20, 0.04],
            ),
            key(duration, [-0.30, -0.19, 0.22], [0.3, 0., 0.1], [0., 0., 0.]),
        ],
        Kind::Marauder => [
            key(0., [-0.30, -0.17, 0.26], [0.15, 0., -0.1], [0., 0., 0.]),
            key(
                impact * 0.70,
                [-0.23, 0.32, 0.06],
                [-0.65, 0.1, -0.25],
                [-0.10, -0.26, -0.08],
            ),
            key(
                impact,
                [-0.16, -0.16, 0.49],
                [1.35, -0.1, -0.30],
                [0.19, 0.14, 0.08],
            ),
            key(
                impact + 0.14,
                [0.02, -0.31, 0.35],
                [1.8, 0., -0.60],
                [0.24, 0.30, 0.10],
            ),
            key(
                duration,
                [-0.30, -0.17, 0.26],
                [0.15, 0., -0.1],
                [0., 0., 0.],
            ),
        ],
        Kind::Apostate => [
            key(0., [0.22, -0.07, 0.33], [0., 0., 0.], [0., 0., 0.]),
            key(
                impact * 0.72,
                [0.30, 0.10, 0.37],
                [0.25, 0., -0.15],
                [-0.04, -0.08, 0.],
            ),
            key(impact, [0.20, 0.03, 0.62], [0.5, 0., 0.], [0.09, 0.08, 0.]),
            key(
                impact + 0.12,
                [0.17, 0.01, 0.65],
                [0.5, 0., 0.],
                [0.12, 0.10, 0.],
            ),
            key(duration, [0.22, -0.07, 0.33], [0., 0., 0.], [0., 0., 0.]),
        ],
    };
    let pair = keys
        .windows(2)
        .find(|p| time <= p[1].at)
        .unwrap_or(&keys[3..]);
    let t = smooth((time - pair[0].at) / (pair[1].at - pair[0].at).max(0.001));
    Stroke {
        chest: rotation(pair[0].torso).slerp(rotation(pair[1].torso), t),
        hand: Hand {
            position: Vec3::from_array(pair[0].hand).lerp(Vec3::from_array(pair[1].hand), t),
            rotation: rotation(pair[0].weapon).slerp(rotation(pair[1].weapon), t),
        },
        weight: smooth(time / 0.13) * (1. - smooth((time - (duration - 0.22)) / 0.22)),
    }
}

/// Frozen locomotion pose plus an authored collapse; this is not a physics ragdoll.
pub struct Fall {
    bones: Vec<(Entity, Transform)>,
    shape: Transform,
    direction: f32,
    hips: Entity,
    feet: [Option<LegAnchor>; 2],
}
struct LegAnchor {
    chain: [Entity; 3],
    position: Vec3,
    rotation: Quat,
}
impl Fall {
    pub fn capture(
        bones: &[Entity],
        shape: Entity,
        owner: Entity,
        transforms: &Query<&mut Transform>,
        parents: &Query<&ChildOf>,
        world: &mortown_world::World,
    ) -> Option<Self> {
        let root = crate::weapon_pose::global(owner, transforms, parents)?;
        let feet = root.transform_point3(Vec3::ZERO);
        // Prefer a clear fall direction; avoid projecting the body through nearby walls.
        let direction = [0., FRAC_PI_2, -FRAC_PI_2, PI]
            .into_iter()
            .find(|&yaw| {
                let end = root.transform_point3(Quat::from_rotation_y(yaw) * Vec3::Z * 1.7);
                world.segment_clear([feet.x, feet.z], [end.x, end.z])
            })
            .unwrap_or(0.);
        // Hips / thigh / shin / foot indices in the shipped adventurer skin.
        let anchors = [[2, 3, 4], [7, 8, 9]].map(|indices| {
            let chain = [
                *bones.get(indices[0])?,
                *bones.get(indices[1])?,
                *bones.get(indices[2])?,
            ];
            let matrix = crate::weapon_pose::global(chain[2], transforms, parents)?;
            let (_, rotation, position) = matrix.to_scale_rotation_translation();
            Some(LegAnchor {
                chain,
                position,
                rotation,
            })
        });
        Some(Self {
            bones: bones
                .iter()
                .filter_map(|&e| transforms.get(e).ok().map(|t| (e, *t)))
                .collect(),
            shape: *transforms.get(shape).ok()?,
            direction,
            hips: *bones.first()?,
            feet: anchors,
        })
    }
    pub fn apply(
        &self,
        shape: Entity,
        owner: Entity,
        chest: Entity,
        age: f32,
        transforms: &mut Query<&mut Transform>,
        parents: &Query<&ChildOf>,
        world: &mortown_world::World,
    ) {
        // Restore before each evaluation so paused animation/IK cannot accumulate twists.
        for &(entity, pose) in &self.bones {
            if let Ok(mut transform) = transforms.get_mut(entity) {
                *transform = pose;
            }
        }
        let collapse = smooth(age / 0.32);
        let fall = smooth((age - 0.16) / (FALL_SECONDS - 0.16));
        if let Ok(mut hips) = transforms.get_mut(self.hips) {
            hips.translation.y -= 0.28 * collapse * (1. - fall);
        }
        if let Ok(mut torso) = transforms.get_mut(chest) {
            torso.rotation *= Quat::from_rotation_x(0.22 * collapse * (1. - fall));
        }
        let turn = Quat::from_rotation_y(self.direction);
        let rotation = turn * Quat::from_rotation_x(FRAC_PI_2 * fall) * turn.inverse();
        let mut pose = self.shape;
        pose.rotation = rotation * self.shape.rotation;
        pose.translation += turn * Vec3::Z * (0.12 * fall);
        let Some(root) = crate::weapon_pose::global(owner, transforms, parents) else {
            return;
        };
        let matrix = root * pose.to_matrix();
        let mut lift = 0_f32;
        // Small body envelopes sampled on the walking surface keep a settled corpse
        // above sloped ground. The outer actor/selection position is never moved.
        for (y, radius) in [(0.18, 0.10), (0.65, 0.16), (1.15, 0.23), (1.70, 0.16)] {
            for x in [-radius, 0., radius] {
                let p = matrix.transform_point3(Vec3::new(x, y, 0.));
                lift = lift.max(world.surface_height(p.x, p.z) + radius * fall + 0.025 - p.y);
            }
        }
        pose.translation.y += lift;
        if let Ok(mut transform) = transforms.get_mut(shape) {
            *transform = pose;
        }
        // The knees buckle while the feet initially stay planted; release the
        // anchors as the body falls, then preserve the settled skeleton pose.
        let release = smooth(fall / 0.55);
        for anchor in self.feet.iter().flatten() {
            let Some(current) = crate::weapon_pose::global(anchor.chain[2], transforms, parents)
            else {
                continue;
            };
            let (_, rotation, position) = current.to_scale_rotation_translation();
            let Some(hip) = crate::weapon_pose::global(anchor.chain[0], transforms, parents) else {
                continue;
            };
            let Some(knee) = crate::weapon_pose::global(anchor.chain[1], transforms, parents)
            else {
                continue;
            };
            let settled_pole = knee.transform_point3(Vec3::ZERO) - hip.transform_point3(Vec3::ZERO);
            crate::weapon_pose::solve(
                anchor.chain,
                anchor.position.lerp(position, release),
                anchor.rotation.slerp(rotation, release),
                root.transform_vector3(Vec3::new(0., -0.4, 1.))
                    .lerp(settled_pole, release),
                transforms,
                parents,
            );
        }
    }
    pub fn restore_shape(&self, shape: Entity, transforms: &mut Query<&mut Transform>) {
        if let Ok(mut transform) = transforms.get_mut(shape) {
            *transform = self.shape;
        }
    }
}
