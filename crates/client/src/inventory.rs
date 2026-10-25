//! Inventory window ("Снаряжение", key I): hero preview, weapon/armour slots, 30-slot bag pages.
//! Slot behaviour (icons, drag, tooltips) lives in `actionbar`; this module only builds the
//! window and pages. It also creates the two off-screen 3D cameras: the HUD portrait and the
//! model preview (shared with the lobby gallery). Both render only `RenderLayers` layer 1,
//! where the preview model and its lights live, so the main world never appears in them.
use crate::{
    Movement,
    actionbar::{self, Icons, ModelDrop, Slot},
    network::Network,
    ui::{CharacterWindow, GOLD, PANEL},
};
use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::render_resource::TextureFormat,
    scene::SceneInstanceReady,
};
/// Marker for buttons that open the inventory (menu entry).
#[derive(Component)]
pub struct OpenInventory;
/// Current bag page (0-based); real bag index = page × 30 + visible slot.
#[derive(Resource, Default)]
pub struct BagPage(pub u16);
#[derive(Component)]
pub struct PageButton(i16);
#[derive(Component)]
pub struct InventoryOverlay;
/// Renders the preview model into an image (inventory and lobby). Active only when shown.
#[derive(Component)]
pub struct PreviewCamera;
/// Renders the hero's face for the HUD portrait; always active.
#[derive(Component)]
pub struct PortraitCamera;
#[derive(Component)]
pub struct Summary;

