use super::WorkRig;
use crate::{
    Terrain,
    network::{Mode, Network},
    ui::CharacterWindow,
    weapon_pose::{GripRig, global, solve},
};
use bevy::prelude::*;
use mortown_world::npcs::Role;
use std::f32::consts::{FRAC_PI_2, TAU};

pub(super) fn tool_socket() -> Quat {
    Quat::from_rotation_x(FRAC_PI_2)
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Clone, Copy)]
struct Hand {
    // Desired contact point and orientation of the tool, in the work-surface frame.
    contact: Vec3,
    rotation: Quat,
    tip: Vec3,
}
fn hands(role: Role, clock: f32) -> [Hand; 2] {
    let idle = Hand {
        contact: Vec3::new(0.16, 0.065, -0.11),
        rotation: Quat::from_rotation_x(FRAC_PI_2),
        tip: Vec3::ZERO,
    };
    let mut left = idle;
    let right = match role {
        Role::Smith => {
            // Slow lift, short controlled tap, then recovery; the hammer face meets the blank.
            let t = (clock / 1.55).fract();
            let lift = if t < 0.62 {
                smooth(t / 0.62)
            } else if t < 0.79 {
                1. - smooth((t - 0.62) / 0.17)
            } else {
                0.
            };
            left = Hand {
                contact: Vec3::new(0.05, 0., 0.),
                rotation: Quat::from_rotation_x(FRAC_PI_2),
                tip: Vec3::new(0., 0.22, 0.),
            };
            Hand {
                contact: Vec3::new(-0.015, lift * 0.27, -lift * 0.17),
                rotation: Quat::from_rotation_x(FRAC_PI_2 - lift * 0.92),
                tip: Vec3::new(0., 0.22, 0.0525),
            }
        }
        Role::Leatherworker => {
            let phase = (clock * 0.85).fract();
            let stitch = smooth((phase - 0.1) / 0.28) * (1. - smooth((phase - 0.54) / 0.36));
            Hand {
                contact: Vec3::new(
                    -0.06 + (clock * 0.32).sin() * 0.055,
                    stitch * 0.09,
                    -stitch * 0.015,
                ),
                rotation: Quat::from_rotation_x(-0.28),
                tip: Vec3::new(0., -0.085, 0.),
            }
        }
        Role::Runekeeper | Role::GuildSteward => {
            let rune = role == Role::Runekeeper;
            Hand {
                contact: Vec3::new(
                    -0.04 + (clock * if rune { 2.2 } else { 3.2 }).sin() * 0.06,
                    0.008 + (clock * 3.).sin().max(0.) * 0.018,
                    0.005 + (clock * 0.95).cos() * 0.036,
                ),
                rotation: Quat::from_rotation_x(-0.4) * Quat::from_rotation_z(0.20),
                tip: Vec3::new(0., -0.13, 0.),
            }
        }
        Role::Merchant => {
            let phase = clock * TAU / 3.6;
            let path = 0.5 - phase.cos() * 0.5;
            Hand {
                contact: Vec3::new(-0.14 + path * 0.26, 0.008 + phase.sin().abs() * 0.12, 0.04),
                rotation: Quat::IDENTITY,
                tip: Vec3::ZERO,
            }
        }
        Role::Postmaster => {
            let t = (clock / 2.6).fract();
            let lift = if t < 0.5 {
                smooth(t / 0.5)
            } else {
                1. - smooth((t - 0.5) / 0.22)
            };
            left = Hand {
                contact: Vec3::new(0.075, 0.04, -0.07),
                rotation: Quat::IDENTITY,
                tip: Vec3::ZERO,
            };
            Hand {
                contact: Vec3::new(0.075, 0.045 + lift * 0.15, -0.07),
                rotation: Quat::IDENTITY,
                tip: Vec3::new(0., -0.04, 0.),
            }
        }
    };
    [right, left]
}

