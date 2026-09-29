use crate::{
    Terrain,
    network::{Mode, Network},
};
use bevy::prelude::*;
use mortown_protocol::professions::Profession;
use mortown_world::npcs::{Npc, RESIDENTS, Role};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Shop,
    Recipes,
    Professions,
    Mail,
    Alliance,
}

#[derive(Message, Clone, Copy)]
pub struct ServiceRequest {
    pub npc: u8,
    pub service: Service,
}

pub fn profession(npc: &Npc) -> Option<Profession> {
    match npc.role {
        Role::Leatherworker => Some(Profession::Leatherworking),
        Role::Smith => Some(Profession::Smithing),
        Role::Runekeeper => Some(Profession::Runecraft),
        _ => None,
    }
}
pub fn offers(npc: &Npc, service: Service) -> bool {
    match service {
        Service::Shop => npc.role == Role::Merchant,
        Service::Recipes | Service::Professions => profession(npc).is_some(),
        Service::Mail => npc.role == Role::Postmaster,
        Service::Alliance => npc.role == Role::GuildSteward,
    }
}
pub fn resolve(network: &Network, id: u8) -> Option<Npc> {
    let state = network.residents.iter().find(|n| n.id == id)?;
    Some(mortown_world::npcs::definition(state.template)?.at(state.id, state.position, state.yaw))
}
#[derive(Component)]
pub struct NpcModel(pub u8);
#[derive(Component)]
pub struct Spawned;
pub fn available(network: &Network, world: &mortown_world::World, id: u8) -> bool {
    network.mode == Mode::Online
        && resolve(network, id).is_some_and(|npc| {
            if !network.residents.iter().any(|n| n.id == id && n.hp > 0.) {
                return false;
            }
            network
                .authoritative
                .as_ref()
                .is_some_and(|a| a.hp > 0. && npc.available(world, a.position))
        })
}
impl ServiceRequest {
    pub fn valid(self, network: &Network, world: &mortown_world::World) -> bool {
        available(network, world, self.npc)
            && resolve(network, self.npc).is_some_and(|n| offers(&n, self.service))
    }
}

#[derive(Component)]
pub struct Nameplate(u8);
pub fn setup(
    mut commands: Commands,
    world: Res<Terrain>,
    assets: Res<AssetServer>,
    icons: Res<crate::actionbar::Icons>,
) {
    let model = assets.load(GltfAssetLabel::Scene(0).from_asset("characters/adventurer.glb"));
    for npc in RESIDENTS {
        commands
            .spawn((
                NpcModel(npc.id),
                Name::new(format!("{} · {}", npc.name, npc.role.title())),
                Transform::from_translation(Vec3::from_array(npc.feet(&world.0)))
                    .with_rotation(Quat::from_rotation_y(npc.yaw())),
                Visibility::default(),
            ))
            .with_children(|root| {
                root.spawn((
                    SceneRoot(model.clone()),
                    Transform::default(),
                    crate::appearance::Wearer::Npc(npc.id),
                ));
            });
        commands
            .spawn((
                Nameplate(npc.id),
                crate::actionbar::label(
                    format!("{}\n{}", npc.name, npc.role.title()),
                    &icons.1,
                    13.,
                ),
                TextLayout::new_with_justify(Justify::Center),
                bevy::ui::FocusPolicy::Pass,
                Node {
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    width: px(240),
                    ..default()
                },
            ))
            // label() already contains TextColor; replace it after spawning the bundle.
            .insert(TextColor(Color::srgb(0.77, 0.88, 0.77)));
    }
}
pub fn labels(
    network: Res<Network>,
    world: Res<Terrain>,
    scale: Res<UiScale>,
    camera: Single<
        (&Camera, &GlobalTransform),
        (
            Without<crate::inventory::PreviewCamera>,
            Without<crate::inventory::PortraitCamera>,
        ),
    >,
    mut labels: Query<(&Nameplate, &mut Node)>,
) {
    for (Nameplate(id), mut node) in &mut labels {
        node.display = Display::None;
        let Some(actor) = network
            .authoritative
            .as_ref()
            .filter(|_| network.mode == Mode::Online)
        else {
            continue;
        };
        let Some(npc) = resolve(&network, *id)
            .filter(|_| network.residents.iter().any(|n| n.id == *id && n.hp > 0.))
        else {
            node.display = Display::None;
            continue;
        };
        let feet = Vec3::from_array(npc.feet(&world.0));
        if feet.distance(Vec3::from_array(actor.position)) > 45.
            || !npc.same_room(&world.0, actor.position)
        {
            continue;
        }
        if !world.0.sight_clear(
            [
                actor.position[0],
                actor.position[1] + 1.5,
                actor.position[2],
            ],
            (feet + Vec3::Y * 1.5).to_array(),
        ) {
            continue;
        }
        let height = if npc.race == 5 { 1.85 } else { 2.4 };
        let Ok(screen) = camera
            .0
            .world_to_viewport(camera.1, feet + Vec3::Y * height)
        else {
            continue;
        };
        node.left = px(screen.x / scale.0 - 120.);
        node.top = px(screen.y / scale.0 - 36.);
        node.display = Display::Flex;
    }
}

pub fn sync(
    mut commands: Commands,
    network: Res<Network>,
    assets: Res<AssetServer>,
    icons: Res<crate::actionbar::Icons>,
    mut models: Query<(Entity, &NpcModel, &mut Transform, &mut Visibility)>,
    labels: Query<(Entity, &Nameplate)>,
    mut desks: Query<(&crate::residents::Desk, &mut Visibility), Without<NpcModel>>,
) {
    let mut present = std::collections::HashSet::new();
    for (e, NpcModel(id), mut transform, mut visibility) in &mut models {
        present.insert(*id);
        if let Some(n) = network.residents.iter().find(|n| n.id == *id) {
            *visibility = Visibility::Inherited;
            transform.translation = Vec3::from_array(n.position);
            transform.rotation = Quat::from_rotation_y(n.yaw)
                * if n.hp <= 0. {
                    Quat::from_rotation_z(1.5)
                } else {
                    Quat::IDENTITY
                };
        } else if *id <= 7 {
            *visibility = Visibility::Hidden;
        } else {
            commands.entity(e).despawn();
        }
    }
    for (desk, mut visibility) in &mut desks {
        *visibility = if network
            .residents
            .iter()
            .any(|n| n.id == desk.0 && n.hp > 0.)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (e, Nameplate(id)) in &labels {
        if *id > 7 && !network.residents.iter().any(|n| n.id == *id) {
            commands.entity(e).despawn();
        }
    }
    for n in &network.residents {
        if present.contains(&n.id) {
            continue;
        }
        let Some(npc) = resolve(&network, n.id) else {
            continue;
        };
        commands
            .spawn((
                NpcModel(n.id),
                Transform::from_translation(Vec3::from_array(n.position))
                    .with_rotation(Quat::from_rotation_y(n.yaw)),
                Visibility::Inherited,
            ))
            .with_children(|p| {
                p.spawn((
                    Spawned,
                    SceneRoot(
                        assets
                            .load(GltfAssetLabel::Scene(0).from_asset("characters/adventurer.glb")),
                    ),
                    Transform::default(),
                    crate::appearance::Wearer::Npc(n.template),
                ));
            });
        commands.spawn((
            Nameplate(n.id),
            crate::actionbar::label(format!("{} · #{}", npc.name, n.id), &icons.1, 13.),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: px(240),
                ..default()
            },
            bevy::ui::FocusPolicy::Pass,
        ));
    }
}
