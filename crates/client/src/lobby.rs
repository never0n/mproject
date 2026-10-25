//! Pre-game screen: login/registration, the hero roster, character creation and the
//! rotating 3D model preview. Shown whenever `Network` is not playing.
//! Flow per frame: `buttons` (clicks) → `typing` (keyboard) → `submit` (sends the request)
//! → `display`/`layout`/`style` (texts, visibility, colours). Server answers arrive via
//! `network::receive`, which switches `Network::mode` (Menu → Connecting → Selection → Online).
use crate::{
    Movement, RunOptions,
    network::{Mode, Network},
};
use bevy::{
    input::{ButtonState, keyboard::KeyboardInput},
    prelude::*,
};
use mortown_protocol::{
    CHARACTER_LIMIT, ClientMessage, PROTOCOL_VERSION, WORLD_VERSION, starting_stats,
    validate_character, validate_credentials,
};

/// Lobby form state (client-only; nothing here is sent until `submit`).
#[derive(Resource)]
pub struct Form {
    /// Text fields: 0 login, 1 password, 2 new hero name.
    values: [String; 3],
    /// Index into `values` that receives typed text.
    focus: usize,
    register: bool,
    creating: bool,
    /// "Меню ▾" dropdown on the roster (new hero / log out).
    menu_open: bool,
    /// Creation choice, 1-based like the protocol id. Every hero is a human without class.
    gender: u8,
    /// Set by a click or Enter; consumed by `submit` later in the same frame.
    submit: bool,
    /// Last seen `Network::selection_revision`; a new roster from the server resets the form.
    revision: u64,
}
impl Default for Form {
    fn default() -> Self {
        Self {
            values: Default::default(),
            focus: 0,
            register: false,
            creating: false,
            menu_open: false,
            gender: 1,
            submit: false,
            revision: 0,
        }
    }
}
impl Form {
    /// What the 3D preview should show: the hero being created, or the selected roster hero
    /// with their equipment. `None` outside hero selection. Read by `appearance`, `ancestry`
    /// and `inventory` (which turns the preview camera on).
    pub fn model_preview(
        &self,
        network: &Network,
    ) -> Option<(
        mortown_protocol::BodyAppearance,
        mortown_protocol::items::EquipmentAppearance,
    )> {
        if network.mode != Mode::Selection {
            return None;
        }
        if self.creating {
            return Some((
                mortown_protocol::BodyAppearance {
                    gender: self.gender,
                },
                default(),
            ));
        }
        network
            .characters
            .iter()
            .find(|c| Some(c.id) == network.selected)
            .map(|c| {
                (
                    c.into(),
                    mortown_protocol::items::EquipmentAppearance::from_items(&c.items),
                )
            })
    }
}
/// Full-screen dimmed root of the lobby.
#[derive(Component)]
pub struct Lobby;
/// Main form panel (left).
#[derive(Component)]
pub struct Panel;
/// "Внешность" side panel with the model image; hidden on narrow windows.
#[derive(Component)]
pub struct ModelPanel;
/// Image that shows the preview camera's render target.
#[derive(Component)]
pub struct ModelImage;
/// ◀/▶ button; rotation step in radians.
#[derive(Component)]
pub struct TurnModel(pub f32);
/// Every lobby button. One enum drives clicks (`buttons`), labels (`display`) and sizes (`layout`).
#[derive(Component, Clone, Copy)]
pub(crate) enum Action {
    /// Text field; index into `Form::values`.
    Field(usize),
    /// Toggle login ↔ registration.
    Mode,
    Gender,
    Submit,
    Menu,
    /// Roster slot, 0-based (the protocol's `Character::slot` is 1-based).
    Slot(usize),
    New,
    Enter,
    Back,
    Logout,
}
/// Text inside an `Action` button.
#[derive(Component)]
pub(crate) struct Label(Action);
/// Status/error line (from `Network::status`).
#[derive(Component)]
pub(crate) struct Feedback;
/// Starting stats summary during creation.
#[derive(Component)]
pub(crate) struct Preview;
#[derive(Component)]
pub(crate) struct Title;
/// Button group; `display` shows only the groups for the current screen.
#[derive(Component, Clone, Copy)]
pub(crate) enum Section {
    Auth,
    Selection,
    Roster,
    Creation,
    Options,
}

