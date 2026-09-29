//! Reference city models. Scene 0 is already Y-up; the front faces +Z.
use bevy::prelude::*;
use mortown_world::SceneryKind;

struct Model {
    kind: SceneryKind,
    path: &'static str,
    /// Lift applied so the mesh bottom meets the stored ground height. From integration_manifest.json.
    ground_offset_y: f32,
}
const MODELS: &[Model] = &[
    Model {
        kind: SceneryKind::Fountain,
        path: "environment/reference_city_v1/glb/09_fountain.glb",
        ground_offset_y: 0.,
    },
    Model {
        kind: SceneryKind::MarketStall,
        path: "environment/reference_city_v1/glb/10_market_stall.glb",
        ground_offset_y: 0.03,
    },
    Model {
        kind: SceneryKind::Counter,
        path: "environment/reference_city_v1/glb/15_merchant_counter.glb",
        ground_offset_y: 0.03,
    },
    Model {
        kind: SceneryKind::Anvil,
        path: "environment/reference_city_v1/glb/07_anvil.glb",
        ground_offset_y: 0.,
    },
    Model {
        kind: SceneryKind::Forge,
        path: "environment/reference_city_v1/glb/06_blacksmith_forge.glb",
        ground_offset_y: 0.,
    },
    Model {
        kind: SceneryKind::BlacksmithSign,
        path: "environment/reference_city_v1/glb/08_blacksmith_sign.glb",
        ground_offset_y: 0.,
    },
    Model {
        kind: SceneryKind::StreetLantern,
        path: "environment/reference_city_v1/glb/11_street_lantern.glb",
        ground_offset_y: 0.,
    },
    Model {
        kind: SceneryKind::WallLantern,
        path: "environment/reference_city_v1/glb/12_wall_lantern.glb",
        ground_offset_y: 0.,
    },
    Model {
        kind: SceneryKind::Banner,
        path: "environment/reference_city_v1/glb/13_royal_banner.glb",
        ground_offset_y: -0.076878,
    },
    Model {
        kind: SceneryKind::Crates,
        path: "environment/reference_city_v1/glb/14_wooden_crates.glb",
        ground_offset_y: 0.,
    },
    Model {
        kind: SceneryKind::SquarePlanter,
        path: "environment/reference_city_v1/glb/16_square_flower_planter.glb",
        ground_offset_y: 0.03,
    },
    Model {
        kind: SceneryKind::LongPlanter,
        path: "environment/reference_city_v1/glb/17_long_flower_planter.glb",
        ground_offset_y: 0.03,
    },
    Model {
        kind: SceneryKind::Fence,
        path: "environment/reference_city_v1/glb/19_stone_iron_fence.glb",
        ground_offset_y: -0.0085,
    },
    Model {
        kind: SceneryKind::Cobblestone,
        path: "environment/reference_city_v1/glb/20_cobblestone_patch.glb",
        ground_offset_y: 0.,
    },
];

pub(super) fn spawn(commands: &mut Commands, assets: &AssetServer, world: &mortown_world::World) {
    let scenes: Vec<_> = MODELS
        .iter()
        .map(|model| assets.load(GltfAssetLabel::Scene(0).from_asset(model.path)))
        .collect();
    for item in &world.scenery {
        let Some(index) = MODELS.iter().position(|model| model.kind == item.kind) else {
            continue;
        };
        commands.spawn((
            SceneRoot(scenes[index].clone()),
            Transform::from_xyz(
                item.position[0],
                item.position[1] + MODELS[index].ground_offset_y,
                item.position[2],
            )
            .with_rotation(Quat::from_rotation_y(item.yaw)),
        ));
    }
}
