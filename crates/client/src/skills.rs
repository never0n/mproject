use crate::{
    Movement,
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, GOLD, INK, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::{ClientMessage, skills};
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct Details;
#[derive(Component)]
pub struct AbilityLabel(u8);
#[derive(Resource, Default)]
pub struct Training(pub bool);
#[derive(Component, Clone, Copy)]
pub enum Action {
    Open,
    Training,
    Learn,
    LearnSecondary,
    LearnAbility(u8),
}
pub fn setup(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    icons: Res<crate::actionbar::Icons>,
) {
    commands.init_resource::<Training>();
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    commands
        .spawn((
            Overlay,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.16)),
            GlobalZIndex(12),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(480),
                    max_width: percent(96),
                    padding: UiRect::all(px(24)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("Навыки"),
                    TextFont {
                        font: font.clone(),
                        font_size: 22.,
                        ..default()
                    },
                    TextColor(GOLD),
                ));
                p.spawn(Node {
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::px(6, 66.),
                    column_gap: px(7),
                    row_gap: px(7),
                    ..default()
                })
                .with_children(|p| {
                    for n in 1..=skills::LAST_SKILL {
                        crate::actionbar::slot(p, crate::actionbar::Slot::Skill(n), &icons, 66.);
                    }
                });
                p.spawn((
                    Details,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 16.,
                        ..default()
                    },
                    TextColor(INK),
                ));
                for (action, label) in [
                    (Action::Training, "Развитие"),
                    (Action::Learn, "Улучшить первый навык"),
                    (Action::LearnSecondary, "Изучить второй навык · 7 SP"),
                    (Action::LearnAbility(3), ""),
                    (Action::LearnAbility(4), ""),
                    (Action::LearnAbility(5), ""),
                    (Action::LearnAbility(skills::RACIAL_SKILL), ""),
                    (Action::LearnAbility(skills::TRADER_SKILL), ""),
                ] {
                    p.spawn((
                        action,
                        Button,
                        Node {
                            padding: UiRect::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|b| {
                        let mut text = b.spawn((
                            Text::new(label),
                            TextFont {
                                font: font.clone(),
                                font_size: 15.,
                                ..default()
                            },
                            TextColor(GOLD),
                        ));
                        if let Action::LearnAbility(id) = action {
                            text.insert((AbilityLabel(id), crate::actionbar::SkillHint(id)));
                        }
                    });
                }
            });
        });
}
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut window: ResMut<CharacterWindow>,
    mut network: ResMut<Network>,
    mut movement: ResMut<Movement>,
    mut training: ResMut<Training>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
) {
    if network.mode != Mode::Online || window.mail {
        return;
    }
    let action = if keys.just_pressed(KeyCode::KeyK) {
        Some(Action::Open)
    } else {
        buttons
            .iter()
            .find(|(_, i)| **i == Interaction::Pressed)
            .map(|(a, _)| *a)
    };
    match action {
        Some(Action::Open) => {
            training.0 = false;
            window.open = !window.open || !window.skills;
            window.inventory = false;
            window.trading = false;
            window.skills = true;
            *movement = Movement::default();
        }
        Some(Action::Training) if window.open && window.skills => training.0 = !training.0,
        Some(Action::LearnSecondary) if window.open && window.skills && !network.pending => {
            network.request(ClientMessage::LearnSecondary)
        }
        Some(Action::LearnAbility(skill)) if window.open && window.skills && !network.pending => {
            network.request(ClientMessage::LearnAbility { skill });
        }
        Some(Action::Learn) if window.open && window.skills && !network.pending => {
            if let Some(a) = network.authoritative.as_ref() {
                let rank = a.progress.skill_rank + 1;
                network.request(ClientMessage::LearnSkill { rank });
            }
        }
        _ => {}
    }
}
#[allow(clippy::type_complexity)]
pub fn display(
    window: Res<CharacterWindow>,
    network: Res<Network>,
    mut overlay: Single<&mut Node, (With<Overlay>, Without<Action>)>,
    mut actions: Query<(&Action, &mut Node), Without<Overlay>>,
    mut details: Single<(&mut Text, &mut Node), (With<Details>, Without<Action>, Without<Overlay>)>,
    training: Res<Training>,
    mut labels: Query<(&AbilityLabel, &mut Text), Without<Details>>,
) {
    overlay.display = if window.open && window.skills && network.mode == Mode::Online {
        Display::Flex
    } else {
        Display::None
    };
    let Some(a) = network.authoritative.as_ref() else {
        return;
    };
    for (action, mut node) in &mut actions {
        let show = match action {
            Action::LearnAbility(id) => {
                training.0
                    && !skills::learned(&a.progress, *id)
                    && network
                        .character
                        .as_ref()
                        .and_then(|c| skills::definition(c.class, c.race, *id))
                        .is_some_and(|ability| {
                            a.progress.level >= ability.level && a.progress.sp >= ability.sp
                        })
            }
            Action::LearnSecondary => {
                training.0
                    && !a.progress.secondary_learned
                    && a.progress.level >= 3
                    && a.progress.sp >= 7.
            }
            Action::Learn => {
                training.0
                    && !a.progress.primary_disabled
                    && skills::upgrade_cost(a.progress.skill_rank + 1)
                        .is_some_and(|(level, sp)| a.progress.level >= level && a.progress.sp >= sp)
            }
            _ => true,
        };
        node.display = if show { Display::Flex } else { Display::None };
    }
    if let Some(c) = network.character.as_ref() {
        for (AbilityLabel(id), mut text) in &mut labels {
            if let Some(ability) = skills::definition(c.class, c.race, *id) {
                text.0 = format!("Изучить: {} · {} SP", ability.skill.name, ability.sp);
            }
        }
    }
    details.1.display = if training.0 {
        Display::Flex
    } else {
        Display::None
    };
    details.0.0 = format!(
        "Уровень {} · SP {:.0}\n{}",
        a.progress.level, a.progress.sp, network.status
    );
}