pub fn setup(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    options: Res<RunOptions>,
    mut network: ResMut<Network>,
) {
    // `--view`: skip the lobby and walk the world offline.
    if !options.view.is_empty() {
        network.mode = Mode::Offline;
    }
    // Dev shortcut: auto-login from MORTOWN_LOGIN / MORTOWN_PASSWORD (not in UI preview runs).
    if let (Ok(login), Ok(password)) = (
        std::env::var("MORTOWN_LOGIN"),
        std::env::var("MORTOWN_PASSWORD"),
    ) && options.ui_preview.is_none()
        && validate_credentials(&login, &password).is_ok()
    {
        network.begin(ClientMessage::Login {
            version: PROTOCOL_VERSION,
            world: WORLD_VERSION.into(),
            login,
            password,
        });
    }
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    commands
        .spawn((
            Lobby,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                column_gap: px(16),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.025, 0.04, 0.70)),
            // Above window overlays (10), below the ⚙ menu (40).
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((
                Panel,
                Node {
                    width: px(500),
                    max_width: percent(96),
                    border: UiRect::all(px(2)),
                    padding: UiRect::all(px(28)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(16),
                    ..default()
                },
                BackgroundColor(crate::ui::PANEL),
                BorderColor::all(crate::ui::GOLD),
            ))
            .with_children(|panel| {
                // Size 32 → restyled as a heading by `skin::setup`.
                panel.spawn((
                    Text::new("М О Р Т А У Н"),
                    TextFont {
                        font: font.clone(),
                        font_size: 32.,
                        ..default()
                    },
                    TextColor(crate::ui::GOLD),
                ));
                panel.spawn((
                    Title,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 18.,
                        ..default()
                    },
                ));
                // All screens are spawned once; `display` toggles them by mode.
                for (section, actions) in [
                    (
                        Section::Auth,
                        vec![
                            Action::Field(0),
                            Action::Field(1),
                            Action::Submit,
                            Action::Mode,
                        ],
                    ),
                    (
                        Section::Roster,
                        (0..CHARACTER_LIMIT).map(Action::Slot).collect(),
                    ),
                    (Section::Selection, vec![Action::Enter, Action::Menu]),
                    (Section::Options, vec![Action::New, Action::Logout]),
                    (
                        Section::Creation,
                        vec![
                            Action::Field(2),
                            Action::Gender,
                            Action::Submit,
                            Action::Back,
                        ],
                    ),
                ] {
                    panel
                        .spawn((
                            section,
                            Node {
                                width: percent(100),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                row_gap: px(8),
                                column_gap: px(8),
                                ..default()
                            },
                        ))
                        .with_children(|group| {
                            for action in actions {
                                let slot = matches!(action, Action::Slot(_));
                                group
                                    .spawn((
                                        Button,
                                        action,
                                        Node {
                                            width: if slot { percent(48.) } else { percent(100.) },
                                            border: UiRect::all(px(1)),
                                            min_height: px(if slot { 58. } else { 42. }),
                                            padding: UiRect::axes(px(14), px(10)),
                                            border_radius: BorderRadius::all(px(4)),
                                            align_items: AlignItems::Center,
                                            justify_content: if matches!(
                                                action,
                                                Action::Submit | Action::Enter
                                            ) {
                                                JustifyContent::Center
                                            } else {
                                                JustifyContent::Start
                                            },
                                            ..default()
                                        },
                                        BackgroundColor(crate::ui::BUTTON),
                                        BorderColor::all(crate::ui::EDGE),
                                    ))
                                    .with_children(|button| {
                                        button.spawn((
                                            Label(action),
                                            Text::new(""),
                                            TextFont {
                                                font: font.clone(),
                                                font_size: if slot { 13. } else { 16. },
                                                ..default()
                                            },
                                            TextColor(crate::ui::INK),
                                        ));
                                    });
                            }
                        });
                }
                panel.spawn((
                    Preview,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 13.,
                        ..default()
                    },
                    TextColor(crate::ui::INK),
                ));
                panel.spawn((
                    Feedback,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 14.,
                        ..default()
                    },
                    TextColor(Color::srgb(0.93, 0.76, 0.43)),
                ));
            });
            root.spawn((
                ModelPanel,
                Node {
                    display: Display::None,
                    width: px(280),
                    flex_shrink: 0.,
                    padding: UiRect::all(px(14)),
                    border: UiRect::all(px(1)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(10),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(crate::ui::PANEL),
                BorderColor::all(crate::ui::EDGE),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Внешность"),
                    TextFont {
                        font: font.clone(),
                        font_size: 18.,
                        ..default()
                    },
                    TextColor(crate::ui::GOLD),
                ));
                panel.spawn((
                    ModelImage,
                    ImageNode::default(),
                    Node {
                        width: percent(100),
                        // Width/height 3:4, portrait framing.
                        aspect_ratio: Some(0.75),
                        ..default()
                    },
                ));
                panel
                    .spawn(Node {
                        column_gap: px(12),
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        // ±0.45 rad ≈ 26° per click.
                        for (text, angle) in [("◀", -0.45), ("▶", 0.45)] {
                            row.spawn((
                                TurnModel(angle),
                                Button,
                                Node {
                                    width: px(56),
                                    height: px(34),
                                    border: UiRect::all(px(1)),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BackgroundColor(crate::ui::BUTTON),
                                BorderColor::all(crate::ui::EDGE),
                            ))
                            .with_child((
                                Text::new(text),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 16.,
                                    ..default()
                                },
                                TextColor(crate::ui::GOLD),
                            ));
                        }
                    });
            });
        });
}
/// Rotates only the preview model (`Wearer::Preview`), never the in-world hero.
pub fn turn_model(
    buttons: Query<(&TurnModel, &Interaction), Changed<Interaction>>,
    mut roots: Query<(&crate::appearance::Wearer, &mut Transform)>,
) {
    let angle: f32 = buttons
        .iter()
        .filter(|(_, i)| **i == Interaction::Pressed)
        .map(|(turn, _)| turn.0)
        .sum();
    if angle == 0. {
        return;
    }
    for (wearer, mut transform) in &mut roots {
        if matches!(wearer, crate::appearance::Wearer::Preview) {
            transform.rotate_y(angle);
        }
    }
}
/// Lobby clicks plus two global keys: Shift+F10 (back to hero selection while playing)
/// and Escape (close the dropdown / creation / registration). First system in the Update chain.
#[allow(clippy::type_complexity)]
pub fn buttons(
    mut form: ResMut<Form>,
    mut network: ResMut<Network>,
    mut movement: ResMut<Movement>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    // After successful registration: back to login with the password field focused.
    if network.registered {
        form.register = false;
        form.focus = 1;
        network.registered = false;
    }
    if network.selection_revision != form.revision {
        form.revision = network.selection_revision;
        form.creating = false;
        form.menu_open = false;
    }
    if keys.just_pressed(KeyCode::F10)
        && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
        && network.playing()
    {
        if network.mode == Mode::Online {
            network.leave_world();
        } else {
            network.disconnect();
        }
        form.creating = false;
        *movement = Movement::default();
        return;
    }
    if !network.pending
        && keys.just_pressed(KeyCode::Escape)
        && (form.menu_open || form.creating || form.register)
    {
        form.menu_open = false;
        form.creating = false;
        form.register = false;
        keys.clear_just_pressed(KeyCode::Escape);
        return;
    }
    // Ignore clicks while a request is in flight, so nothing is sent twice.
    if network.pending || !matches!(network.mode, Mode::Menu | Mode::Selection) {
        return;
    }
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            Action::Field(i) => form.focus = i,
            Action::Mode => {
                form.register = !form.register;
                form.focus = 0;
                form.values[1].clear();
                network.status.clear();
            }
            Action::Gender => form.gender = form.gender % 2 + 1,
            Action::Submit | Action::Enter => form.submit = true,
            Action::Menu => form.menu_open = !form.menu_open,
            Action::Slot(slot) => {
                if let Some(c) = network
                    .characters
                    .iter()
                    .find(|c| c.slot as usize == slot + 1)
                {
                    network.selected = Some(c.id);
                }
            }
            Action::New => {
                if network.characters.len() < CHARACTER_LIMIT {
                    form.menu_open = false;
                    form.creating = true;
                    form.focus = 2;
                    form.values[2].clear();
                }
            }
            Action::Back => form.creating = false,
            Action::Logout => {
                network.disconnect();
                form.creating = false;
                form.menu_open = false;
                form.register = false;
                form.focus = 0;
                network.status.clear();
            }
        }
    }
}
/// Reads raw `KeyboardInput` (not `ButtonInput`) to get typed characters, including Cyrillic.
pub fn typing(
    mut input: MessageReader<KeyboardInput>,
    mut form: ResMut<Form>,
    network: Res<Network>,
) {
    for event in input.read() {
        if network.pending
            || !matches!(network.mode, Mode::Menu | Mode::Selection)
            || event.state != ButtonState::Pressed
        {
            continue;
        }
        // On the roster, only Enter matters (enter the world with the selected hero).
        if network.mode == Mode::Selection && !form.creating {
            if form.menu_open {
                continue;
            }
            if matches!(event.key_code, KeyCode::Enter | KeyCode::NumpadEnter) {
                form.submit = true;
            }
            continue;
        }
        match event.key_code {
            KeyCode::Tab => {
                form.focus = if network.mode == Mode::Menu {
                    (form.focus + 1) % 2
                } else {
                    2
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter => form.submit = true,
            KeyCode::Backspace => {
                let i = form.focus;
                form.values[i].pop();
            }
            _ => {
                if let Some(text) = &event.text {
                    let i = if network.mode == Mode::Menu {
                        form.focus.min(1)
                    } else {
                        2
                    };
                    // Limits are in UTF-8 bytes (Cyrillic letters take 2): password 128, others 64.
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if form.values[i].len() + c.len_utf8() <= if i == 1 { 128 } else { 64 } {
                            form.values[i].push(c);
                        }
                    }
                }
            }
        }
    }
}
/// Validates locally with the shared protocol rules (same checks the server runs),
/// then sends CreateCharacter / SelectCharacter, or opens the connection with Login / Register.
pub fn submit(mut form: ResMut<Form>, mut network: ResMut<Network>) {
    if !form.submit {
        return;
    }
    form.submit = false;
    if network.pending {
        return;
    }
    if network.mode == Mode::Selection {
        if form.creating {
            if let Err(e) = validate_character(&form.values[2], form.gender) {
                network.status = e.into();
                return;
            }
            network.request(ClientMessage::CreateCharacter {
                nickname: form.values[2].clone(),
                gender: form.gender,
            });
        } else if let Some(character_id) = network.selected {
            network.request(ClientMessage::SelectCharacter { character_id });
        }
        return;
    }
    if network.mode != Mode::Menu {
        return;
    }
    if let Err(e) = validate_credentials(&form.values[0], &form.values[1]) {
        network.status = e.into();
        return;
    }
    let login = form.values[0].clone();
    // The password is moved out, so it is not kept in the form after sending.
    let password = std::mem::take(&mut form.values[1]);
    network.begin(if form.register {
        ClientMessage::Register {
            version: PROTOCOL_VERSION,
            world: WORLD_VERSION.into(),
            login,
            password,
        }
    } else {
        ClientMessage::Login {
            version: PROTOCOL_VERSION,
            world: WORLD_VERSION.into(),
            login,
            password,
        }
    });
}
/// Rewrites every lobby text and picks which `Section`s are visible.
/// The `Without<...>` filters keep the four `Single<&mut Text>` queries disjoint (Bevy B0001).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn display(
    form: Res<Form>,
    network: Res<Network>,
    mut lobby: Single<&mut Node, With<Lobby>>,
    mut sections: Query<(&Section, &mut Node), Without<Lobby>>,
    mut labels: Query<(&Label, &mut Text)>,
    mut feedback: Single<
        &mut Text,
        (
            With<Feedback>,
            Without<Label>,
            Without<Preview>,
            Without<Title>,
        ),
    >,
    mut preview: Single<
        &mut Text,
        (
            With<Preview>,
            Without<Label>,
            Without<Feedback>,
            Without<Title>,
        ),
    >,
    mut title: Single<
        &mut Text,
        (
            With<Title>,
            Without<Label>,
            Without<Feedback>,
            Without<Preview>,
        ),
    >,
) {
    lobby.display = if network.playing() {
        Display::None
    } else {
        Display::Flex
    };
    for (section, mut node) in &mut sections {
        node.display = if match section {
            Section::Auth => network.mode != Mode::Selection,
            Section::Selection => network.mode == Mode::Selection && !form.creating,
            Section::Roster => {
                network.mode == Mode::Selection && !form.creating && !network.characters.is_empty()
            }
            Section::Creation => network.mode == Mode::Selection && form.creating,
            Section::Options => network.mode == Mode::Selection && !form.creating && form.menu_open,
        } {
            Display::Flex
        } else {
            Display::None
        };
    }
    title.0 = if network.mode == Mode::Selection {
        if form.creating {
            "Создание персонажа".into()
        } else {
            "Выбор персонажа".into()
        }
    } else if form.register {
        "Регистрация аккаунта".into()
    } else {
        "Вход в аккаунт".into()
    };
    feedback.0 = if network.mode == Mode::Selection
        && network.characters.is_empty()
        && !form.creating
        && !network.pending
    {
        "Персонажей пока нет. Создайте первого через «Меню».".into()
    // Default prompts set by `network.rs` duplicate the title, so they are hidden.
    } else if [
        "Войдите в аккаунт или зарегистрируйтесь",
        "Войдите в аккаунт",
        "Выберите героя для входа в мир",
        "Создайте первого персонажа",
    ]
    .contains(&network.status.as_str())
    {
        String::new()
    } else {
        network.status.clone()
    };
    for (label, mut text) in &mut labels {
        text.0 = match label.0 {
            Action::Field(i) => format!(
                "{}: {}{}",
                ["Логин", "Пароль", "Имя героя"][i],
                if i == 1 {
                    "•".repeat(form.values[i].chars().count())
                } else {
                    form.values[i].clone()
                },
                if form.focus == i { "│" } else { "" }
            ),
            Action::Mode => if form.register {
                "← Назад ко входу"
            } else {
                "Регистрация"
            }
            .into(),
            Action::Gender => format!(
                "Пол: {}  ›",
                if form.gender == 1 {
                    "Мужской"
                } else {
                    "Женский"
                }
            ),
            Action::Submit => if network.pending || network.mode == Mode::Connecting {
                "Подождите…"
            } else if network.mode == Mode::Selection {
                "Создать персонажа"
            } else if form.register {
                "Создать аккаунт"
            } else {
                "Войти"
            }
            .into(),
            Action::Slot(slot) => network
                .characters
                .iter()
                .find(|c| c.slot as usize == slot + 1)
                .map(|c| {
                    format!(
                        "{} {} · ур. {:.0}\n{}",
                        if network.selected == Some(c.id) {
                            "▸"
                        } else {
                            " "
                        },
                        c.nickname,
                        // Panics if the server omits "level".
                        c.stats["level"],
                        if c.gender == 1 {
                            "Мужчина"
                        } else {
                            "Женщина"
                        }
                    )
                })
                .unwrap_or_default(),
            Action::New => if network.characters.len() < CHARACTER_LIMIT {
                "Создать нового персонажа"
            } else {
                "Все 12 слотов заняты"
            }
            .into(),
            Action::Enter => if network.pending {
                "Подождите…"
            } else if network.selected.is_some() {
                "Войти"
            } else {
                "Сначала создайте персонажа"
            }
            .into(),
            Action::Back => "Назад к персонажам".into(),
            Action::Logout => "Выйти из аккаунта".into(),
            Action::Menu => if form.menu_open {
                "Меню ▴"
            } else {
                "Меню ▾"
            }
            .into(),
        };
    }
    preview.0 = if network.mode == Mode::Selection && form.creating {
        let s = starting_stats().unwrap();
        format!(
            "Здоровье {:.0} · Мана {:.0} · Выносливость {:.0}\nСИЛ {:.0} · ЛОВ {:.0} · ВЫН {:.0} · ИНТ {:.0} · МДР {:.0} · ДУХ {:.0} · УДЧ {:.0}\nВсе герои — люди без класса. Свободные очки ({:.0}) распределяются в окне C; путь определяют оружие и навыки.",
            s["hp"],
            s["mana"],
            s["stamina"],
            s["STR"],
            s["DEX"],
            s["CON"],
            s["INT_"],
            s["WIT"],
            s["MEN"],
            s["LUC"],
            s["stat_point"]
        )
    } else {
        String::new()
    };
}

