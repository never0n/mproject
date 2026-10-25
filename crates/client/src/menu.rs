//! In-game main menu: the ⚙ button (bottom right) and its popup with window shortcuts,
//! auto-attack, respawn, hero selection, the controls guide and "Exit".
//! Most entries only carry another module's marker component (e.g. `inventory::OpenInventory`);
//! that module reacts to the same click. `input` handles only Help, Roster and Exit itself.
use crate::{
    Movement,
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, Hud, INK, PANEL},
};
use bevy::prelude::*;
/// Controls guide is showing (the popup then hides most entries).
#[derive(Resource, Default)]
pub struct Help(pub bool);
/// The ⚙ button; a `Hud` panel that is excluded from dragging in `movable::setup`.
#[derive(Component)]
pub struct Gear;
/// The menu popup panel.
#[derive(Component)]
pub struct Popup;
/// Controls cheat-sheet text inside the popup.
#[derive(Component)]
pub struct Guide;
/// Menu button identity; visibility rules live in `display`.
#[derive(Component, Clone, Copy)]
pub enum Entry {
    Character,
    Inventory,
    Skills,
    Craft,
    Attack,
    Respawn,
    Roster,
    Help,
    Alliance,
    Exit,
}

pub fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    commands.init_resource::<Help>();
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    let label = |title: &str, size: f32| {
        (
            Text::new(title),
            TextFont {
                font: font.clone(),
                font_size: size,
                ..default()
            },
            TextColor(GOLD),
        )
    };
    commands
        .spawn((
            Hud,
            Gear,
            Button,
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                bottom: px(16),
                width: px(48),
                height: px(48),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
            // Above window overlays (10) and the lobby (20).
            GlobalZIndex(40),
        ))
        .with_children(|b| {
            b.spawn(label("⚙", 26.));
        });
    // Entry → marker: the marked module opens its window on the same press.
    commands.spawn((Popup,crate::ui::WorldInputBlocker,Node{display:Display::None,position_type:PositionType::Absolute,right:px(16),bottom:px(84),width:px(304),max_width:percent(94),padding:UiRect::all(px(16)),border:UiRect::all(px(1)),flex_direction:FlexDirection::Column,row_gap:px(10),..default()},BackgroundColor(PANEL),BorderColor::all(GOLD),GlobalZIndex(40))).with_children(|p|{
        p.spawn(label("Мортаун",18.));
        p.spawn((crate::ui::Readout::Players,Text::new(""),TextFont{font:font.clone(),font_size:12.,..default()},TextColor(INK)));
        p.spawn(Node{display:Display::Grid,grid_template_columns:RepeatedGridTrack::flex(2,1.),column_gap:px(8),row_gap:px(8),..default()}).with_children(|grid|{
            for(entry,title) in [(Entry::Character,"Персонаж"),(Entry::Inventory,"Инвентарь"),(Entry::Skills,"Навыки"),(Entry::Alliance,"Альянс"),(Entry::Craft,"Ремесло"),(Entry::Attack,"Автоатака"),(Entry::Respawn,"Подняться"),(Entry::Roster,"Выбор героя"),(Entry::Help,"Управление"),(Entry::Exit,"Выйти из игры")] {
                let mut b=grid.spawn((Button,entry,Node{padding:UiRect::all(px(9)),border:UiRect::all(px(1)),..default()},BackgroundColor(BUTTON),BorderColor::all(EDGE)));
                match entry {
                    Entry::Character=>{b.insert(crate::ui::ToggleCharacter);}
                    Entry::Inventory=>{b.insert(crate::inventory::OpenInventory);}
                    Entry::Alliance=>{b.insert(crate::alliance::Action::Open);}
                    Entry::Skills=>{b.insert(crate::skills::Action::Open);}
                    Entry::Craft=>{b.insert((crate::inventory::OpenInventory,crate::crafting::Action::Open));}
                    Entry::Attack=>{b.insert(crate::combat::Action::Attack);}
                    Entry::Respawn=>{b.insert(crate::combat::Action::Respawn);}
                    _=>{}
                }
                b.with_children(|b|{b.spawn(label(title,13.));});
            }
        });
        // Keep this list in sync with the real hotkeys in each module's `input`.
        p.spawn((Guide,Node{display:Display::None,..default()},Text::new("ОКНА\nC — персонаж · I — инвентарь\nK — навыки · L — альянс\nShift+F10 — выбор героя · Esc — закрыть\n\nДВИЖЕНИЕ И КАМЕРА\nWASD — бег · Shift — шаг\nЛКМ — маршрут · ПКМ — камера\nКолесо — масштаб · F — обзор города\nHome — сброс камеры\n\nБОЙ\nTab / ЛКМ — цель · Space — автоатака\n1 / 2 — первые навыки · H — зелье\nG — к лагерю · R — подняться\nF1–F12 — закреплённые действия\n\nПАНЕЛЬ И ВЕЩИ\nПеретащите навык или предмет на F1–F12.\nПКМ очищает слот панели.\nЭкипировка — перенос вещи на героя.\nE — поднять предмет с земли.\nРемесло — рецепты в инвентаре.\n\nЧАТ\nEnter — написать / отправить · Esc — вернуться в игру\nОбщий — весь мир · Рядом — 60 метров\nНажмите сообщение для игнора или жалобы.\n/unignore Имя — убрать из игнора."),TextFont{font:font.clone(),font_size:12.,..default()},TextColor(INK)));
    });
}