/// Apply a world-space turn to an animated bone. Parent axes in the imported rig
/// need not match game axes; this also avoids accumulating rotations frame to frame.
fn turn(
    entity: Entity,
    rotation: Quat,
    transforms: &mut Query<&mut Transform>,
    parents: &Query<&ChildOf>,
) {
    let Ok(parent) = parents.get(entity) else {
        return;
    };
    let Some(matrix) = global(parent.parent(), transforms, parents) else {
        return;
    };
    let (_, basis, _) = matrix.to_scale_rotation_translation();
    if let Ok(mut bone) = transforms.get_mut(entity) {
        bone.rotation = (basis.inverse() * rotation * basis * bone.rotation).normalize();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn pose(
    time: Res<Time>,
    world: Res<Terrain>,
    network: Res<Network>,
    panel: Res<CharacterWindow>,
    mut rigs: Query<(Entity, &GripRig, &mut WorkRig)>,
    mut transforms: Query<&mut Transform>,
    parents: Query<&ChildOf>,
    mut visibility: Query<&mut Visibility>,
) {
    for (root, grip, mut state) in &mut rigs {
        if !network
            .residents
            .iter()
            .any(|n| n.id == state.npc && n.hp > 0.)
        {
            continue;
        }
        let Some(npc) = mortown_world::npcs::definition(state.npc) else {
            continue;
        };
        let Some(actor) = network
            .authoritative
            .as_ref()
            .filter(|_| network.mode == Mode::Online)
        else {
            continue;
        };
        let talking =
            panel.open && panel.npc == Some(npc.id) && npc.available(&world.0, actor.position);
        state.attention += ((if talking { 1. } else { 0. }) - state.attention)
            * (1. - (-time.delta_secs() * 6.).exp());
        if !talking {
            state.clock = (state.clock + time.delta_secs().min(0.1)) % 3600.;
        }
        let cycle = state.clock.rem_euclid(9.4);
        let work =
            smooth(cycle / 0.6) * (1. - smooth((cycle - 6.8) / 0.8)) * (1. - state.attention);
        let yaw = Quat::from_rotation_y(npc.yaw());
        // Bend from the lower back so the hands meet the counter top or anvil face.
        let lean = if npc.role == Role::Smith { 0.08 } else { 0.32 } * work;
        turn(
            state.back,
            yaw * Quat::from_rotation_x(lean) * yaw.inverse(),
            &mut transforms,
            &parents,
        );
        turn(
            grip.chest,
            yaw * Quat::from_rotation_x(0.08 * work) * yaw.inverse(),
            &mut transforms,
            &parents,
        );
        let Some(root_matrix) = global(root, &transforms, &parents) else {
            continue;
        };
        let delta = yaw.inverse()
            * (Vec3::from_array(actor.position) - root_matrix.transform_point3(Vec3::ZERO));
        let look = delta.x.atan2(delta.z).clamp(-0.85, 0.85) * state.attention;
        turn(
            state.head,
            yaw * Quat::from_rotation_y(look) * Quat::from_rotation_x(0.16 * work) * yaw.inverse(),
            &mut transforms,
            &parents,
        );
        let Some(chest) = global(grip.chest, &transforms, &parents) else {
            continue;
        };
        let surface = Vec3::from_array(npc.work_surface(&world.0));
        let samples = hands(npc.role, state.clock);
        for (side, (chain, sample)) in [grip.right, grip.left].into_iter().zip(samples).enumerate()
        {
            let Some(hand) = global(chain[2], &transforms, &parents) else {
                continue;
            };
            let (scale, _, _) = hand.to_scale_rotation_translation();
            let sign = if side == 0 { -1. } else { 1. };
            let rest = chest.transform_point3(Vec3::new(sign * 0.17, -0.27, 0.20));
            let work_orientation = yaw * sample.rotation * tool_socket().inverse();
            let target = surface + yaw * sample.contact
                - work_orientation * (scale * (tool_socket() * sample.tip));
            let resting_rotation = yaw * Quat::from_rotation_x(0.15) * tool_socket().inverse();
            let orientation = resting_rotation.slerp(work_orientation, work);
            solve(
                chain,
                rest.lerp(target, work),
                orientation,
                yaw * Vec3::new(sign, -0.55, -0.25),
                &mut transforms,
                &parents,
            );
            if let Ok(mut visible) = visibility.get_mut(state.tools[side]) {
                // Loose coins/letters stay on the desk during a conversation or rest.
                let loose = (npc.role == Role::Merchant && side == 0)
                    || (npc.role == Role::Postmaster && side == 1);
                *visible = if loose && work < 0.1 {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                };
            }
        }
    }
}
