use crate::{
    Movement, Terrain,
    network::{Mode, Network},
    ui::CharacterWindow,
};
use bevy::{prelude::*, tasks::AsyncComputeTaskPool};
use mortown_protocol::{ClientMessage, monsters::Kind};
#[derive(Resource, Default)]
pub struct CombatUi {
    pub selected: Option<u32>,
    pub player: Option<i64>,
}
#[derive(Component)]
pub struct CombatText;
#[derive(Component, Clone, Copy)]
pub enum Action {
    Attack,
    Skill,
    Secondary,
    Camp,
    Respawn,
}
pub fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    commands.init_resource::<CombatUi>();
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    commands.spawn((
        CombatText,
        Text::new(""),
        TextFont {
            font,
            font_size: 13.,
            ..default()
        },
        Node {
            display: Display::None,
            ..default()
        },
    ));
}
#[allow(clippy::too_many_arguments)]
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
    panel: Res<CharacterWindow>,
    mut ui: ResMut<CombatUi>,
    mut network: ResMut<Network>,
    world: Res<Terrain>,
    mut movement: ResMut<Movement>,
) {
    if network.mode != Mode::Online {
        ui.selected = None;
        return;
    }
    if panel.open {
        return;
    }
    let Some(actor) = network.authoritative.clone() else {
        return;
    };
    let from = [actor.position[0], actor.position[2]];
    let mut candidates: Vec<_> = network.monsters.iter().filter(|m| m.hp > 0.).collect();
    candidates.sort_by(|a, b| {
        let distance = |m: &mortown_protocol::Monster| {
            mortown_world::distance(from, [m.position[0], m.position[2]])
        };
        distance(a).total_cmp(&distance(b))
    });
    if keys.just_pressed(KeyCode::Tab) && !candidates.is_empty() {
        let index = candidates
            .iter()
            .position(|m| Some(m.id) == ui.selected)
            .map_or(0, |i| (i + 1) % candidates.len());
        ui.selected = Some(candidates[index].id);
        ui.player = None;
    }
    let pressed = buttons
        .iter()
        .find(|(_, i)| **i == Interaction::Pressed)
        .map(|(a, _)| *a);
    let action = if keys.just_pressed(KeyCode::KeyG) {
        Some(Action::Camp)
    } else if keys.just_pressed(KeyCode::Space) {
        Some(Action::Attack)
    } else if keys.just_pressed(KeyCode::Digit1) {
        Some(Action::Skill)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(Action::Secondary)
    } else if keys.just_pressed(KeyCode::KeyR) {
        Some(Action::Respawn)
    } else {
        pressed
    };
    if ui.player.is_some()
        && matches!(
            action,
            Some(Action::Attack | Action::Skill | Action::Secondary)
        )
    {
        network.status = "Эта цель не является противником".into();
        return;
    }
    match action {
        Some(Action::Attack) => {
            if ui.selected.is_none() {
                ui.selected = candidates.first().map(|m| m.id);
            }
            let target = if actor.attack_target == ui.selected {
                None
            } else {
                ui.selected
            };
            network.action(ClientMessage::Attack { target });
        }
        Some(Action::Skill | Action::Secondary) => {
            if ui.selected.is_none() {
                ui.selected = candidates.first().map(|m| m.id);
            }
            if let Some(target) = ui.selected {
                if matches!(action, Some(Action::Secondary)) {
                    network.action(ClientMessage::UseSecondary { target });
                } else {
                    network.action(ClientMessage::UseSkill { target });
                }
            } else {
                network.status = "Нет доступной цели".into();
            }
        }
        Some(Action::Respawn) => network.action(ClientMessage::Respawn),
        Some(Action::Camp) if actor.hp > 0. => {
            if let Some(m) = candidates.iter().find(|m| m.kind == Kind::Bandit) {
                ui.selected = Some(m.id);
                let target = [m.position[0], m.position[2]];
                let distance = mortown_world::distance(from, target);
                let range = mortown_protocol::weapons::attack_range(actor.equipment.weapon);
                let stand_off = (range * 0.75).max(1.5);
                let to = if distance > stand_off {
                    [
                        target[0] - (target[0] - from[0]) / distance * stand_off,
                        target[1] - (target[1] - from[1]) / distance * stand_off,
                    ]
                } else {
                    from
                };
                let world = world.0.clone();
                movement.path.clear();
                movement.destination = Some(to);
                movement.status = "Идём к лагерю за южными воротами".into();
                movement.pending =
                    Some(AsyncComputeTaskPool::get().spawn(async move {
                        mortown_world::navigation::find_path(&world, from, to)
                    }));
                network.action(ClientMessage::Attack { target: None });
            }
        }
        _ => {}
    }
}
pub fn display(
    network: Res<Network>,
    ui: Res<CombatUi>,
    mut text: Single<&mut Text, With<CombatText>>,
) {
    let selected = network.monsters.iter().find(|m| Some(m.id) == ui.selected);
    text.0 = if let Some(a) = network.authoritative.as_ref() {
        if a.downed {
            format!(
                "Вы без сознания · подъём через {} с\n{}",
                a.respawn_seconds, network.status
            )
        } else if a.hunger >= 80. || a.thirst >= 80. {
            format!(
                "Голод {:.0} · жажда {:.0}\n{}",
                a.hunger, a.thirst, network.status
            )
        } else if a.return_seconds > 0 {
            format!(
                "Новая личность · уйти можно через {} с\n{}",
                a.return_seconds, network.status
            )
        } else {
            let target = selected
                .map(|m| {
                    let mut info = format!(
                        "{} · {:.0}/{:.0} HP · {:.0} м{}",
                        m.name,
                        m.hp,
                        m.max_hp,
                        Vec3::from_array(a.position).distance(Vec3::from_array(m.position)),
                        if m.slow_ms > 0 {
                            format!(" · замедлен {:.1} с", m.slow_ms as f32 / 1000.)
                        } else {
                            String::new()
                        }
                    );
                    if !m.effects.is_empty() {
                        info.push('\n');
                        info.push_str(
                            &m.effects
                                .iter()
                                .map(|e| {
                                    format!(
                                        "{} {:.1} с",
                                        e.kind.name(),
                                        e.remaining_ms as f32 / 1000.
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" · "),
                        );
                    }
                    info
                })
                .unwrap_or("Цель не выбрана".into());
            let protection = if a.effects.is_empty() {
                String::new()
            } else {
                format!(
                    "\nНа вас: {}",
                    a.effects
                        .iter()
                        .map(|e| format!(
                            "{} {:.1} с",
                            e.kind.name(),
                            e.remaining_ms as f32 / 1000.
                        ))
                        .collect::<Vec<_>>()
                        .join(" · ")
                )
            };
            format!(
                "{target}{protection}\n{} · {}",
                a.combat_status, network.status
            )
        }
    } else {
        "Бой доступен после входа персонажем.\nЗа южными воротами расположен лагерь разбойников."
            .into()
    };
}
