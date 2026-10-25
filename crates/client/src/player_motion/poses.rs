//! Hand-tuned keyframes for attack/spell/gesture poses, per weapon family.
//! Purely visual; tweak numbers here to change how a swing looks.
use bevy::prelude::*;
use mortown_protocol::{actions::Kind, weapons::Family};
#[derive(Clone, Copy)]
pub(super) struct Hand {
    pub position: Vec3,
    pub rotation: Quat,
}
pub(super) struct Pose {
    pub chest: Quat,
    pub hand: Hand,
}
/// One keyframe: hand position in the chest bone's local space (meters),
/// hand rotation and torso twist as Euler XYZ angles (radians).
#[derive(Clone, Copy)]
struct Key {
    hand: [f32; 3],
    angle: [f32; 3],
    torso: [f32; 3],
}
fn k(hand: [f32; 3], angle: [f32; 3], torso: [f32; 3]) -> Key {
    Key { hand, angle, torso }
}
fn rotation(v: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::XYZ, v[0], v[1], v[2])
}

/// Pose at progress `t` (0..1): rest → ready → strike → follow-through → rest.
pub(super) fn sample(kind: Kind, family: Option<Family>, t: f32, skill: bool) -> Pose {
    let rest = k([-0.28, -0.18, 0.25], [0.15, 0., -0.1], [0.; 3]);
    let [ready, strike, follow] = match (kind, family) {
        (Kind::Spell, _) => [
            k([0.28, 0.16, 0.30], [0.2, 0., 0.1], [-0.06, -0.15, 0.]),
            k([0.16, 0., 0.60], [0.7, 0., 0.], [0.12, 0.15, 0.]),
            k([0.18, -0.04, 0.61], [0.8, 0., 0.], [0.16, 0.20, 0.]),
        ],
        (Kind::Gesture, _) => [
            k([0.24, 0.04, 0.20], [0.2, 0., 0.2], [-0.04, 0., 0.]),
            k([0.34, -0.27, 0.34], [1.6, 0., 0.4], [0.15, 0.12, 0.]),
            k([0.36, -0.37, 0.18], [2., 0., 0.4], [0.10, 0.12, 0.]),
        ],
        (_, Some(Family::Sword)) => [
            k(
                [-0.40, 0.10, 0.13],
                [-0.35, -0.45, -0.8],
                [-0.04, -0.32, -0.05],
            ),
            k([-0.05, -0.05, 0.53], [1.25, 0., -0.55], [0.08, 0.20, 0.07]),
            k([0.22, -0.16, 0.29], [1.60, 0.40, -0.8], [0.12, 0.40, 0.08]),
        ],
        (_, Some(Family::Axe)) => [
            k(
                [-0.29, 0.29, 0.09],
                [-0.65, 0.05, -0.3],
                [-0.09, -0.2, -0.08],
            ),
            k([-0.15, -0.17, 0.51], [1.4, 0., -0.4], [0.19, 0.12, 0.08]),
            k([0.05, -0.29, 0.33], [1.85, 0., -0.7], [0.22, 0.27, 0.09]),
        ],
        (_, Some(Family::Hammer)) => [
            k(
                [-0.27, 0.31, 0.14],
                [-0.75, 0., -0.12],
                [-0.12, -0.13, -0.04],
            ),
            k([-0.20, -0.21, 0.48], [1.48, 0., -0.12], [0.24, 0.12, 0.04]),
            k([-0.12, -0.36, 0.29], [1.9, 0., -0.15], [0.27, 0.16, 0.04]),
        ],
        (_, Some(Family::Greatsword)) => [
            k(
                [-0.29, 0.13, 0.14],
                [-0.4, -0.3, -0.7],
                [-0.05, -0.35, -0.1],
            ),
            k([-0.02, -0.10, 0.47], [1.28, 0., -0.55], [0.12, 0.24, 0.1]),
            k([0.16, -0.21, 0.28], [1.65, 0.35, -0.8], [0.16, 0.43, 0.12]),
        ],
        (_, Some(Family::Greataxe)) => [
            k([-0.16, 0.25, 0.13], [-0.8, 0., -0.35], [-0.14, -0.3, -0.1]),
            k([-0.02, -0.19, 0.44], [1.45, 0., -0.35], [0.23, 0.25, 0.1]),
            k([0.15, -0.31, 0.28], [1.9, 0.2, -0.55], [0.28, 0.34, 0.1]),
        ],
        (_, Some(Family::Greatclub)) => [
            k([-0.10, 0.29, 0.14], [-0.8, 0., -0.08], [-0.15, -0.1, 0.]),
            k([-0.05, -0.22, 0.42], [1.50, 0., -0.08], [0.25, 0.08, 0.]),
            k([0., -0.36, 0.27], [1.95, 0., -0.08], [0.29, 0.12, 0.]),
        ],
        (_, Some(Family::Pike)) => [
            k([-0.18, -0.12, 0.07], [1.5, 0., -0.05], [-0.04, -0.20, 0.]),
            k([-0.10, -0.10, 0.40], [1.56, 0., -0.03], [0.15, 0.12, 0.]),
            k([-0.07, -0.13, 0.44], [1.58, 0., -0.03], [0.18, 0.17, 0.]),
        ],
        (_, Some(Family::Bow)) => [
            k([0.12, -0.03, 0.49], [0., 0., 0.02], [-0.02, -0.12, -0.02]),
            k([0.12, -0.03, 0.51], [0., 0., 0.02], [0., -0.12, -0.02]),
            k([0.11, -0.05, 0.49], [0.06, 0., 0.04], [0.04, -0.08, 0.]),
        ],
        (_, Some(Family::Dagger)) => [
            k([-0.35, 0.02, 0.13], [1.3, 0., -0.1], [-0.04, -0.22, -0.03]),
            k([-0.16, -0.02, 0.58], [1.57, 0., 0.], [0.12, 0.16, 0.03]),
            k([-0.09, -0.08, 0.63], [1.6, 0., 0.], [0.16, 0.22, 0.04]),
        ],
        (_, _) => [
            k([-0.27, 0.02, 0.19], [1.2, 0., -0.1], [-0.03, -0.2, 0.]),
            k([-0.13, -0.02, 0.57], [1.55, 0., -0.1], [0.13, 0.15, 0.]),
            k([-0.10, -0.07, 0.60], [1.55, 0., -0.1], [0.15, 0.20, 0.]),
        ],
    };
    let rest = if kind != Kind::Weapon || family == Some(Family::Bow) {
        k([0.22, -0.14, 0.28], [0.; 3], [0.; 3])
    } else {
        rest
    };
    // Key times as fractions of the action duration; the hit visually lands around t = 0.48.
    let keys = [rest, ready, strike, follow, rest];
    let times = [0., 0.34, 0.48, 0.64, 1.];
    let i = (0..4).find(|&i| t <= times[i + 1]).unwrap_or(3);
    let blend = super::smooth((t - times[i]) / (times[i + 1] - times[i]));
    let mut torso = rotation(keys[i].torso).slerp(rotation(keys[i + 1].torso), blend);
    // Skills exaggerate the torso twist by 12% (slerp past 1 extrapolates).
    if skill {
        torso = Quat::IDENTITY.slerp(torso, 1.12);
    }
    Pose {
        chest: torso,
        hand: Hand {
            position: Vec3::from_array(keys[i].hand)
                .lerp(Vec3::from_array(keys[i + 1].hand), blend),
            rotation: rotation(keys[i].angle).slerp(rotation(keys[i + 1].angle), blend),
        },
    }
}