/// Runs before the other window `input` systems in the Update chain.
pub fn input(
    mut state: ResMut<CharacterWindow>,
    mut shutdown: ResMut<crate::shutdown::Requested>,
    mut help: ResMut<Help>,
    mut network: ResMut<Network>,
    mut movement: ResMut<Movement>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    gears: Query<&Interaction, (With<Gear>, Changed<Interaction>)>,
    entries: Query<(&Entry, &Interaction), Changed<Interaction>>,
) {
    if !network.playing() {
        state.menu = false;
        return;
    }
    if gears.iter().any(|i| *i == Interaction::Pressed) {
        let open = !state.menu;
        // Opening the menu closes every other window and stops walking.
        *state = CharacterWindow {
            open,
            menu: open,
            ..default()
        };
        *movement = Movement::default();
        help.0 = false;
    }
    if !state.menu {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.open = false;
        state.menu = false;
        keys.clear_just_pressed(KeyCode::Escape);
        return;
    }
    // A window hotkey closes the menu; that window's own system then opens it.
    if [KeyCode::KeyC, KeyCode::KeyI, KeyCode::KeyK, KeyCode::KeyM]
        .iter()
        .any(|k| keys.just_pressed(*k))
    {
        state.open = false;
        state.menu = false;
        return;
    }
    for (entry, interaction) in &entries {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if matches!(entry, Entry::Help) {
            help.0 = !help.0;
            continue;
        }
        state.menu = false;
        state.open = false;
        match entry {
            // Exit is graceful: `shutdown::update` waits for the server to save first.
            Entry::Exit => {
                shutdown.0 = true;
                network.status = "Сохраняем персонажа и выходим…".into();
            }
            // Back to hero selection; same as Shift+F10 in `lobby::buttons`.
            Entry::Roster => {
                if network.mode == Mode::Online {
                    network.leave_world();
                } else {
                    network.disconnect();
                }
            }
            _ => {} // Existing action systems consume the same button press.
        }
    }
}

pub fn display(
    state: Res<CharacterWindow>,
    help: Res<Help>,
    network: Res<Network>,
    mut popup: Single<&mut Node, (With<Popup>, Without<Guide>, Without<Entry>)>,
    mut guide: Single<&mut Node, (With<Guide>, Without<Popup>, Without<Entry>)>,
    mut entries: Query<(&Entry, &mut Node), (Without<Popup>, Without<Guide>)>,
) {
    popup.display = if state.menu && network.playing() {
        Display::Flex
    } else {
        Display::None
    };
    guide.display = if help.0 { Display::Flex } else { Display::None };
    for (entry, mut node) in &mut entries {
        let online = network.mode == Mode::Online;
        let alive = network.authoritative.as_ref().is_some_and(|a| a.hp > 0.);
        // Guide mode keeps only Help/Exit; offline view hides server-only entries.
        let show = (!help.0 || matches!(entry, Entry::Help | Entry::Exit))
            && match entry {
                Entry::Respawn => online && !alive,
                Entry::Attack => online && alive,
                Entry::Character | Entry::Help | Entry::Roster | Entry::Exit => true,
                _ => online,
            };
        node.display = if show { Display::Flex } else { Display::None };
    }
}
