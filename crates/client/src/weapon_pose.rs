//! Secondary grips for the current skeleton, evaluated after locomotion and body proportions.
use crate::appearance::{EquipmentRig, weapon_socket};
use bevy::prelude::*;
use mortown_protocol::weapons::{self, Family};

#[derive(Component)]
pub struct GripRig {
    pub(crate) chest: Entity,
    pub(crate) left: [Entity; 3],
    pub(crate) right: [Entity; 3],
}
pub fn prepare(
    mut commands: Commands,
    roots: Query<Entity, (With<EquipmentRig>, Without<GripRig>)>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    for root in &roots {
        let descendants: Vec<_> = children.iter_descendants(root).collect();
        let find = |name: &str| {
            descendants
                .iter()
                .copied()
                .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
        };
        let (Some(chest), Some(la), Some(lf), Some(lh), Some(ra), Some(rf), Some(rh)) = (
            find("Spine1"),
            find("LeftArm"),
            find("LeftForeArm"),
            find("LeftHand"),
            find("RightArm"),
            find("RightForeArm"),
            find("RightHand"),
        ) else {
            continue;
        };
        commands.entity(root).insert(GripRig {
            chest,
            left: [la, lf, lh],
            right: [ra, rf, rh],
        });
    }
}
pub(crate) fn global(
    mut entity: Entity,
    transforms: &Query<&mut Transform>,
    parents: &Query<&ChildOf>,
) -> Option<Mat4> {
    let mut chain = Vec::new();
    // The shipped rig is shallow; a broken hierarchy must not trap a frame.
    for _ in 0..40 {
        chain.push(transforms.get(entity).ok()?.to_matrix());
        if let Ok(parent) = parents.get(entity) {
            entity = parent.parent();
        } else {
            return Some(chain.into_iter().rev().fold(Mat4::IDENTITY, |a, b| a * b));
        }
    }
    None
}
fn aim(
    joint: Entity,
    child: Entity,
    target: Vec3,
    transforms: &mut Query<&mut Transform>,
    parents: &Query<&ChildOf>,
) -> Option<()> {
    let parent = global(parents.get(joint).ok()?.parent(), transforms, parents)?;
    let child = global(child, transforms, parents)?.transform_point3(Vec3::ZERO);
    let origin = transforms.get(joint).ok()?.translation;
    let inverse = parent.inverse();
    let from = (inverse.transform_point3(child) - origin).try_normalize()?;
    let to = (inverse.transform_point3(target) - origin).try_normalize()?;
    let mut local = transforms.get_mut(joint).ok()?;
    local.rotation = (Quat::from_rotation_arc(from, to) * local.rotation).normalize();
    Some(())
}
pub(crate) fn solve(
    arm: [Entity; 3],
    target: Vec3,
    orientation: Quat,
    pole: Vec3,
    transforms: &mut Query<&mut Transform>,
    parents: &Query<&ChildOf>,
) -> Option<()> {
    let [upper, forearm, hand] = arm;
    let s = global(upper, transforms, parents)?.transform_point3(Vec3::ZERO);
    let e = global(forearm, transforms, parents)?.transform_point3(Vec3::ZERO);
    let h = global(hand, transforms, parents)?.transform_point3(Vec3::ZERO);
    let a = s.distance(e);
    let b = e.distance(h);
    if a < 0.01 || b < 0.01 {
        return None;
    }
    let direction = (target - s).try_normalize()?;
    let distance = s
        .distance(target)
        .clamp((a - b).abs() + 0.005, a + b - 0.005);
    let end = s + direction * distance;
    let bend = (pole - direction * pole.dot(direction))
        .try_normalize()
        .or_else(|| (e - s - direction * (e - s).dot(direction)).try_normalize())?;
    let along = ((a * a + distance * distance - b * b) / (2. * distance)).clamp(-a, a);
    let elbow = s + direction * along + bend * (a * a - along * along).max(0.).sqrt();
    aim(upper, forearm, elbow, transforms, parents)?;
    aim(forearm, hand, end, transforms, parents)?;
    let parent = global(parents.get(hand).ok()?.parent(), transforms, parents)?;
    let (_, parent_rotation, _) = parent.to_scale_rotation_translation();
    transforms.get_mut(hand).ok()?.rotation = (parent_rotation.inverse() * orientation).normalize();
    Some(())
}
pub fn pose(
    rigs: Query<(&EquipmentRig, &GripRig)>,
    mut transforms: Query<&mut Transform>,
    parents: Query<&ChildOf>,
) {
    for (equipment, rig) in &rigs {
        let Some(kind) = equipment.weapon_kind() else {
            continue;
        };
        let Some(weapon) = weapons::definition(kind).filter(|w| w.hands == 2) else {
            continue;
        };
        let Some(chest) = global(rig.chest, &transforms, &parents) else {
            continue;
        };
        let (_, rotation, _) = chest.to_scale_rotation_translation();
        let bow = weapon.family == Family::Bow;
        let orientation = rotation
            * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2 + if bow { 0. } else { 0.24 });
        let (primary, secondary, sign) = if bow {
            (rig.left, rig.right, 1.)
        } else {
            (rig.right, rig.left, -1.)
        };
        let wrist = chest.transform_point3(if bow {
            Vec3::new(0.16, -0.18, 0.38)
        } else {
            Vec3::new(-0.015, -0.11, 0.24)
        });
        let pole = chest.transform_vector3(Vec3::new(sign, -0.75, -0.10));
        if solve(primary, wrist, orientation, pole, &mut transforms, &parents).is_none() {
            continue;
        }
        let Some(hand) = global(primary[2], &transforms, &parents) else {
            continue;
        };
        let socket = hand * weapon_socket().to_matrix();
        let grip = match weapon.family {
            Family::Bow => Vec3::new(0.025, 0., -0.11),
            Family::Pike | Family::Staff => Vec3::new(0., 0.30, 0.),
            _ => Vec3::new(0., -0.16, 0.),
        };
        let secondary_orientation = orientation * Quat::from_rotation_y(std::f32::consts::PI);
        let Some(secondary_matrix) = global(secondary[2], &transforms, &parents) else {
            continue;
        };
        let (scale, _, _) = secondary_matrix.to_scale_rotation_translation();
        let target = socket.transform_point3(grip)
            - secondary_orientation * (scale * weapon_socket().translation);
        let pole = chest.transform_vector3(Vec3::new(-sign, -0.75, -0.10));
        solve(
            secondary,
            target,
            secondary_orientation,
            pole,
            &mut transforms,
            &parents,
        );
    }
}
