use crate::{
    actionbar::{self, Icons, ItemHint},
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::{ClientMessage, crafting};

#[derive(Resource, Default)]
pub struct Open(pub bool);
#[derive(Resource, Default)]
pub struct Selected(Option<u8>);
#[derive(Component)]
pub struct Choice(u8);
#[derive(Component, Clone, Copy)]
pub enum Action {
    Open,
    Back,
    Select(u8),
    Create(u8),
    Cook,
    Stall,
    Timber,
    PlaceTimber,
    Cart,
    Haul,
    Foundry,
}
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct RecipeRow(u8);
#[derive(Component)]
pub struct Ingredient {
    recipe: u8,
    kind: u8,
}
#[derive(Component)]
pub struct Status;
#[derive(Component)]
pub struct Empty;
#[derive(Component)]
pub struct Reason(u8);

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
    commands.spawn((Overlay,
        Node { display: Display::None, position_type: PositionType::Absolute, width: percent(100), height: percent(100),
            padding: UiRect::bottom(px(70)), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() },
        BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.16)), GlobalZIndex(12),
    )).with_children(|root| {
        root.spawn((Node { width: px(720), max_width: percent(96), padding: UiRect::all(px(22)), border: UiRect::all(px(1)),
            flex_direction: FlexDirection::Column, row_gap: px(12), ..default() }, BackgroundColor(PANEL), BorderColor::all(GOLD),
        )).with_children(|p| {
            p.spawn(actionbar::label("Ремесло", &icons.1, 22.));
            p.spawn((Empty, actionbar::label("У вас пока нет рецептов. Рецепты — физические предметы: их можно получить от других игроков.", &icons.1, 14.)));
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
            p.spawn(actionbar::label("Клинок рисуется в литье у горна. Здесь остаются готовка, прилавок, брус и телега.", &icons.1, 12.));
            p.spawn(Node { column_gap: px(8), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|p| {
                button(p, &icons, Action::Cook, "Испечь хлеб");
                button(p, &icons, Action::Stall, "Сколотить прилавок");
                button(p, &icons, Action::Timber, "Собрать брус");
                button(p, &icons, Action::PlaceTimber, "Поставить брус");
                button(p, &icons, Action::Cart, "Сколотить телегу");
                button(p, &icons, Action::Haul, "Тянуть телегу");
                button(p, &icons, Action::Foundry, "Литьё по чертежу");
            });
            p.spawn(actionbar::label("Рецепт сохраняется. Создать вещь может любой, у кого есть рецепт и материалы. Золотая руда не становится монетой.", &icons.1, 12.));
            p.spawn((Status, actionbar::label("", &icons.1, 13.)));
            button(p, &icons, Action::Back, "← В инвентарь");
        });
    });
}
pub fn input(
    mut open: ResMut<Open>,
    mut selected: ResMut<Selected>,
    state: Res<CharacterWindow>,
    mut network: ResMut<Network>,
    mut foundry: ResMut<crate::foundry::Board>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
) {
    if !state.open || !state.inventory || network.mode != Mode::Online {
        open.0 = false;
        return;
    }
    for (action, interaction) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::Open => open.0 = true,
            Action::Back => open.0 = false,
            Action::Select(item)
                if open.0
                    && crafting::definition(*item).is_some()
                    && network
                        .character
                        .as_ref()
                        .is_some_and(|c| crafting::in_bag(c, *item) > 0) =>
            {
                selected.0 = Some(*item)
            }
            Action::Cook if open.0 && !network.object_pending => {
                network.work_request(mortown_protocol::work::Command::Cook);
            }
            Action::Stall if open.0 && !network.object_pending => {
                network.work_request(mortown_protocol::work::Command::MakeStall);
            }
            Action::Timber if open.0 && !network.object_pending => {
                network.work_request(mortown_protocol::work::Command::MakeTimber);
            }
            Action::PlaceTimber if open.0 && !network.object_pending => {
                network.work_request(mortown_protocol::work::Command::PlaceTimber);
            }
            Action::Cart if open.0 && !network.object_pending => {
                network.work_request(mortown_protocol::work::Command::MakeCart);
            }
            Action::Haul if open.0 && !network.object_pending => {
                network.work_request(mortown_protocol::work::Command::Haul);
            }
            Action::Foundry if open.0 => foundry.open = true,
            Action::Create(recipe) if open.0 && !network.pending => {
                if let Some(c) = network.character.as_ref() {
                    let expected = crafting::count(c);
                    network.request(ClientMessage::Craft {
                        recipe: *recipe,
                        expected,
                    });
                }
            }
            _ => {}
        }
    }
}
#[allow(clippy::type_complexity)]
pub fn display(
    mut open: ResMut<Open>,
    mut selected: ResMut<Selected>,
    state: Res<CharacterWindow>,
    network: Res<Network>,
    mut nodes: Query<
        (
            &mut Node,
            Option<&Overlay>,
            Option<&RecipeRow>,
            Option<&Action>,
            Option<&Empty>,
        ),
        Or<(With<Overlay>, With<RecipeRow>, With<Action>, With<Empty>)>,
    >,
    mut texts: Query<
        (
            &mut Text,
            Option<&Ingredient>,
            Option<&Reason>,
            Option<&mut TextColor>,
        ),
        Or<(With<Ingredient>, With<Reason>, With<Status>)>,
    >,
    mut borders: Query<(&mut BorderColor, &Interaction, &Choice)>,
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
    let has_recipe =
        |item: u8| crafting::in_bag(c, item) > 0 && crafting::definition(item).is_some();
    if !selected.0.is_some_and(has_recipe) {
        selected.0 = crafting::RECIPES
            .iter()
            .find(|r| has_recipe(r.item))
            .map(|r| r.item);
    }
    let alive = network.authoritative.as_ref().is_some_and(|a| a.hp > 0.);
    let reason = |id| -> Option<&str> {
        if network.pending {
            Some("Сохраняем…")
        } else if !alive {
            Some("Сначала возродитесь")
        } else {
            crafting::definition(id).unwrap().validate(c).err()
        }
    };
    for (mut node, overlay, row, action, empty) in &mut nodes {
        let show = if overlay.is_some() {
            Some(open.0)
        } else if let Some(row) = row {
            Some(selected.0 == Some(row.0))
        } else if empty.is_some() {
            Some(selected.0.is_none())
        } else if let Some(action) = action {
            match action {
                Action::Select(item) => Some(has_recipe(*item)),
                Action::Create(item) => Some(reason(*item).is_none()),
                _ => None,
            }
        } else {
            None
        };
        if let Some(show) = show {
            node.display = if show { Display::Flex } else { Display::None };
        }
    }
    for (mut border, interaction, choice) in &mut borders {
        let selected = selected.0 == Some(choice.0);
        *border = BorderColor::all(if selected || *interaction != Interaction::None {
            GOLD
        } else {
            EDGE
        });
    }
    for (mut text, ingredient, why, color) in &mut texts {
        if let Some(ingredient) = ingredient {
            let required = crafting::definition(ingredient.recipe)
                .unwrap()
                .ingredients
                .iter()
                .find(|(kind, _)| *kind == ingredient.kind)
                .unwrap()
                .1;
            let have = crafting::in_bag(c, ingredient.kind);
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
        } else {
            text.0.clone_from(&network.status);
        }
    }
}