/// Responsive sizes: hides the model panel below 840 UI px, widens the form on the roster,
/// hides empty roster slots and empty status lines.
#[allow(clippy::type_complexity)]
pub fn layout(
    network: Res<Network>,
    form: Res<Form>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    scale: Res<UiScale>,
    mut nodes: Query<
        (
            Option<&Action>,
            Has<Panel>,
            Option<&Text>,
            Has<Feedback>,
            Has<Preview>,
            Has<ModelPanel>,
            &mut Node,
        ),
        Or<(
            With<Action>,
            With<Panel>,
            With<Feedback>,
            With<Preview>,
            With<ModelPanel>,
        )>,
    >,
) {
    let width = window.width() / scale.0;
    let show_model = width >= 840. && form.model_preview(&network).is_some();
    for (action, panel, text, feedback, preview, model, mut node) in &mut nodes {
        if model {
            node.display = if show_model {
                Display::Flex
            } else {
                Display::None
            };
            node.width = px(if width >= 1000. { 280. } else { 220. });
        }
        if panel {
            node.width = px(if show_model {
                if form.creating { 460. } else { 580. }
            } else if network.mode == Mode::Selection {
                620.
            } else {
                460.
            });
        }
        if feedback || preview {
            node.display = if text.is_some_and(|t| !t.0.is_empty()) {
                Display::Flex
            } else {
                Display::None
            };
        }
        if let Some(action) = action {
            node.display = if match action {
                Action::Slot(slot) => network
                    .characters
                    .iter()
                    .any(|c| c.slot as usize == slot + 1),
                Action::Enter => !network.characters.is_empty(),
                _ => true,
            } {
                Display::Flex
            } else {
                Display::None
            };
            match action {
                Action::Menu => {
                    node.width = px(100);
                    node.margin.left = Val::Auto;
                }
                Action::Enter => {
                    node.width = Val::Auto;
                    node.flex_grow = 1.;
                }
                Action::Mode => {
                    node.width = Val::Auto;
                    node.margin.left = Val::Auto;
                    node.padding = UiRect::axes(px(8), px(4));
                }
                Action::Slot(_) => {
                    node.width = percent(if network.characters.len() == 1 {
                        100.
                    } else {
                        48.
                    });
                }
                _ => {}
            }
        }
    }
}

