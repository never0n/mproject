//! Shared native UI chrome. No image mockups: these are the actual interactive windows.
use crate::ui::{EDGE, GOLD, PANEL};
use bevy::{prelude::*, ui::FocusPolicy, window::PrimaryWindow};
#[derive(Component)]
pub struct Close;
/// Controls which render their own hover, focus or selection states.
#[derive(Component)]
pub struct OwnStyle;
fn shadow() -> BoxShadow {
    BoxShadow(vec![ShadowStyle {
        color: Color::srgba(0., 0., 0., 0.48),
        x_offset: px(0),
        y_offset: px(9),
        spread_radius: px(1),
        blur_radius: px(24),
    }])
}
pub fn setup(
    mut commands: Commands,
    overlays: Query<
        &Children,
        Or<(
            With<crate::ui::CharacterOverlay>,
            With<crate::inventory::InventoryOverlay>,
            With<crate::skills::Overlay>,
            With<crate::journal::Overlay>,
            With<crate::trade::TradeOverlay>,
            With<crate::crafting::Overlay>,
            With<crate::dialogue::Overlay>,
            With<crate::alliance::Overlay>,
            With<crate::warehouse::Overlay>,
        )>,
    >,
    mail: Query<(&crate::mail::Section, &Children)>,
    exchange: Query<&Children, With<crate::exchange::Overlay>>,
    own_style: Query<
        Entity,
        Or<(
            With<crate::lobby::Action>,
            With<crate::chat::Action>,
            With<crate::chat::Entry>,
            With<crate::chat::Scroll>,
            With<crate::movable::Handle>,
            With<crate::actionbar::Slot>,
            With<crate::journal::Tab>,
        )>,
    >,
    plain: Query<
        Entity,
        Or<(
            With<crate::lobby::Panel>,
            With<crate::lobby::ModelPanel>,
            With<crate::menu::Popup>,
            With<crate::target::Panel>,
            With<crate::target::SelfPanel>,
            With<crate::party::Root>,
            With<crate::alliance::Notice>,
        )>,
    >,
    mut nodes: Query<(
        &mut Node,
        Option<&mut BackgroundColor>,
        Option<&mut BorderColor>,
    )>,
    mut fonts: Query<(&mut TextFont, &mut TextColor)>,
    handles: Query<(Entity, &crate::movable::Handle)>,
    handle_texts: Query<(&ChildOf, Entity), With<Text>>,
    handle_entities: Query<(), With<crate::movable::Handle>>,
    icons: Res<crate::actionbar::Icons>,
    assets: Res<AssetServer>,
) {
    let heading = assets.load("fonts/DejaVuSerif.ttf");
    for entity in &own_style {
        commands.entity(entity).insert(OwnStyle);
    }
    for (mut font, mut color) in &mut fonts {
        if font.font_size >= 20. && font.font_size <= 32. {
            font.font = heading.clone();
            color.0 = GOLD;
        }
    }
    let mut frames: Vec<_> = overlays.iter().filter_map(|c| c.first().copied()).collect();
    for (s, c) in &mail {
        if matches!(s, crate::mail::Section::Overlay)
            && let Some(e) = c.first()
        {
            frames.push(*e);
        }
    }
    for entity in frames
        .iter()
        .copied()
        .chain(plain.iter())
        .chain(exchange.iter().filter_map(|c| c.first().copied()))
    {
        if let Ok((mut node, background, border)) = nodes.get_mut(entity) {
            node.border = UiRect::all(px(1));
            node.border_radius = BorderRadius::all(px(7));
            if let Some(mut bg) = background {
                bg.0 = PANEL;
            }
            if let Some(mut b) = border {
                *b = BorderColor::all(EDGE);
            }
        }
        commands.entity(entity).insert(shadow()).with_children(|p| {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(14),
                    right: px(14),
                    top: px(0),
                    height: px(1),
                    ..default()
                },
                BackgroundColor(GOLD),
                FocusPolicy::Pass,
            ));
            for right in [false, true] {
                p.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: if right { Val::Auto } else { px(0) },
                        right: if right { px(0) } else { Val::Auto },
                        top: px(0),
                        width: px(5),
                        height: px(5),
                        ..default()
                    },
                    BackgroundColor(GOLD),
                    FocusPolicy::Pass,
                ));
            }
        });
    }
    for entity in frames.iter().copied() {
        commands.entity(entity).with_children(|p| {
            p.spawn((
                Close,
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(12),
                    top: px(10),
                    width: px(26),
                    height: px(26),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(crate::ui::BUTTON),
                ZIndex(3),
            ))
            .with_children(|p| {
                p.spawn(crate::actionbar::label("×", &icons.1, 19.));
            });
        });
    }
    for (e, handle) in &handles {
        commands.entity(e).insert(BackgroundColor(Color::NONE));
        // Drag from the whole title strip without covering the close button.
        if frames.contains(&handle.0)
            && let Ok((mut node, _, _)) = nodes.get_mut(e)
        {
            node.height = px(44);
            node.right = px(44);
        }
    }
    for (parent, e) in &handle_texts {
        if handle_entities.contains(parent.parent()) {
            commands.entity(e).insert(Text::new(""));
        }
    }
}
pub fn close(
    mut windows: ResMut<crate::ui::CharacterWindow>,
    mut crafting: ResMut<crate::crafting::Open>,
    buttons: Query<&Interaction, (With<Close>, Changed<Interaction>)>,
) {
    if buttons.iter().any(|i| *i == Interaction::Pressed) {
        *windows = default();
        crafting.0 = false;
    }
}
pub fn scale(window: Single<&Window, With<PrimaryWindow>>, mut scale: ResMut<UiScale>) {
    let factor = (window.width() / 1440.)
        .min(window.height() / 900.)
        .clamp(0.75, 1.15);
    if (scale.0 - factor).abs() > 0.005 {
        scale.0 = factor;
    }
}
