use crate::{
    combat::CombatUi,
    network::{Mode, Network},
    ui::{GOLD, PANEL},
};
use bevy::prelude::*;
#[derive(Component)]
pub struct SelfPanel;
#[derive(Component)]
pub struct Panel;
#[derive(Component)]
pub struct Title;
#[derive(Component)]
pub struct Casting;
#[derive(Component)]
pub struct Meter(pub usize);
#[derive(Component)]
pub struct Fill(pub usize);
#[derive(Component)]
pub struct Resources;
#[derive(Resource, Default)]
pub struct Owner(pub Option<i64>);
pub fn setup(mut commands: Commands, icons: Res<crate::actionbar::Icons>) {
    commands.init_resource::<Owner>();
    commands
        .spawn((
            Panel,
            crate::ui::WorldInputBlocker,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: percent(40),
                top: px(18),
                width: px(300),
                padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
        ))
        .with_children(|p| {
            p.spawn((
                Button,
                crate::exchange::Invite,
                Node {
                    display: Display::None,
                    padding: UiRect::all(px(8)),
                    ..default()
                },
                BackgroundColor(crate::ui::BUTTON),
            ))
            .with_children(|b| {
                b.spawn(crate::actionbar::label("Предложить обмен", &icons.1, 13.));
            });
            p.spawn((
                Button,
                crate::party::Invite,
                Node {
                    display: Display::None,
                    padding: UiRect::all(px(8)),
                    ..default()
                },
                BackgroundColor(crate::ui::BUTTON),
            ))
            .with_children(|b| {
                b.spawn(crate::actionbar::label(
                    "Пригласить в группу",
                    &icons.1,
                    13.,
                ));
            });
            p.spawn((Title, crate::actionbar::label("", &icons.1, 16.)));
            p.spawn((Casting, crate::actionbar::label("", &icons.1, 13.)));
            p.spawn((
                Resources,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    ..default()
                },
            ))
            .with_children(|p| {
                for (i, color) in [
                    Color::srgb(0.76, 0.19, 0.19),
                    Color::srgb(0.19, 0.38, 0.75),
                    Color::srgb(0.19, 0.63, 0.19),
                ]
                .into_iter()
                .enumerate()
                {
                    p.spawn((Meter(i), crate::actionbar::label("", &icons.1, 12.)));
                    p.spawn((
                        Node {
                            height: px(8),
                            width: percent(100),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.07, 0.06, 0.05)),
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            Fill(i),
                            Node {
                                height: percent(100),
                                width: percent(0),
                                ..default()
                            },
                            BackgroundColor(color),
                        ));
                    });
                }
            });
        });
}
pub fn input(
    mut ui: ResMut<CombatUi>,
    mut network: ResMut<Network>,
    mut owner: ResMut<Owner>,
    panels: Query<&Interaction, (With<SelfPanel>, Changed<Interaction>)>,
    drag: Res<crate::movable::DragState>,
) {
    let id = network.character.as_ref().map(|c| c.id);
    if owner.0 != id || network.mode != Mode::Online {
        *ui = default();
        owner.0 = id;
        return;
    }
    if !drag.blocked && panels.iter().any(|i| *i == Interaction::Pressed) {
        ui.selected = None;
        ui.player = id;
        ui.npc = None;
        network.action(mortown_protocol::ClientMessage::Attack { target: None });
    }
}
pub fn display(
    network: Res<Network>,
    mut ui: ResMut<CombatUi>,
    mut root: Single<&mut Node, (With<Panel>, Without<Resources>, Without<Fill>)>,
    mut resources: Single<&mut Node, (With<Resources>, Without<Panel>, Without<Fill>)>,
    mut title: Single<&mut Text, (With<Title>, Without<Meter>, Without<Casting>)>,
    mut meters: Query<(&Meter, &mut Text), (Without<Title>, Without<Casting>)>,
    mut casting: Single<&mut Text, (With<Casting>, Without<Meter>, Without<Title>)>,
    mut fills: Query<(&Fill, &mut Node), (Without<Panel>, Without<Resources>)>,
) {
    casting.0 = if network.mode == Mode::Online && ui.player.is_none() && ui.npc.is_none() {
        network
            .monsters
            .iter()
            .find(|m| Some(m.id) == ui.selected && m.hp > 0.)
            .and_then(|m| m.cast.as_ref())
            .map_or_else(String::new, |cast| {
                format!(
                    "Пепельный заряд · {:.1} с",
                    cast.remaining_ms as f32 / 1000.
                )
            })
    } else {
        String::new()
    };
    let mut pools = [None; 3];
    let mut npc = false;
    let name = if network.mode != Mode::Online {
        None
    } else if let Some(id) = ui.player {
        let actor = network
            .roster
            .iter()
            .find(|a| a.id == id)
            .or_else(|| network.authoritative.as_ref().filter(|a| a.id == id));
        actor.map(|a| format!("{} · Уровень {}", a.nickname, a.progress.level))
    } else if let Some(id) = ui.npc {
        npc = true;
        pools[0] = network
            .residents
            .iter()
            .find(|n| n.id == id)
            .map(|n| (n.hp, 100.));
        crate::npcs::resolve(&network, id)
            .map(|n| format!("{} · Уровень 1\n{}", n.name, n.role.title()))
    } else if let Some(m) = network.monsters.iter().find(|m| Some(m.id) == ui.selected) {
        npc = true;
        pools = [Some((m.hp, m.max_hp)), m.mana, m.stamina];
        Some(format!("{} · Уровень {}", m.name, m.level))
    } else {
        None
    };
    if name.is_none() {
        ui.selected = None;
        ui.player = None;
        ui.npc = None;
    }
    root.display = if name.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    resources.display = if npc { Display::Flex } else { Display::None };
    title.0 = name.unwrap_or_default();
    for (Meter(i), mut t) in &mut meters {
        let name = ["Здоровье", "Мана", "Выносливость"][*i];
        t.0 = if let Some((current, max)) = pools[*i] {
            format!("{name}: {current:.0} / {max:.0}")
        } else {
            format!("{name}: не используется")
        };
    }
    for (Fill(i), mut node) in &mut fills {
        node.width = percent(pools[*i].map_or(0., |(current, max)| {
            if max > 0. {
                (current / max * 100.).clamp(0., 100.) as f32
            } else {
                0.
            }
        }));
    }
}
