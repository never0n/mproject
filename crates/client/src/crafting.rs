use crate::{
    actionbar::{self, Icons, ItemHint},
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::{
    ClientMessage, crafting,
    professions::{self, Profession},
};

#[derive(Resource, Default)]
pub struct Open(pub bool);
#[derive(Resource, Default)]
pub struct Selected(Option<u8>);
#[derive(Resource, Default)]
pub struct Page(u8);
#[derive(Resource, Default)]
pub struct Mentor(Option<Profession>);
#[derive(Component)]
pub struct ProfessionCard(Profession);
#[derive(Clone, Copy)]
pub struct Proposal {
    owner: i64,
    profession: Profession,
    revision: u32,
    cost: u32,
}
#[derive(Resource, Default)]
pub struct Confirmation(Option<Proposal>);
#[derive(Component)]
pub struct Choice(u8);
#[derive(Component)]
pub struct Tab(u8);
#[derive(Component, Clone, Copy)]
pub enum Action {
    Open,
    Back,
    Select(u8),
    Create(u8),
    Page(u8),
    Specialize(Profession),
    Confirm,
    Cancel,
}
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct PagePanel(u8);
#[derive(Component)]
pub struct RecipeRow(u8);
#[derive(Component)]
pub struct Ingredient {
    recipe: u8,
    kind: u8,
}
#[derive(Component)]
pub struct RecipeMastery(u8);
#[derive(Component)]
pub struct Status;
#[derive(Component)]
pub struct Empty;
#[derive(Component)]
pub struct Reason(u8);
#[derive(Component)]
pub enum Caption {
    Heading,
    Mastery(Profession),
    Specialization,
    Confirm,
}
#[derive(Component)]
pub struct MasteryFill(Profession);

fn item(p: &mut ChildSpawnerCommands, icons: &Icons, kind: u8, ingredient: Option<u8>) {
    p.spawn(Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(6),
        ..default()
    })
    .with_children(|p| {
        p.spawn((
            ItemHint(kind),
            ImageNode::new(icons.0[actionbar::item_icon(kind)].clone()),
            Node {
                width: px(60),
                height: px(60),
                ..default()
            },
        ));
        if let Some(recipe) = ingredient {
            p.spawn((
                Ingredient { recipe, kind },
                actionbar::label("", &icons.1, 13.),
            ));
        }
    });
}
fn button(p: &mut ChildSpawnerCommands, icons: &Icons, action: Action, title: &str) {
    p.spawn((
        action,
        Button,
        Node {
            padding: UiRect::all(px(10)),
            border: UiRect::all(px(1)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(EDGE),
    ))
    .with_children(|p| {
        p.spawn(actionbar::label(title, &icons.1, 13.));
    });
}
pub fn setup(mut commands: Commands, icons: Res<Icons>) {
    commands.init_resource::<Open>();
    commands.init_resource::<Selected>();
    commands.init_resource::<Page>();
    commands.init_resource::<Confirmation>();
    commands.init_resource::<Mentor>();
    commands.spawn((Overlay,
        Node { display: Display::None, position_type: PositionType::Absolute, width: percent(100), height: percent(100),
            padding: UiRect::bottom(px(70)), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() },
        BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.16)), GlobalZIndex(12),
    )).with_children(|root| {
        root.spawn((Node { width: px(720), max_width: percent(96), padding: UiRect::all(px(22)), border: UiRect::all(px(1)),
            flex_direction: FlexDirection::Column, row_gap: px(12), ..default() }, BackgroundColor(PANEL), BorderColor::all(GOLD),
        )).with_children(|p| {
            p.spawn((Caption::Heading, actionbar::label("Ремесло", &icons.1, 22.)));
            p.spawn(Node { column_gap: px(10), ..default() }).with_children(|tabs| {
                for (id, title) in [(0, "Рецепты"), (1, "Профессии")] {
                    tabs.spawn((Button, Action::Page(id), Tab(id), crate::skin::OwnStyle,
                        Node { padding: UiRect::axes(px(16), px(8)), border: UiRect::all(px(1)), ..default() },
                        BackgroundColor(BUTTON), BorderColor::all(EDGE),
                    )).with_children(|b| { b.spawn(actionbar::label(title, &icons.1, 14.)); });
                }
            });
            p.spawn((PagePanel(0), Node { flex_direction: FlexDirection::Column, row_gap: px(12), ..default() })).with_children(|p| {
                p.spawn((Empty, actionbar::label("У вас пока нет рецептов этого ремесла. Их можно купить у торговцев на площади и в торговых рядах.", &icons.1, 14.)));
                p.spawn(Node { column_gap: px(10), ..default() }).with_children(|p| {
                    for recipe in crafting::RECIPES {
                        p.spawn((Button, Action::Select(recipe.item), Choice(recipe.item), ItemHint(recipe.item), crate::skin::OwnStyle,
                            Node { display: Display::None, padding: UiRect::all(px(6)), border: UiRect::all(px(1)), ..default() },
                            BackgroundColor(BUTTON), BorderColor::all(EDGE),
                        )).with_children(|b| { b.spawn((ImageNode::new(icons.0[actionbar::item_icon(recipe.item)].clone()),
                            Node { width: px(42), height: px(42), ..default() })); });
                    }
                });
                for recipe in crafting::RECIPES {
                    p.spawn((RecipeRow(recipe.item), Node { flex_direction: FlexDirection::Column, row_gap: px(12), ..default() })).with_children(|p| {
                        p.spawn((RecipeMastery(recipe.item), actionbar::label("", &icons.1, 13.)));
                        p.spawn(Node { column_gap: px(30), ..default() }).with_children(|p| {
                            for (heading, kinds) in [
                                ("РЕЦЕПТ", vec![(recipe.item, None)]),
                                ("МАТЕРИАЛЫ", recipe.ingredients.iter().map(|(k,_)| (*k,Some(recipe.item))).collect()),
                                ("РЕЗУЛЬТАТ", vec![(recipe.result,None)]),
                            ] {
                                p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() }).with_children(|p| {
                                    p.spawn(actionbar::label(heading, &icons.1, 12.));
                                    p.spawn(Node { column_gap: px(12), ..default() }).with_children(|p| {
                                        for (kind, ingredient) in kinds { item(p, &icons, kind, ingredient); }
                                    });
                                });
                            }
                        });
                        p.spawn((Reason(recipe.item), actionbar::label("", &icons.1, 13.)));
                        button(p, &icons, Action::Create(recipe.item), "Создать одну вещь");
                    });
                }
                p.spawn(actionbar::label("Рецепт сохраняется. Показан расход с учётом вашего мастерства и специализации.", &icons.1, 12.));
            });
            p.spawn((PagePanel(1), Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(10), ..default() })).with_children(|p| {
                p.spawn((Caption::Specialization, actionbar::label("", &icons.1, 13.)));
                for profession in professions::ALL {
                    p.spawn((ProfessionCard(profession), Node { padding: UiRect::all(px(10)), column_gap: px(12), align_items: AlignItems::Center, ..default() }, BackgroundColor(BUTTON))).with_children(|card| {
                        card.spawn(Node { flex_grow: 1., flex_basis: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|p| {
                            p.spawn((Caption::Mastery(profession), actionbar::label("", &icons.1, 13.)));
                            p.spawn((Node { width: percent(100), height: px(5), ..default() }, BackgroundColor(PANEL))).with_children(|bar| {
                                bar.spawn((MasteryFill(profession), Node { width: percent(0), height: percent(100), ..default() }, BackgroundColor(GOLD)));
                            });
                        });
                        button(card, &icons, Action::Specialize(profession), "Специализироваться");
                    });
                }
                p.spawn(actionbar::label("Все три ремесла развиваются независимо до 5 уровня: +25 опыта за изделие.\nУр. 2 — выбор специализации; ур. 3 и 5 — на 1 меньше вспомогательного материала.\nСпециализация экономит 1 основной материал, с ур. 4 — уже 2. Минимальный расход — 1.", &icons.1, 12.));
            });
            p.spawn((PagePanel(2), Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(16), ..default() })).with_children(|p| {
                p.spawn((Caption::Confirm, actionbar::label("", &icons.1, 15.)));
                p.spawn(Node { column_gap: px(12), ..default() }).with_children(|p| {
                    button(p, &icons, Action::Confirm, "Подтвердить выбор");
                    button(p, &icons, Action::Cancel, "Назад к профессиям");
                });
            });
            p.spawn((Status, actionbar::label("", &icons.1, 13.)));
            button(p, &icons, Action::Back, "← В инвентарь");
        });
    });
}
#[allow(clippy::too_many_arguments)]
pub fn input(
    mut open: ResMut<Open>,
    mut selected: ResMut<Selected>,
    mut page: ResMut<Page>,
    mut confirmation: ResMut<Confirmation>,
    mut state: ResMut<CharacterWindow>,
    mut network: ResMut<Network>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
    mut mentor: ResMut<Mentor>,
    mut services: MessageReader<crate::npcs::ServiceRequest>,
    world: Res<crate::Terrain>,
) {
    if let Some(request) = services.read().copied().find(|r| {
        matches!(
            r.service,
            crate::npcs::Service::Recipes | crate::npcs::Service::Professions
        ) && r.valid(&network, &world.0)
    }) {
        let npc = crate::npcs::resolve(&network, request.npc).unwrap();
        mentor.0 = crate::npcs::profession(&npc);
        *state = CharacterWindow {
            open: true,
            inventory: true,
            ..default()
        };
        open.0 = true;
        page.0 = u8::from(request.service == crate::npcs::Service::Professions);
        selected.0 = None;
        confirmation.0 = None;
    }
    if !state.open || !state.inventory || network.mode != Mode::Online {
        open.0 = false;
        confirmation.0 = None;
        page.0 = 0;
        mentor.0 = None;
        return;
    }
    if confirmation.0.is_some_and(|p| {
        network
            .character
            .as_ref()
            .is_none_or(|c| c.id != p.owner || professions::revision(&c.stats) != p.revision)
    }) {
        confirmation.0 = None;
    }
    for (action, interaction) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::Open => {
                open.0 = true;
                mentor.0 = None;
            }
            Action::Back => {
                open.0 = false;
                confirmation.0 = None;
                mentor.0 = None;
            }
            Action::Page(id) if open.0 => {
                page.0 = *id;
                confirmation.0 = None;
            }
            Action::Cancel if open.0 => confirmation.0 = None,
            Action::Select(item)
                if open.0
                    && crafting::definition(*item)
                        .is_some_and(|r| mentor.0.is_none_or(|p| p == r.profession))
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| c.loot.get(item).copied().unwrap_or(0) > 0) =>
            {
                selected.0 = Some(*item)
            }
            Action::Create(recipe) if open.0 && !network.pending && page.0 == 0 => {
                if let Some(c) = network.character.as_ref() {
                    let expected = crafting::count(c);
                    let specialization_revision = professions::revision(&c.stats);
                    network.request(ClientMessage::Craft {
                        recipe: *recipe,
                        expected,
                        specialization_revision,
                    });
                }
            }
            Action::Specialize(profession)
                if open.0
                    && page.0 == 1
                    && !network.pending
                    && mentor.0.is_none_or(|p| p == *profession) =>
            {
                if let Some(c) = network.character.as_ref()
                    && professions::eligible(&c.stats, *profession).is_ok()
                {
                    confirmation.0 = Some(Proposal {
                        owner: c.id,
                        profession: *profession,
                        revision: professions::revision(&c.stats),
                        cost: professions::selection_cost(&c.stats),
                    });
                }
            }
            Action::Confirm if open.0 && !network.pending => {
                if let Some(proposal) = confirmation.0 {
                    network.request(ClientMessage::Specialize {
                        profession: proposal.profession,
                        expected: proposal.revision,
                    });
                    confirmation.0 = None;
                }
            }
            _ => {}
        }
    }
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn display(
    mut open: ResMut<Open>,
    mut selected: ResMut<Selected>,
    page: Res<Page>,
    confirmation: Res<Confirmation>,
    mentor: Res<Mentor>,
    state: Res<CharacterWindow>,
    network: Res<Network>,
    mut nodes: Query<
        (
            &mut Node,
            Option<&Overlay>,
            Option<&PagePanel>,
            Option<&RecipeRow>,
            Option<&Action>,
            Option<&Empty>,
            Option<&MasteryFill>,
            Option<&ProfessionCard>,
        ),
        Or<(
            With<Overlay>,
            With<PagePanel>,
            With<RecipeRow>,
            With<Action>,
            With<Empty>,
            With<MasteryFill>,
            With<ProfessionCard>,
        )>,
    >,
    mut texts: Query<
        (
            &mut Text,
            Option<&Ingredient>,
            Option<&Reason>,
            Option<&Status>,
            Option<&RecipeMastery>,
            Option<&Caption>,
            Option<&mut TextColor>,
        ),
        Or<(
            With<Ingredient>,
            With<Reason>,
            With<Status>,
            With<RecipeMastery>,
            With<Caption>,
        )>,
    >,
    mut borders: Query<
        (
            &mut BorderColor,
            &Interaction,
            Option<&Choice>,
            Option<&Tab>,
        ),
        Or<(With<Choice>, With<Tab>)>,
    >,
) {
    if !state.open || !state.inventory || network.mode != Mode::Online {
        open.0 = false;
    }
    let Some(c) = network.character.as_ref() else {
        for (mut node, overlay, ..) in &mut nodes {
            if overlay.is_some() {
                node.display = Display::None;
            }
        }
        return;
    };
    let has_recipe = |item: u8| {
        c.loot.get(&item).copied().unwrap_or(0) > 0
            && crafting::definition(item)
                .is_some_and(|r| mentor.0.is_none_or(|p| p == r.profession))
    };
    if !selected.0.is_some_and(has_recipe) {
        selected.0 = crafting::RECIPES
            .iter()
            .find(|r| has_recipe(r.item))
            .map(|r| r.item);
    }
    let active = professions::active(&c.stats);
    let alive = network.authoritative.as_ref().is_some_and(|a| a.hp > 0.);
    let coins = network
        .authoritative
        .as_ref()
        .map_or(0., |a| a.progress.coins);
    let reason = |id| -> Option<&str> {
        if network.pending {
            Some("Сохраняем…")
        } else if !alive {
            Some("Сначала возродитесь")
        } else {
            crafting::definition(id)
                .unwrap()
                .validate_with(&c.loot, &c.stats)
                .err()
        }
    };
    let visible_page = if confirmation.0.is_some() { 2 } else { page.0 };
    for (mut node, overlay, panel, row, action, empty, fill, profession_card) in &mut nodes {
        let show = if overlay.is_some() {
            Some(open.0)
        } else if let Some(panel) = panel {
            Some(panel.0 == visible_page)
        } else if let Some(card) = profession_card {
            Some(mentor.0.is_none_or(|p| p == card.0))
        } else if let Some(row) = row {
            Some(selected.0 == Some(row.0))
        } else if empty.is_some() {
            Some(selected.0.is_none())
        } else if let Some(action) = action {
            match action {
                Action::Select(item) => Some(has_recipe(*item)),
                Action::Create(item) => Some(reason(*item).is_none()),
                Action::Specialize(profession) => Some(
                    alive
                        && !network.pending
                        && professions::eligible(&c.stats, *profession).is_ok(),
                ),
                Action::Confirm => Some(
                    alive
                        && !network.pending
                        && confirmation.0.is_some_and(|p| coins >= f64::from(p.cost)),
                ),
                _ => None,
            }
        } else {
            None
        };
        if let Some(show) = show {
            node.display = if show { Display::Flex } else { Display::None };
        }
        if let Some(fill) = fill {
            let m = fill.0.mastery(&c.stats);
            node.width = percent(if m.needed == 0 {
                100.
            } else {
                m.within_level as f32 / m.needed as f32 * 100.
            });
        }
    }
    for (mut border, interaction, choice, tab) in &mut borders {
        let selected = choice.is_some_and(|choice| selected.0 == Some(choice.0))
            || tab.is_some_and(|tab| tab.0 == page.0);
        *border = BorderColor::all(if selected || *interaction != Interaction::None {
            GOLD
        } else {
            EDGE
        });
    }
    for (mut text, ingredient, why, status, recipe_mastery, caption, color) in &mut texts {
        if let Some(ingredient) = ingredient {
            let required = crafting::definition(ingredient.recipe)
                .unwrap()
                .requirements(&c.stats)
                .into_iter()
                .find(|(kind, _)| *kind == ingredient.kind)
                .unwrap()
                .1;
            let have = c.loot.get(&ingredient.kind).copied().unwrap_or(0);
            text.0 = format!("{have} / {required}");
            if let Some(mut color) = color {
                color.0 = if have >= required {
                    Color::srgb(0.60, 0.82, 0.56)
                } else {
                    Color::srgb(0.95, 0.60, 0.46)
                };
            }
        } else if let Some(why) = why {
            text.0 = reason(why.0).unwrap_or("Все материалы собраны").into();
        } else if status.is_some() {
            text.0.clone_from(&network.status);
        } else if let Some(recipe) = recipe_mastery {
            let profession = crafting::definition(recipe.0).unwrap().profession;
            let m = profession.mastery(&c.stats);
            text.0 = format!(
                "{}{}{}",
                profession.summary(&c.stats),
                if active == Some(profession) {
                    " · специализация"
                } else {
                    ""
                },
                if m.level < professions::MAX_LEVEL {
                    format!(
                        "\nЗа изготовление: +{} опыта ремесла",
                        professions::CRAFT_XP
                    )
                } else {
                    String::new()
                }
            );
        } else if let Some(caption) = caption {
            text.0 = match caption {
                Caption::Heading => mentor.0.map_or("Ремесло", |p| p.name()).into(),
                Caption::Specialization => format!("Специализация: {} · создано вещей: {}\nПервая — бесплатно. Смена — {} монет; опыт всех ремёсел сохраняется.", active.map_or("не выбрана", |p| p.name()), crafting::count(c), professions::SWITCH_COST),
                Caption::Mastery(profession) => format!("{}\n{}", profession.summary(&c.stats), if active == Some(*profession) { "Основная специализация" } else if profession.mastery(&c.stats).level < 2 { "Специализация доступна со 2 уровня" } else { "Можно выбрать основной специализацией" }),
                Caption::Confirm => confirmation.0.map_or_else(String::new, |p| format!("Выбрать «{}»?\n\nСтоимость: {}\nОпыт всех ремёсел и рецепты сохранятся. Экономия основного материала будет действовать только для выбранной специализации.{}", p.profession.name(),
                    if p.cost == 0 { "бесплатно".into() } else { format!("{} монет", p.cost) },
                    if coins < f64::from(p.cost) { "\n\nНедостаточно монет." } else { "" })),
            };
        }
    }
}