/// Keep keyboard focus visible independently of mouse hover.
/// Lobby buttons are `skin::OwnStyle`, so `ui::button_feedback` leaves them alone.
pub fn style(
    form: Res<Form>,
    network: Res<Network>,
    mut buttons: Query<(
        &Action,
        &Interaction,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
) {
    for (action, interaction, mut background, mut border) in &mut buttons {
        let active = matches!(action, Action::Field(i) if *i == form.focus)
            || matches!(action, Action::Slot(slot) if network.characters.iter().any(|c| c.slot as usize == slot + 1 && Some(c.id) == network.selected));
        let primary = matches!(action, Action::Submit | Action::Enter);
        *border = BorderColor::all(if active || primary || *interaction != Interaction::None {
            crate::ui::GOLD
        } else {
            crate::ui::EDGE
        });
        background.0 = match interaction {
            Interaction::Pressed => Color::srgb(0.25, 0.29, 0.29),
            Interaction::Hovered => Color::srgb(0.16, 0.21, 0.24),
            _ if primary => Color::srgb(0.21, 0.23, 0.22),
            _ if matches!(action, Action::Field(_)) => Color::srgb(0.025, 0.035, 0.045),
            _ => crate::ui::BUTTON,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roster_hides_empty_slots_and_menu_opens_character_creation() {
        let mut app = App::new();
        let mut network = Network::default();
        network.mode = Mode::Selection;
        app.insert_resource(network)
            .init_resource::<Form>()
            .init_resource::<Movement>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<UiScale>()
            .add_systems(Update, (buttons, layout).chain());
        app.world_mut()
            .spawn((Window::default(), bevy::window::PrimaryWindow));
        let first = app
            .world_mut()
            .spawn((Action::Slot(0), Node::default()))
            .id();
        let empty = app
            .world_mut()
            .spawn((Action::Slot(1), Node::default()))
            .id();
        let enter = app.world_mut().spawn((Action::Enter, Node::default())).id();
        let menu = app
            .world_mut()
            .spawn((Action::Menu, Node::default(), Interaction::None))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Node>(first).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(enter).unwrap().display,
            Display::None
        );
        *app.world_mut().get_mut::<Interaction>(menu).unwrap() = Interaction::Pressed;
        app.update();
        assert!(app.world().resource::<Form>().menu_open);
        app.world_mut().spawn((Action::New, Interaction::Pressed));
        app.update();
        assert!(app.world().resource::<Form>().creating);
        assert!(!app.world().resource::<Form>().menu_open);
        app.world_mut()
            .resource_mut::<Network>()
            .characters
            .push(mortown_protocol::Character {
                id: 1,
                slot: 1,
                nickname: "Hero".into(),
                gender: 1,
                stats: starting_stats().unwrap(),
                items: vec![],
            });
        app.update();
        assert_eq!(
            app.world().get::<Node>(first).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Node>(empty).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(enter).unwrap().display,
            Display::Flex
        );
    }
}
