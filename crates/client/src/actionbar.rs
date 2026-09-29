use crate::{
    combat::CombatUi,
    network::{Mode, Network},
    ui::{CharacterWindow, EDGE, GOLD, INK, PANEL},
};
use bevy::{prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use mortown_protocol::{
    Character, ClientMessage,
    items::definition,
    layout::{self, Place},
};

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    Bag(usize),
    Skill(u8),
    Bar(u8),
    Equipment(bool),
}
#[derive(Component)]
pub struct ModelDrop;
#[derive(Component)]
pub struct ItemHint(pub u8);
#[derive(Component)]
pub struct SkillHint(pub u8);
#[derive(Component)]
pub struct SlotIcon(Slot);
#[derive(Component)]
pub struct SlotText(Slot);
#[derive(Component)]
pub struct Bar;
#[derive(Component)]
pub struct Tooltip;
#[derive(Component)]
pub struct TooltipCard;
#[derive(Component)]
pub struct Ghost;
#[derive(Resource)]
pub struct Icons(pub Vec<Handle<Image>>, pub Handle<Font>);
#[derive(Resource, Default)]
pub struct Drag {
    held: Option<(Slot, u64, Vec2)>,
    owner: Option<i64>,
}

fn place(s: Slot) -> Place {
    match s {
        Slot::Bag(n) => Place::Bag(n as u16),
        Slot::Bar(n) => Place::Bar(n),
        Slot::Skill(n) => Place::Skill(n),
        Slot::Equipment(w) => Place::Equipment(w),
    }
}
fn absolute(s: Slot, page: u16) -> Slot {
    if let Slot::Bag(n) = s {
        Slot::Bag(n + usize::from(page) * 30)
    } else {
        s
    }
}
pub fn binding(c: &Character, s: Slot) -> u64 {
    layout::at(c, place(s))
}
fn available(c: &Character, b: u64) -> bool {
    layout::valid(c, b)
}
pub fn item_icon(kind: u8) -> usize {
    match kind {
        9 => 13,
        10 => 14,
        11 => 25,
        12 => 26,
        13..=26 => usize::from(kind) + 14,
        _ => kind as usize,
    }
}
fn icon(c: &Character, b: u64) -> usize {
    match b {
        1 => {
            if c.class == 1 {
                9
            } else {
                11
            }
        }
        2 => {
            if c.class == 1 {
                10
            } else {
                12
            }
        }
        3..=7 => {
            mortown_protocol::skills::definition(c.class, c.race, b as u8).map_or(0, |a| a.icon)
        }
        _ => item_icon(layout::kind(b).unwrap_or(0)),
    }
}
fn description(c: &Character, b: u64, network: &Network) -> String {
    if (3..=7).contains(&b) {
        let Some(ability) = mortown_protocol::skills::definition(c.class, c.race, b as u8) else {
            return String::new();
        };
        let s = &ability.skill;
        if ability.effect == mortown_protocol::skills::Effect::Trader {
            return format!(
                "{}\n{}\nИзучение: уровень {}, {} SP\nНе требует активации или ячейки панели.",
                s.name, ability.description, ability.level, ability.sp
            );
        }
        let cd = network
            .authoritative
            .as_ref()
            .map_or(0, |a| mortown_protocol::skills::cooldown(a, b as u8));
        let damage = if s.multiplier > 0. {
            format!(
                "Урон: {:.0}% {} атаки\n",
                s.multiplier * 100.,
                if c.class == 1 {
                    "физической"
                } else {
                    "магической"
                }
            )
        } else {
            String::new()
        };
        let targeting = if ability.needs_target() {
            format!("Дальность: {:.0} м · по выбранной цели", s.range)
        } else {
            "Применение на себя, цель не нужна".into()
        };
        return format!(
            "{}\n{damage}Расход: {:.0} {}\n{targeting}\nПерезарядка: {:.1} с · Осталось: {:.1} с\n{}\nИзучение: уровень {}, {} SP\nПеретащите на панель F1–F12.",
            s.name,
            s.cost,
            if c.class == 1 {
                "выносливости"
            } else {
                "маны"
            },
            s.cooldown_ticks as f32 / 20.,
            cd as f32 / 1000.,
            ability.description,
            ability.level,
            ability.sp
        );
    }
    if b == 1 || b == 2 {
        let rank = c.stats.get("skill_rank").copied().unwrap_or(1.) as u8;
        let skill = if b == 1 {
            mortown_protocol::skills::ranked(c.class, rank)
        } else {
            mortown_protocol::skills::secondary(c.class)
        }
        .unwrap();
        let cd = network.authoritative.as_ref().map_or(0, |a| {
            if b == 1 {
                a.skill_cooldown_ms
            } else {
                a.secondary_cooldown_ms
            }
        });
        let mut text = format!(
            "{}{}\nУрон: {:.0}% {} атаки\nРасход: {:.0} {} · Дальность: {:.0} м\nПерезарядка: {:.1} с · Осталось: {:.1} с",
            skill.name,
            if b == 1 {
                format!(" · ранг {rank}/3")
            } else {
                String::new()
            },
            skill.multiplier * 100.,
            if c.class == 1 {
                "физической"
            } else {
                "магической"
            },
            skill.cost,
            if c.class == 1 {
                "выносливости"
            } else {
                "маны"
            },
            skill.range,
            skill.cooldown_ticks as f32 / 20.,
            cd as f32 / 1000.
        );
        if b == 2 {
            text.push_str("\nПри попадании: замедление 40% на 4 с");
        }
        if b == 1 {
            if let Some((level, sp)) = mortown_protocol::skills::upgrade_cost(rank + 1) {
                text.push_str(&format!("\nСледующий ранг: уровень {level}, {sp} SP"));
            } else {
                text.push_str("\nМаксимальный ранг");
            }
        }
        text.push_str("\nПрименение по выбранной цели.\nПеретащите на панель F1–F12.");
        return text;
    }
    let Some(k) = layout::kind(b) else {
        return String::new();
    };
    let mut text = item_description(c, k, network.exchange.is_none());
    if mortown_protocol::items::equippable(k) && layout::valid(c, b) {
        text.push_str(
            if layout::equipped(c, definition(k).unwrap().slot == "Правая рука") == b {
                "\nНадето"
            } else {
                "\nВ рюкзаке"
            },
        );
    }
    text
}
/// Shared item facts for hover cards and the shop's persistent inspection panel.
pub(crate) fn item_description(c: &Character, k: u8, show_quantity: bool) -> String {
    let Some(d) = definition(k) else {
        return String::new();
    };
    if mortown_protocol::items::currency(k).is_some() {
        return format!(
            "{}\nВалюта · Количество: {}\nРасходуется автоматически при покупке.\nМожно перемещать между ячейками рюкзака.",
            d.name,
            layout::quantity(c, k)
        );
    }
    let mut text = format!(
        "{}\n{} · Обычный\nКласс: {}",
        d.name,
        d.slot,
        match d.class {
            Some(1) => "Воин",
            Some(2) => "Маг",
            _ => "Любой",
        }
    );
    if d.bonus > 0. {
        text.push_str(&format!(
            "\n{} +{:.0}\nНе складывается в стопку",
            match d.stat {
                "f_attack" => "Физическая атака",
                "m_attack" => "Магическая атака",
                _ => "Физическая защита",
            },
            d.bonus
        ));
    }
    if let Some(weapon) = mortown_protocol::weapons::definition(k) {
        text.push_str(&format!(
            "\n{} · {}\nУровень: {} · Дистанция автоатаки: {:.1} м\nИнтервал ударов: ×{:.2}",
            weapon.name,
            if weapon.hands == 2 {
                "Две руки"
            } else {
                "Одна рука"
            },
            weapon.level,
            weapon.range,
            weapon.interval_scale
        ));
        if weapon.family == mortown_protocol::weapons::Family::Bow {
            text.push_str("\nФизическая стрельба; стрелы пока не расходуются.");
        }
    }
    if !mortown_protocol::items::equippable(k) && show_quantity {
        text.push_str(&format!("\nКоличество: {}", layout::quantity(c, k)));
    }
    if k == 8 {
        text.push_str("\nВосстанавливает до 50 HP. Не расходуется при полном здоровье.");
    }
    if let Some(source) = match k {
        7 | 9 => Some("Добывается с разбойников за южными воротами"),
        13 => Some("Добывается с мародёров дальше по южному тракту"),
        14 => Some("Добывается с пепельных отступников за поворотом южного тракта"),
        _ => None,
    } {
        text.push_str(&format!(
            "\nМатериал для ремесла. {source}.\nИспользовать: инвентарь → Ремесло."
        ));
    }
    if let Some(recipe) = mortown_protocol::crafting::definition(k) {
        text.push_str(&format!(
            "\nСоздаёт: {} ×1",
            definition(recipe.result).unwrap().name
        ));
        text.push_str(&format!("\n{}", recipe.profession.summary(&c.stats)));
        if recipe.profession.mastery(&c.stats).level < mortown_protocol::professions::MAX_LEVEL {
            text.push_str(&format!(
                "\nИзготовление: +{} опыта ремесла",
                mortown_protocol::professions::CRAFT_XP
            ));
        }
        for (kind, count) in recipe.requirements(&c.stats) {
            text.push_str(&format!(
                "\n{}: {} / {count}",
                definition(kind).unwrap().name,
                layout::quantity(c, kind)
            ));
        }
        text.push_str(
            "\nРецепт не расходуется.\nСоздать вещь: Ремесло или закрепить рецепт на F1–F12.",
        );
    }
    if let Some(price) = mortown_protocol::items::sell_price(k) {
        text.push_str(&format!("\nПродажа торговцу: {price} монет за штуку"));
    }
    if let Some(price) = mortown_protocol::items::purchase_price(
        k,
        &mortown_protocol::progression::Progress::from_stats(&c.stats),
    ) {
        text.push_str(&format!("\nПокупка у торговца: {price} монет"));
    }
    text
}
pub fn label(value: impl Into<String>, font: &Handle<Font>, size: f32) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(INK),
    )
}
pub fn slot(p: &mut ChildSpawnerCommands, kind: Slot, icons: &Icons, size: f32) {
    p.spawn((
        kind,
        Button,
        Node {
            width: px(size),
            height: px(size),
            flex_shrink: 0.,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.025, 0.035, 0.045)),
        BorderColor::all(EDGE),
    ))
    .with_children(|p| {
        p.spawn((
            SlotIcon(kind),
            ImageNode::new(icons.0[0].clone()),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            FocusPolicy::Pass,
        ));
        if let Slot::Bar(n) = kind {
            p.spawn((
                label(format!("F{}", n + 1), &icons.1, 10.),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(3),
                    top: px(1),
                    ..default()
                },
                BackgroundColor(Color::srgba(0., 0., 0., 0.75)),
                FocusPolicy::Pass,
            ));
        }
        p.spawn((
            SlotText(kind),
            label("", &icons.1, 12.),
            Node {
                position_type: PositionType::Absolute,
                right: px(3),
                bottom: px(1),
                ..default()
            },
            BackgroundColor(Color::srgba(0., 0., 0., 0.75)),
            FocusPolicy::Pass,
        ));
    });
}
pub fn setup(mut commands: Commands, assets: Res<AssetServer>, mut fonts: ResMut<Assets<Font>>) {
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    let icons = Icons(
        (0..=40)
            .map(|n| {
                if n == 0 {
                    Handle::default()
                } else {
                    assets.load(format!("icons/{n}.png"))
                }
            })
            .collect(),
        font,
    );
    commands.init_resource::<Drag>();
    commands
        .spawn((
            Bar,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(18),
                width: percent(100),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(30),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    padding: UiRect::all(px(10)),
                    border_radius: BorderRadius::all(px(7)),
                    column_gap: px(5),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|p| {
                for n in 0..12 {
                    slot(p, Slot::Bar(n), &icons, 46.);
                }
            });
        });
    commands
        .spawn((
            TooltipCard,
            Node {
                position_type: PositionType::Absolute,
                top: px(8),
                left: px(8),
                width: px(420),
                padding: UiRect::all(px(14)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(5)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
            BoxShadow(vec![ShadowStyle {
                color: Color::srgba(0., 0., 0., 0.5),
                y_offset: px(6),
                blur_radius: px(16),
                ..default()
            }]),
            GlobalZIndex(32),
            FocusPolicy::Pass,
        ))
        .with_children(|p| {
            p.spawn((Tooltip, label("", &icons.1, 14.), FocusPolicy::Pass));
        });
    commands.spawn((
        Ghost,
        ImageNode::default(),
        Node {
            position_type: PositionType::Absolute,
            width: px(48),
            height: px(48),
            display: Display::None,
            ..default()
        },
        GlobalZIndex(40),
        FocusPolicy::Pass,
    ));
    commands.insert_resource(icons);
}
fn activate(b: u64, network: &mut Network, combat: &mut CombatUi) {
    if network.pending {
        return;
    }
    let Some(c) = network.character.as_ref() else {
        return;
    };
    if !available(c, b) {
        network.status = "Предмет отсутствует или навык не изучен".into();
        return;
    }
    match b {
        1..=6 => {
            if b >= 3
                && mortown_protocol::skills::definition(c.class, c.race, b as u8)
                    .is_some_and(|a| !a.needs_target())
            {
                network.action(ClientMessage::UseAbility {
                    skill: b as u8,
                    target: None,
                });
                return;
            }
            if combat.player.is_some() || combat.npc.is_some() {
                network.status = "Эта цель не является противником".into();
                return;
            }
            if combat.selected.is_none() {
                let position = network
                    .authoritative
                    .as_ref()
                    .map(|a| a.position)
                    .unwrap_or_default();
                combat.selected = network
                    .monsters
                    .iter()
                    .filter(|m| m.hp > 0.)
                    .min_by(|a, b| {
                        let d = |m: &mortown_protocol::Monster| {
                            (m.position[0] - position[0]).hypot(m.position[2] - position[2])
                        };
                        d(a).total_cmp(&d(b))
                    })
                    .map(|m| m.id);
            }
            if let Some(target) = combat.selected {
                network.action(match b {
                    1 => ClientMessage::UseSkill { target },
                    2 => ClientMessage::UseSecondary { target },
                    _ => ClientMessage::UseAbility {
                        skill: b as u8,
                        target: Some(target),
                    },
                });
            } else {
                network.status = "Выберите цель: Tab или щелчок по противнику".into();
            }
        }
        7 => network.status = "Торгаш действует постоянно: скидка уже учтена в лавке".into(),
        _ if layout::kind(b) == Some(8) => network.request(ClientMessage::UsePotion),
        _ if layout::kind(b)
            .is_some_and(|k| mortown_protocol::crafting::definition(k).is_some()) =>
        {
            let recipe = layout::kind(b).unwrap();
            let expected = mortown_protocol::crafting::count(c);
            let specialization_revision = mortown_protocol::professions::revision(&c.stats);
            network.request(ClientMessage::Craft {
                recipe,
                expected,
                specialization_revision,
            });
        }
        _ if layout::kind(b).is_some_and(mortown_protocol::items::equippable) => {
            let weapon = definition(layout::kind(b).unwrap()).unwrap().slot == "Правая рука";
            let source = if layout::equipped(c, weapon) == b {
                Place::Equipment(weapon)
            } else {
                Place::Bag(*layout::bag(c).iter().find(|(_, v)| **v == b).unwrap().0)
            };
            let target = if matches!(source, Place::Equipment(_)) {
                Place::Bag(
                    (0..=u16::MAX)
                        .find(|n| !layout::bag(c).contains_key(n))
                        .unwrap(),
                )
            } else {
                Place::Model
            };
            network.request(ClientMessage::Arrange {
                source,
                target,
                expected: b,
            });
        }
        _ => network.status = "Материал используется в ремесле: инвентарь [I] → Ремесло".into(),
    }
}
/// Hit testing uses physical pixels, just like Bevy UI layout and transforms.
pub(crate) fn contains(node: &ComputedNode, transform: &UiGlobalTransform, cursor: Vec2) -> bool {
    let p = transform
        .try_inverse()
        .map(|t| t.transform_point2(cursor))
        .unwrap_or(Vec2::INFINITY);
    node.size().min_element() > 0. && p.abs().cmple(node.size() / 2.).all()
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn input(
    mut drag: ResMut<Drag>,
    windows: Res<crate::movable::DragState>,
    mut network: ResMut<Network>,
    mut combat: ResMut<CombatUi>,
    panel: Res<CharacterWindow>,
    crafting: Res<crate::crafting::Open>,
    mut page: ResMut<crate::inventory::BagPage>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    slots: Query<(&Slot, &ComputedNode, &UiGlobalTransform)>,
    models: Query<(&ComputedNode, &UiGlobalTransform), With<ModelDrop>>,
) {
    let owner = network.character.as_ref().map(|c| c.id);
    if drag.owner != owner || network.mode != Mode::Online || keys.just_pressed(KeyCode::Escape) {
        drag.held = None;
        drag.owner = owner;
        page.0 = 0;
    }
    if network.chat.dialog.is_some()
        || windows.blocked
        || network.mode != Mode::Online
        || panel.mail
        || panel.menu
        || panel.journal
        || panel.dialogue
        || panel.alliance
    {
        drag.held = None;
        return;
    }
    let cursor = window.physical_cursor_position();
    let visible = |s: Slot| match s {
        Slot::Bar(_) => true,
        Slot::Skill(_) => panel.open && panel.skills,
        _ => panel.open && panel.inventory && !crafting.0,
    };
    let hover = cursor.and_then(|p| {
        slots
            .iter()
            .find(|(s, n, t)| visible(**s) && contains(n, t, p))
            .map(|(s, _, _)| absolute(*s, page.0))
    });
    if mouse.just_pressed(MouseButton::Right)
        && let Some(Slot::Bar(slot)) = hover
        && !network.pending
        && let Some(c) = network.character.as_ref()
    {
        let expected = binding(c, Slot::Bar(slot));
        if expected != 0 {
            network.request(ClientMessage::Arrange {
                source: Place::Bar(slot),
                target: Place::Clear,
                expected,
            });
        }
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some(s) = hover
        && let Some(c) = network.character.as_ref()
    {
        let b = binding(c, s);
        if b != 0
            && b != u64::from(mortown_protocol::skills::TRADER_SKILL)
            && (available(c, b) || matches!(s, Slot::Bar(_)))
        {
            drag.held = Some((s, b, cursor.unwrap()));
        }
    }
    if drag.held.is_some_and(|(source, _, _)| !visible(source)) {
        drag.held = None;
    }
    if mouse.just_released(MouseButton::Left)
        && let Some((source, b, start)) = drag.held.take()
    {
        if network.pending {
            return;
        }
        if let Some(p) = cursor {
            let moved = p.distance(start) > 6.;
            if moved {
                let target = hover.map(place).or_else(|| {
                    if panel.open
                        && panel.inventory
                        && !crafting.0
                        && models.iter().any(|(n, t)| contains(n, t, p))
                    {
                        Some(Place::Model)
                    } else {
                        None
                    }
                });
                if let Some(target) = target {
                    network.request(ClientMessage::Arrange {
                        source: place(source),
                        target,
                        expected: b,
                    });
                }
            } else if hover == Some(source) && matches!(source, Slot::Bar(_)) && !panel.open {
                activate(b, &mut network, &mut combat);
            }
        }
    }
    if !panel.open
        && !keys.pressed(KeyCode::ShiftLeft)
        && !keys.pressed(KeyCode::ShiftRight)
        && !keys.pressed(KeyCode::ControlLeft)
        && !keys.pressed(KeyCode::ControlRight)
    {
        let functions = [
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
        ];
        for (n, key) in functions.iter().enumerate() {
            if keys.just_pressed(*key)
                && let Some(c) = network.character.as_ref()
            {
                let b = binding(c, Slot::Bar(n as u8));
                if b != 0 {
                    activate(b, &mut network, &mut combat);
                }
                break;
            }
        }
    }
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn display(
    network: Res<Network>,
    panel: Res<CharacterWindow>,
    crafting: Res<crate::crafting::Open>,
    icons: Res<Icons>,
    drag: Res<Drag>,
    page: Res<crate::inventory::BagPage>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut bar: Single<&mut Node, (With<Bar>, Without<Ghost>, Without<TooltipCard>)>,
    mut pictures: Query<(&SlotIcon, &mut ImageNode), Without<Ghost>>,
    mut slot_nodes: Query<(&Slot, &mut Node), (Without<Bar>, Without<TooltipCard>, Without<Ghost>)>,
    mut labels: Query<(&SlotText, &mut Text), Without<Tooltip>>,
    mut slots: Query<(&Slot, &ComputedNode, &UiGlobalTransform, &mut BorderColor)>,
    mut tooltip: (
        Single<&mut Text, With<Tooltip>>,
        Single<&mut Node, (With<TooltipCard>, Without<Bar>, Without<Ghost>)>,
    ),
    mut ghost: Single<
        (&mut ImageNode, &mut Node),
        (With<Ghost>, Without<Bar>, Without<TooltipCard>),
    >,
    hints: Query<
        (
            Option<&ItemHint>,
            Option<&SkillHint>,
            &ComputedNode,
            &UiGlobalTransform,
        ),
        Or<(With<ItemHint>, With<SkillHint>)>,
    >,
) {
    bar.display = if network.mode == Mode::Online {
        Display::Flex
    } else {
        Display::None
    };
    tooltip.1.display = Display::None;
    ghost.1.display = Display::None;
    if panel.mail || panel.menu || panel.journal || panel.dialogue || panel.alliance {
        return;
    }
    let Some(c) = network.character.as_ref() else {
        return;
    };
    let cursor = window.physical_cursor_position();
    for (s, mut node) in &mut slot_nodes {
        if matches!(s, Slot::Skill(_)) {
            node.display = if binding(c, *s) != 0 {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    for (SlotIcon(s), mut image) in &mut pictures {
        let b = binding(c, absolute(*s, page.0));
        image.image = icons.0[icon(c, b)].clone();
        image.color = if b == 0 { Color::NONE } else { Color::WHITE };
    }
    for (SlotText(s), mut text) in &mut labels {
        let b = binding(c, absolute(*s, page.0));
        let cooldown = network.authoritative.as_ref().map_or(0, |a| {
            if (1..=u64::from(mortown_protocol::skills::LAST_ACTIVE)).contains(&b) {
                mortown_protocol::skills::cooldown(a, b as u8)
            } else {
                0
            }
        });
        text.0 = if cooldown > 0 {
            format!("{:.1}", cooldown as f32 / 1000.)
        } else if layout::kind(b).is_some_and(|k| !mortown_protocol::items::equippable(k)) {
            format!("{}", layout::quantity(c, layout::kind(b).unwrap()))
        } else {
            String::new()
        };
    }
    for (s, n, t, mut border) in &mut slots {
        let visible = match s {
            Slot::Bar(_) => network.mode == Mode::Online,
            Slot::Skill(_) => panel.open && panel.skills,
            _ => panel.open && panel.inventory && !crafting.0,
        };
        let hover = visible && cursor.is_some_and(|p| contains(n, t, p));
        *border = BorderColor::all(if hover { GOLD } else { EDGE });
        if hover {
            let b = binding(c, absolute(*s, page.0));
            if b != 0 {
                tooltip.0.0 = description(c, b, &network);
                tooltip.1.display = Display::Flex;
            }
        }
    }
    if let Some(p) = cursor {
        for (item, skill, node, transform) in &hints {
            if contains(node, transform, p) {
                let token =
                    if crafting.0 || network.exchange.is_some() || panel.open && panel.trading {
                        item.filter(|i| i.0 != 0).map(|i| layout::token(i.0, 0))
                    } else if panel.open && panel.skills {
                        skill.map(|s| u64::from(s.0))
                    } else {
                        None
                    };
                if let Some(token) = token {
                    tooltip.0.0 = if panel.open && panel.trading {
                        item_description(c, layout::kind(token).unwrap_or(0), false)
                    } else {
                        description(c, token, &network)
                    };
                    tooltip.1.display = Display::Flex;
                }
            }
        }
    }
    if tooltip.1.display == Display::Flex
        && let Some(p) = cursor
    {
        let p = p / (window.scale_factor() * ui_scale.0);
        let rows: usize = tooltip
            .0
            .0
            .lines()
            .map(|line| line.chars().count().div_ceil(48).max(1))
            .sum();
        let height = rows as f32 * 18. + 30.;
        tooltip.1.left = px((p.x + 16.).min(window.width() / ui_scale.0 - 440.).max(8.));
        tooltip.1.top = px(if p.y + height + 24. < window.height() / ui_scale.0 {
            p.y + 20.
        } else {
            (p.y - height - 12.).max(8.)
        });
    }
    if let Some((_, b, start)) = drag.held
        && let Some(p) = cursor
        && p.distance(start) > 6.
    {
        let p = p / (window.scale_factor() * ui_scale.0);
        ghost.0.image = icons.0[icon(c, b)].clone();
        ghost.1.display = Display::Flex;
        ghost.1.left = px(p.x + 12.);
        ghost.1.top = px(p.y + 12.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slot_hit_areas_use_physical_cursor_at_ui_and_display_scales() {
        for (dpi, ui_scale) in [(1., 0.75), (1., 1.15), (2., 0.8), (2., 1.)] {
            let factor = dpi * ui_scale;
            let node = ComputedNode {
                size: Vec2::splat(58. * factor),
                inverse_scale_factor: 1. / factor,
                ..default()
            };
            let center = Vec2::new(680., 260.) * factor;
            let transform = UiGlobalTransform::from_translation(center);
            assert!(contains(&node, &transform, center));
            assert!(contains(
                &node,
                &transform,
                center + Vec2::splat(28. * factor)
            ));
            assert!(!contains(
                &node,
                &transform,
                center + Vec2::new(31. * factor, 0.)
            ));
            assert!(!contains(
                &node,
                &transform,
                center + Vec2::new(0., -31. * factor)
            ));
        }
    }
    fn hero() -> Character {
        Character {
            id: 1,
            slot: 0,
            nickname: "Test".into(),
            race: 1,
            class: 1,
            gender: 1,
            stats: mortown_protocol::starting_stats(1, 1).unwrap(),
            inventory: mortown_protocol::items::starter(1),
            loot: Default::default(),
        }
    }
    #[test]
    fn clearing_default_slots_and_missing_items_preserves_saved_binding() {
        let mut c = hero();
        assert_eq!(binding(&c, Slot::Bar(0)), 0);
        c.stats.insert("hotbar_0".into(), 0.);
        assert_eq!(binding(&c, Slot::Bar(0)), 0);
        c.stats.insert("hotbar_11".into(), 108.);
        assert_eq!(binding(&c, Slot::Bar(11)), 0);
        assert!(!available(&c, 108));
        c.inventory.push(mortown_protocol::items::Item {
            kind: 8,
            equipped: false,
        });
        c.loot.insert(8, 2);
        assert!(available(&c, 108));
        assert!(!available(&c, 2));
        c.stats.insert("secondary_learned".into(), 1.);
        assert!(available(&c, 2));
        assert_eq!(binding(&hero(), Slot::Bar(11)), 0);
    }
}