/// Needs `ui::Portrait`, `lobby::ModelImage` and `Icons` to exist (runs after their setups).
pub fn setup(
    mut commands: Commands,
    icons: Res<Icons>,
    assets: Res<AssetServer>,
    portrait: Single<Entity, With<crate::ui::Portrait>>,
    gallery: Single<Entity, With<crate::lobby::ModelImage>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.init_resource::<BagPage>();
    let portrait_target = images.add(Image::new_target_texture(
        128,
        150,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    commands
        .entity(*portrait)
        .insert(ImageNode::new(portrait_target.clone()));
    commands.spawn((
        PortraitCamera,
        Camera3d::default(),
        bevy::core_pipeline::tonemapping::Tonemapping::AcesFitted,
        // Negative order: render to texture before the main camera (order 0) uses it.
        Camera {
            order: -2,
            clear_color: Color::srgb(0.055, 0.075, 0.092).into(),
            ..default()
        },
        RenderTarget::Image(portrait_target.into()),
        RenderLayers::layer(1),
        // Initial framing only; `ancestry::frame_preview` re-aims both cameras at the model's head/body.
        Transform::from_xyz(0., 1.70, 0.68).looking_at(Vec3::new(0., 1.70, 0.), Vec3::Y),
    ));
    let target = images.add(Image::new_target_texture(
        384,
        512,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    commands
        .entity(*gallery)
        .insert(ImageNode::new(target.clone()));
    commands.spawn((
        PreviewCamera,
        Camera3d::default(),
        bevy::core_pipeline::tonemapping::Tonemapping::AcesFitted,
        Camera {
            order: -1,
            is_active: false,
            clear_color: Color::srgb(0.045, 0.062, 0.078).into(),
            ..default()
        },
        RenderTarget::Image(target.clone().into()),
        RenderLayers::layer(1),
        Transform::from_xyz(0., 1.0, 3.1).looking_at(Vec3::new(0., 0.9, 0.), Vec3::Y),
    ));
    commands
        .spawn((
            SceneRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset("characters/adventurer.glb")),
            ),
            crate::appearance::Wearer::Preview,
            Transform::from_rotation(Quat::from_rotation_y(-0.3)),
            RenderLayers::layer(1),
        ))
        // Render layers are not inherited: tag every mesh of the loaded glTF once it spawns.
        .observe(
            |ready: On<SceneInstanceReady>, children: Query<&Children>, mut commands: Commands| {
                for e in children.iter_descendants(ready.entity) {
                    commands.entity(e).insert(RenderLayers::layer(1));
                }
            },
        );
    commands.spawn((
        PointLight {
            intensity: 350_000.,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(2., 3., 3.),
        RenderLayers::layer(1),
    ));
    commands.spawn((
        PointLight {
            intensity: 180_000.,
            shadows_enabled: false,
            color: Color::srgb(0.55, 0.7, 1.),
            ..default()
        },
        Transform::from_xyz(-2., 2., 1.),
        RenderLayers::layer(1),
    ));
    commands
        .spawn((
            InventoryOverlay,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                padding: UiRect::bottom(px(100)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.16)),
            // Just above the character sheet (10).
            GlobalZIndex(11),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(820),
                    max_width: percent(96),
                    padding: UiRect::all(px(24)),
                    border: UiRect::all(px(1)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|p| {
                p.spawn(actionbar::label("Снаряжение", &icons.1, 22.));
                p.spawn((Summary, actionbar::label("", &icons.1, 14.)));
                p.spawn(Node {
                    column_gap: px(24),
                    ..default()
                })
                .with_children(|p| {
                    p.spawn(Node {
                        width: px(292),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    })
                    .with_children(|p| {
                        p.spawn(actionbar::label("Экипировка", &icons.1, 16.));
                        p.spawn((
                            ModelDrop,
                            Button,
                            ImageNode::new(target),
                            Node {
                                width: px(264),
                                height: px(334),
                                align_self: AlignSelf::Center,
                                ..default()
                            },
                        ));
                        p.spawn(Node {
                            column_gap: px(18),
                            justify_content: JustifyContent::Center,
                            ..default()
                        })
                        .with_children(|p| {
                            for (weapon, title) in [(true, "Оружие"), (false, "Доспех")]
                            {
                                p.spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(4),
                                    align_items: AlignItems::Center,
                                    ..default()
                                })
                                .with_children(|p| {
                                    actionbar::slot(p, Slot::Equipment(weapon), &icons, 54.);
                                    p.spawn(actionbar::label(title, &icons.1, 12.));
                                });
                            }
                        });
                    });
                    p.spawn(Node {
                        width: px(420),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(10),
                        ..default()
                    })
                    .with_children(|p| {
                        p.spawn(Node {
                            column_gap: px(10),
                            ..default()
                        })
                        .with_children(|p| {
                            p.spawn(actionbar::label("Рюкзак", &icons.1, 16.));
                            for (step, title) in [(-1, "←"), (1, "→")] {
                                p.spawn((
                                    PageButton(step),
                                    Button,
                                    Node {
                                        padding: UiRect::axes(px(12), px(2)),
                                        ..default()
                                    },
                                    BackgroundColor(crate::ui::BUTTON),
                                ))
                                .with_children(|p| {
                                    p.spawn(actionbar::label(title, &icons.1, 14.));
                                });
                            }
                        });
                        p.spawn(Node {
                            display: Display::Grid,
                            grid_template_columns: RepeatedGridTrack::px(6, 58.),
                            grid_template_rows: RepeatedGridTrack::px(5, 58.),
                            column_gap: px(7),
                            row_gap: px(7),
                            ..default()
                        })
                        .with_children(|p| {
                            for n in 0..30 {
                                actionbar::slot(p, Slot::Bag(n), &icons, 58.);
                            }
                        });
                        p.spawn(actionbar::label(
                            "ПКМ — в открытый сундук или поставить сундук · Shift+ПКМ — на землю · [E] — подобрать/открыть",
                            &icons.1,
                            12.,
                        ));
                    });
                });
                // Handled by `crafting::input`, which swaps the bag for the crafting panel.
                p.spawn((
                    Button,
                    crate::crafting::Action::Open,
                    Node {
                        width: px(142),
                        align_self: AlignSelf::FlexEnd,
                        border_radius: BorderRadius::all(px(4)),
                        padding: UiRect::all(px(8)),
                        ..default()
                    },
                    BackgroundColor(crate::ui::BUTTON),
                ))
                .with_children(|p| {
                    p.spawn(actionbar::label("Ремесло", &icons.1, 14.));
                });
            });
        });
}
/// I / menu entry toggles the window; PageUp/PageDown or the ←/→ buttons change bag page.
pub fn input(
    mut state: ResMut<CharacterWindow>,
    keys: Res<ButtonInput<KeyCode>>,
    network: Res<Network>,
    mut movement: ResMut<Movement>,
    mut page: ResMut<BagPage>,
    pages: Query<(&PageButton, &Interaction), Changed<Interaction>>,
    toggles: Query<&Interaction, (With<OpenInventory>, Changed<Interaction>)>,
) {
    if state.open && state.inventory {
        let direction = if keys.just_pressed(KeyCode::PageDown) {
            1
        } else if keys.just_pressed(KeyCode::PageUp) {
            -1
        } else {
            pages
                .iter()
                .find(|(_, i)| **i == Interaction::Pressed)
                .map_or(0, |(b, _)| b.0)
        };
        // Allow one empty page past the last used one. 2183 is the last page whose
        // 30 slots still fit the protocol's u16 bag index.
        if let Some(c) = network.character.as_ref() {
            let max = mortown_protocol::layout::bag(c)
                .last_key_value()
                .map_or(0, |(n, _)| n / 30)
                + 1;
            page.0 = (i32::from(page.0) + i32::from(direction)).clamp(0, i32::from(max.min(2183)))
                as u16;
        }
    }
    if !network.playing() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyI) || toggles.iter().any(|i| *i == Interaction::Pressed) {
        // From another side window, I switches to the inventory instead of closing.
        state.open = !state.open || !state.inventory;
        state.inventory = true;
        state.skills = false;
        *movement = Movement::default();
        movement.status = "Маршрут остановлен".into();
    }
}
pub fn display(
    crafting: Res<crate::crafting::Open>,
    state: Res<CharacterWindow>,
    network: Res<Network>,
    form: Res<crate::lobby::Form>,
    mut root: Single<&mut Node, With<InventoryOverlay>>,
    mut camera: Single<&mut Camera, With<PreviewCamera>>,
    mut summary: Single<&mut Text, With<Summary>>,
    page: Res<BagPage>,
) {
    let show = state.open && state.inventory && network.playing() && !crafting.0;
    root.display = if show { Display::Flex } else { Display::None };
    // Off-screen render costs GPU time, so the preview camera runs only when visible.
    camera.is_active = show || form.model_preview(&network).is_some();
    summary.0 = network
        .character
        .as_ref()
        .map(|c| {
            use mortown_protocol::items;
            format!(
                "{}   ·   Груз {:.1} / {:.0} кг   ·   Кошель {} ({} / {} монет)   ·   Страница {}",
                c.nickname,
                items::carried_weight(&c.items),
                items::CARRY_LIMIT,
                items::format_coins(items::purse_value(&c.items)),
                items::coin_count(items::purse_value(&c.items)),
                items::PURSE_LIMIT,
                page.0 + 1
            )
        })
        .unwrap_or("Войдите персонажем, чтобы открыть его вещи".into());
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::Mode;
    #[test]
    fn inventory_and_character_windows_switch_and_escape_is_consumed() {
        let mut app = App::new();
        let mut network = Network::default();
        network.mode = Mode::Offline;
        app.insert_resource(network)
            .init_resource::<Movement>()
            .init_resource::<BagPage>()
            .init_resource::<CharacterWindow>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, (input, crate::ui::character_window).chain());
        app.world_mut()
            .spawn((crate::ui::CharacterOverlay, Node::default()));
        app.world_mut()
            .spawn((crate::ui::CharacterDetails, Text::new("")));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyI);
        app.update();
        assert!(app.world().resource::<CharacterWindow>().open);
        assert!(app.world().resource::<CharacterWindow>().inventory);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyC);
        app.update();
        assert!(app.world().resource::<CharacterWindow>().open);
        assert!(!app.world().resource::<CharacterWindow>().inventory);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyI);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<CharacterWindow>().open);
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Escape)
        );
    }
}
