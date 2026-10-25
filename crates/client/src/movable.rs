//! Draggable windows. At startup `setup` gives every HUD panel and overlay frame a thin
//! title strip (`Handle`); `input` moves the window while that strip is held.
//! `DragState::blocked` tells world clicks (`main::click_destination`, target, actionbar)
//! to ignore the mouse during a drag.
use crate::{Movement, ui};
use bevy::{prelude::*, window::PrimaryWindow};
/// Opt-out marker: HUD panels with it never get a drag handle (e.g. the chat).
#[derive(Component)]
pub struct Fixed;
/// Window root that can be dragged.
#[derive(Component, Default)]
pub struct Movable;
/// Title strip button; holds the window entity it drags.
#[derive(Component)]
pub struct Handle(pub Entity);
#[derive(Resource, Default)]
pub struct DragState {
    pub active: bool,
    /// True during a drag and for one frame after release, so the mouse-up is not a ground click.
    pub blocked: bool,
    target: Option<Entity>,
    last: Vec2,
}

/// Must run after every window's own `setup` (last in the Startup UI chain).
/// A new overlay must be added to `overlays` here and in `skin::setup`.
pub fn setup(
    mut commands: Commands,
    hud: Query<Entity, (With<ui::Hud>, Without<Fixed>, Without<crate::menu::Gear>)>,
    overlays: Query<
        &Children,
        Or<(
            With<ui::CharacterOverlay>,
            With<crate::inventory::InventoryOverlay>,
            With<crate::skills::Overlay>,
            With<crate::crafting::Overlay>,
            With<crate::alliance::Overlay>,
            With<crate::exchange::Overlay>,
        )>,
    >,
    lobby: Query<Entity, With<crate::lobby::Panel>>,
    bar: Query<&Children, With<crate::actionbar::Bar>>,
    target: Query<Entity, Or<(With<crate::target::Panel>, With<crate::party::Root>)>>,
    icons: Res<crate::actionbar::Icons>,
) {
    commands.init_resource::<DragState>();
    // Overlays are full-screen dimmers; the draggable frame is their first child.
    let mut targets: Vec<Entity> = hud
        .iter()
        .chain(lobby.iter())
        .chain(target.iter())
        .collect();
    for children in &bar {
        if let Some(e) = children.first() {
            targets.push(*e);
        }
    }
    for children in &overlays {
        if let Some(e) = children.first() {
            targets.push(*e);
        }
    }
    targets.sort();
    targets.dedup();
    for target in targets {
        commands
            .entity(target)
            .insert((Movable, ui::WorldInputBlocker));
        commands.entity(target).with_children(|p| {
            p.spawn((
                Handle(target),
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    top: px(0),
                    height: px(13),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.6, 0.47, 0.23, 0.25)),
            ))
            .with_children(|b| {
                // `skin::setup` later blanks this text and makes the strip transparent.
                b.spawn(crate::actionbar::label("⋮⋮⋮", &icons.1, 10.));
            });
        });
    }
}
/// Moves a window along one axis, keeping whichever edge it was anchored to
/// (left/top or right/bottom). Percent anchors are converted to pixels on first drag.
fn shift(near: &mut Val, far: &mut Val, delta: f32, extent: f32) {
    match (*near, *far) {
        (Val::Percent(n), _) => *near = px(extent * n / 100. + delta),
        (_, Val::Percent(n)) => *far = px(extent * n / 100. - delta),
        (Val::Px(n), _) => *near = px(n + delta),
        (_, Val::Px(n)) => *far = px(n - delta),
        _ => *near = px(delta),
    }
}
/// Runs before world-click systems in the Update chain so `blocked` is fresh.
pub fn input(
    mut drag: ResMut<DragState>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    handles: Query<(&Handle, &Interaction, &ComputedNode)>,
    mut windows: Query<(&mut Node, &ComputedNode, &UiGlobalTransform), With<Movable>>,
    mut movement: ResMut<Movement>,
) {
    drag.blocked = drag.active;
    if !mouse.pressed(MouseButton::Left) {
        drag.active = false;
        drag.target = None;
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    // Work in UI units (logical pixels divided by `UiScale`), the same units as `Node` offsets.
    let cursor = cursor / ui_scale.0;
    let extent = Vec2::new(window.width(), window.height()) / ui_scale.0;
    if mouse.just_pressed(MouseButton::Left)
        && let Some((handle, _, _)) = handles
            .iter()
            // Zero size = the window is hidden.
            .find(|(_, i, n)| **i == Interaction::Pressed && n.size().min_element() > 0.)
    {
        drag.target = Some(handle.0);
        drag.last = cursor;
        drag.active = true;
        drag.blocked = true;
        // Grabbing a window stops the hero's current route.
        *movement = Movement::default();
    }
    let Some(target) = drag.target else {
        return;
    };
    if let Ok((mut node, computed, transform)) = windows.get_mut(target) {
        let mut delta = cursor - drag.last;
        // Computed layout is in physical pixels; convert to UI units.
        let scale = computed.inverse_scale_factor();
        let center = transform.transform_point2(Vec2::ZERO) * scale;
        let size = computed.size() * scale;
        // Keep 40 px visible left/right, the top edge on screen and 16 px above the bottom.
        let min = Vec2::new(40. - size.x / 2., size.y / 2.);
        let max = Vec2::new(extent.x - 40. + size.x / 2., extent.y - 16. + size.y / 2.);
        let next = (center + delta).clamp(min, max.max(min));
        delta = next - center;
        let mut right = node.right;
        shift(&mut node.left, &mut right, delta.x, extent.x);
        node.right = right;
        let mut bottom = node.bottom;
        shift(&mut node.top, &mut bottom, delta.y, extent.y);
        node.bottom = bottom;
    }
    drag.last = cursor;
}
