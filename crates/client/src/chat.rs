//! Chat panel (bottom left): channel tabs, message list, input line, ignore/report.
//! `Log` (stored as `Network::chat`) holds what the server sent; `Form` is local input state.
//! `input` owns the keyboard while the chat is focused, so typed letters never trigger
//! game hotkeys; it runs before every other hotkey system in the Update chain.
//! Submodules: `windows` (server-driven dialogs and settings), `admin` (staff panel).
pub mod admin;
pub mod windows;
use crate::{
    actionbar::{Icons, label},
    network::{Mode, Network},
    ui::BUTTON,
};
use bevy::{
    input::{
        ButtonState,
        keyboard::KeyboardInput,
        mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    },
    prelude::*,
};
use mortown_protocol::chat::{Channel, Command, Message, Reply};
/// Chat data received from the server; lives inside `Network` and is filled by `receive`.
#[derive(Default)]
pub struct Log {
    /// Last 100 messages; must not exceed the 100 `Entry` rows spawned in `setup`.
    pub messages: Vec<Message>,
    pub status: String,
    /// A chat command is awaiting the server's reply; blocks sending another.
    pub pending: bool,
    /// Server accepted the last command; `input` then clears the draft.
    pub sent: bool,
    /// Bumped on every change so the list can auto-scroll to the newest line.
    pub revision: u64,
    /// "Система" tab lines (last 100).
    pub system: Vec<String>,
    /// Ignored sender ids, synced from the server.
    pub ignored: std::collections::HashSet<i64>,
    pub ignore_all: bool,
    pub silence: bool,
    /// Modal window requested by the server (settings, help, confirm, admin); see `windows`.
    pub dialog: Option<mortown_protocol::chat::Dialog>,
}
impl Log {
    /// Shows a line in the status bar and appends it to the "Система" tab.
    fn record(&mut self, message: String) {
        self.status = message.clone();
        self.system.push(message);
        if self.system.len() > 100 {
            self.system.remove(0);
        }
        self.revision += 1;
    }

    /// Called by `network::receive` for every chat `Reply`. Status, Acknowledged, Notice
    /// and Dialog end the pending request; the others are unsolicited pushes.
    pub fn receive(&mut self, reply: Reply, _owner: i64) {
        match reply {
            Reply::Message { message } => {
                self.messages.push(message);
                if self.messages.len() > 100 {
                    self.messages.remove(0);
                }
                self.revision += 1;
            }
            Reply::Status { message } => {
                self.pending = false;
                self.record(message);
            }
            Reply::Acknowledged => {
                self.pending = false;
                self.sent = true;
                self.status.clear();
            }
            Reply::Notice { message } => {
                self.pending = false;
                self.sent = true;
                self.record(message);
            }
            Reply::Dialog { dialog } => {
                self.pending = false;
                self.sent = true;
                self.status.clear();
                self.dialog = Some(dialog);
            }
            Reply::Preferences {
                ignored,
                ignore_all,
                silence,
            } => {
                self.ignored = ignored.into_iter().collect();
                self.ignore_all = ignore_all;
                self.silence = silence;
            }
            Reply::Announcement { message } => self.record(format!("[Объявление] {message}")),
            Reply::Logout => {} // Normal logout arrives as Saved after persistence.
        }
    }
}
/// Local chat input state; reset whenever the hero changes or the client leaves the world.
#[derive(Resource, Default)]
pub struct Form {
    /// Keys pressed while typing; hidden from gameplay until physically released.
    suppressed: std::collections::HashSet<KeyCode>,
    last_party: Option<u64>,
    last_alliance: Option<i64>,
    /// Active tab; `None` = "Система".
    channel: Option<Channel>,
    focused: bool,
    draft: String,
    /// Byte offset into `draft` (always on a char boundary).
    caret: usize,
    owner: Option<i64>,
    /// Clicked message (id, sender) for Ignore/Report.
    selected: Option<(i64, String)>,
    revision: u64,
    /// Stick to the newest message; cleared when the user scrolls up.
    bottom: bool,
    /// Set by `windows::input` while a dialog is open, so chat swallows keys that frame too.
    modal_capture: bool,
}
/// Chat panel root; `movable::Fixed`, so it cannot be dragged.
#[derive(Component)]
pub struct Root;
#[derive(Component)]
pub struct Scroll;
/// One of 100 pre-spawned message rows; index into `Log::messages`.
#[derive(Component)]
pub struct Entry(pub usize);
#[derive(Component)]
pub struct EntryText(pub usize);
/// Input line text.
#[derive(Component)]
pub struct FieldText;
#[derive(Component)]
pub struct Status;
/// Rows visible only on the "Система" tab.
#[derive(Component)]
pub struct System;
/// The system log text (`Log::system`).
#[derive(Component)]
pub struct ConsoleSystem;
#[derive(Component, Clone, Copy)]
pub enum Action {
    Channel(Option<Channel>),
    Field,
    Ignore,
    Report,
    Bottom,
}
fn button(action: Action) -> impl Bundle {
    (
        Button,
        action,
        Node {
            padding: UiRect::all(px(4)),
            ..default()
        },
        BackgroundColor(BUTTON),
    )
}
pub fn setup(mut commands: Commands, icons: Res<Icons>) {
    commands.init_resource::<Form>();
    commands.init_resource::<admin::Board>();
    commands
        .spawn((
            Root,
            crate::ui::WorldInputBlocker,
            crate::movable::Fixed,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                bottom: px(0),
                width: px(300),
                padding: UiRect::all(px(8)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.04, 0.055, 0.68)),
            // Below window overlays (10).
            GlobalZIndex(5),
        ))
        .with_children(|p| {
            p.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                row_gap: px(4),
                column_gap: px(5),
                ..default()
            })
            .with_children(|row| {
                for (channel, name) in [
                    (Some(Channel::General), "Рядом"),
                    (Some(Channel::Local), "Рядом"),
                    (Some(Channel::Group), "Группа"),
                    (Some(Channel::Alliance), "Альянс"),
                    (None, "Система"),
                ] {
                    row.spawn(button(Action::Channel(channel)))
                        .with_children(|b| {
                            b.spawn(label(name, &icons.1, 12.));
                        });
                }
            });
            p.spawn(button(Action::Bottom)).with_children(|b| {
                b.spawn(label("↓ К новым", &icons.1, 11.));
            });
            p.spawn((
                Scroll,
                Button,
                BackgroundColor(Color::NONE),
                ScrollPosition::default(),
                Node {
                    height: px(128),
                    overflow: Overflow::scroll_y(),
                    flex_direction: FlexDirection::Column,
                    flex_shrink: 0.,
                    ..default()
                },
            ))
            .with_children(|list| {
                list.spawn((
                    System,
                    crate::ui::Readout::Journal,
                    label("", &icons.1, 12.),
                ));
                list.spawn((System, ConsoleSystem, label("", &icons.1, 12.)));
                // Fixed pool of rows, reused instead of spawning per message; 100 = `Log` cap.
                for index in 0..100 {
                    list.spawn((
                        Entry(index),
                        Button,
                        BackgroundColor(Color::NONE),
                        Node {
                            display: Display::None,
                            flex_shrink: 0.,
                            width: percent(100),
                            padding: UiRect::vertical(px(3)),
                            ..default()
                        },
                    ))
                    .with_children(|row| {
                        row.spawn((
                            TextShadow::default(),
                            EntryText(index),
                            label("", &icons.1, 12.),
                            Node {
                                width: percent(100),
                                ..default()
                            },
                        ));
                    });
                }
            });
            p.spawn((Status, label("", &icons.1, 11.)));
            p.spawn(Node {
                column_gap: px(5),
                ..default()
            })
            .with_children(|row| {
                for (action, name) in [(Action::Ignore, "Игнор"), (Action::Report, "Пожаловаться")]
                {
                    row.spawn(button(action)).with_children(|b| {
                        b.spawn(label(name, &icons.1, 11.));
                    });
                }
            });
            p.spawn(button(Action::Field)).with_children(|b| {
                b.spawn((FieldText, label("Enter — чат", &icons.1, 13.)));
            });
        });
}
/// Turns the draft into a server command: a recognised "/" command → `Admin` (with the current
/// target), `"nick text` → whisper, anything else → message to the active channel.
/// Validation uses the shared protocol rules; errors go to the system log, nothing is sent.
fn submit(
    form: &mut Form,
    network: &mut Network,
    target: Option<mortown_protocol::admin::Target>,
    person: Option<i64>,
) {
    if network.chat.pending {
        return;
    }
    let raw = form.draft.as_str();
    if let Some(name) = raw.strip_prefix("/имя ") {
        match mortown_protocol::society::claim(name) {
            Ok(name) => network
                .society_request(mortown_protocol::society::Command::Present { name: name.into() }),
            Err(e) => network.chat.record(e.into()),
        }
        form.draft.clear();
        return;
    }
    if raw == "/лицо" {
        network.society_request(mortown_protocol::society::Command::Cover { hidden: true });
        form.draft.clear();
        return;
    }
    if raw == "/открыть" {
        network.society_request(mortown_protocol::society::Command::Cover { hidden: false });
        form.draft.clear();
        return;
    }
    if let Some(text) = raw.strip_prefix("/письмо ") {
        network.work_request(mortown_protocol::work::Command::WriteDocument { text: text.into() });
        form.draft.clear();
        return;
    }
    if raw == "/птица" {
        network.work_request(mortown_protocol::work::Command::DispatchDocument);
        form.draft.clear();
        return;
    }
    if let Some(note) = raw.strip_prefix("/запомнить ") {
        if let Some(person) = person {
            match mortown_protocol::society::note(note) {
                Ok(note) => network.society_request(mortown_protocol::society::Command::Remember {
                    person,
                    note: note.into(),
                }),
                Err(e) => network.chat.record(e.into()),
            }
        } else {
            network.chat.record("Сначала выберите человека".into());
        }
        form.draft.clear();
        return;
    }
    let command = match mortown_protocol::chat::commands::parse(raw) {
        Err(e) => {
            network.chat.record(e.into());
            return;
        }
        Ok(Some(_)) => Command::Admin {
            text: raw.into(),
            target,
        },
        Ok(None) => {
            if let Some(whisper) = raw.strip_prefix('"') {
                let Some((nickname, text)) = whisper.split_once(char::is_whitespace) else {
                    network.chat.record("Личное сообщение: \"ник текст".into());
                    return;
                };
                if nickname.is_empty() {
                    network.chat.record("Укажите имя получателя".into());
                    return;
                }
                if let Err(e) = mortown_protocol::chat::validate(text) {
                    network.chat.record(e.into());
                    return;
                }
                Command::Whisper {
                    nickname: nickname.into(),
                    text: text.into(),
                }
            } else {
                if let Err(e) = mortown_protocol::chat::validate(raw) {
                    network.chat.record(e.into());
                    return;
                }
                Command::Send {
                    channel: form.channel.unwrap_or(Channel::General),
                    text: raw.into(),
                }
            }
        }
    };
    network.chat_request(command);
}

/// Focus, typing, scrolling and chat buttons. While focused it clears all keyboard state
/// at the end (`keys.reset_all`) so later systems see no key presses.
pub fn input(
    mut form: ResMut<Form>,
    combat: Res<crate::combat::CombatUi>,
    mut network: ResMut<Network>,
    mut panels: ResMut<crate::ui::CharacterWindow>,
    mut crafting: ResMut<crate::crafting::Open>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    // Two readers of the same messages, each with its own cursor: releases are always read,
    // typed events only when focused.
    mut events: MessageReader<KeyboardInput>,
    mut releases: MessageReader<KeyboardInput>,
    mut wheel: ResMut<AccumulatedMouseScroll>,
    mut movement: ResMut<crate::Movement>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    entries: Query<(&Interaction, &Entry), Changed<Interaction>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut scroll: Single<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<Scroll>>,
) {
    let pressed: Vec<_> = keys.get_pressed().copied().collect();
    for event in releases.read() {
        if event.state == ButtonState::Released {
            form.suppressed.remove(&event.key_code);
        }
    }
    // Keys captured by the chat stay unavailable to gameplay until physically released.
    for key in &form.suppressed {
        keys.reset(*key);
    }
    let owner = network.character.as_ref().map(|c| c.id);
    let party = network.party.group.as_ref().map(|g| g.id);
    if form.last_party != party {
        if form.channel == Some(Channel::Group) {
            // A draft addressed to an old group must never silently go to a new audience.
            form.draft.clear();
            form.caret = 0;
            form.focused = false;
            if party.is_none() {
                form.channel = Some(Channel::General);
            }
        }
        form.last_party = party;
    }
    let alliance = network.alliance.alliance.as_ref().map(|a| a.id);
    if form.last_alliance != alliance {
        if form.channel == Some(Channel::Alliance) {
            form.draft.clear();
            form.caret = 0;
            form.focused = false;
            if alliance.is_none() {
                form.channel = Some(Channel::General);
            }
        }
        form.last_alliance = alliance;
    }
    if network.mode != Mode::Online || form.owner != owner {
        *form = Form {
            owner,
            channel: (network.mode == Mode::Online).then_some(Channel::General),
            bottom: true,
            ..default()
        };
    }
    if network.mode != Mode::Online {
        events.clear();
        return;
    }
    if network.chat.sent {
        network.chat.sent = false;
        form.draft.clear();
        form.caret = 0;
    }
    // A modal dialog owns the keyboard (including the frame it was closed on).
    if network.chat.dialog.is_some() || std::mem::take(&mut form.modal_capture) {
        form.focused = false;
        form.suppressed.extend(pressed);
        *movement = default();
        keys.reset_all();
        events.clear();
        return;
    }
    if window
        .physical_cursor_position()
        .is_some_and(|p| crate::actionbar::contains(scroll.0, scroll.1, p))
        && wheel.delta.y != 0.
    {
        // Mouse wheels report lines, touchpads pixels; ~24 px per line.
        let amount = wheel.delta.y
            * if wheel.unit == MouseScrollUnit::Line {
                24.
            } else {
                1.
            };
        scroll.2.y = (scroll.2.y - amount).max(0.);
        form.bottom = false;
        // Consume the wheel so the camera does not zoom too.
        wheel.delta = Vec2::ZERO;
    }
    if form.revision != network.chat.revision {
        form.revision = network.chat.revision;
        if form.bottom {
            // Bevy clamps this to the real maximum: "scroll to end".
            scroll.2.y = f32::MAX;
        }
    }
    let wants_chat = keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::NumpadEnter)
        || buttons
            .iter()
            .any(|(i, a)| *i == Interaction::Pressed && matches!(a, Action::Field));
    // Keep Enter for letter editing, but ordinary game windows must not prevent chat focus.
    if wants_chat && !panels.alliance && network.exchange.is_none() {
        *panels = default();
        crafting.0 = false;
    }
    let available = !panels.open && !crafting.0 && network.exchange.is_none();
    if !available {
        form.focused = false;
        events.clear();
        return;
    }
    if let Some((_, entry)) = entries.iter().find(|(i, _)| **i == Interaction::Pressed)
        && let Some(m) = network.chat.messages.get(entry.0)
        // Own messages can't be ignored or reported.
        && Some(m.sender) != owner
    {
        form.selected = Some((m.id, m.nickname.clone()));
    }
    let mut opened = false;
    if let Some((_, action)) = buttons.iter().find(|(i, _)| **i == Interaction::Pressed) {
        match *action {
            Action::Bottom => {
                form.bottom = true;
                scroll.2.y = f32::MAX;
            }
            Action::Channel(channel) => {
                form.channel = channel;
                form.selected = None;
                form.bottom = true;
                scroll.2.y = f32::MAX;
            }
            Action::Field => {
                opened = !form.focused;
                form.focused = true;
            }
            Action::Ignore => {
                if let Some((_, nickname)) = &form.selected {
                    network.chat_request(Command::Ignore {
                        nickname: nickname.clone(),
                        blocked: true,
                    });
                    form.selected = None;
                }
            }
            Action::Report => {
                if let Some((id, _)) = form.selected {
                    network.chat_request(Command::Report { id });
                    form.selected = None;
                }
            }
        }
    }
    if !form.focused
        && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter))
    {
        if form.channel.is_none() {
            form.channel = Some(Channel::General);
        }
        form.focused = true;
        opened = true;
    }
    if !form.focused {
        events.clear();
        return;
    }
    // Focused: stop walking and hide every held key from gameplay.
    form.suppressed.extend(pressed);
    *movement = default();
    if keys.just_pressed(KeyCode::Escape) {
        form.focused = false;
        events.clear();
        keys.reset_all();
        return;
    }
    // Skip the frame focus was gained, so the opening Enter isn't also typed/submitted.
    if !opened && !network.chat.pending {
        for event in events.read() {
            if event.state != ButtonState::Pressed {
                continue;
            }
            match event.key_code {
                KeyCode::Escape => {
                    form.focused = false;
                    break;
                }
                KeyCode::Enter | KeyCode::NumpadEnter if event.repeat => {}
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    submit(
                        &mut form,
                        &mut network,
                        admin::target(&combat),
                        combat.player,
                    );
                    break;
                }
                // Caret moves by whole UTF-8 characters, never into the middle of a Cyrillic letter.
                KeyCode::Backspace => {
                    if form.caret > 0 {
                        let prev = form.draft[..form.caret]
                            .char_indices()
                            .last()
                            .map_or(0, |(i, _)| i);
                        let end = form.caret;
                        form.draft.replace_range(prev..end, "");
                        form.caret = prev;
                    }
                }
                KeyCode::Delete => {
                    if let Some(c) = form.draft[form.caret..].chars().next() {
                        let start = form.caret;
                        form.draft.replace_range(start..start + c.len_utf8(), "");
                    }
                }
                KeyCode::ArrowLeft => {
                    form.caret = form.draft[..form.caret]
                        .char_indices()
                        .last()
                        .map_or(0, |(i, _)| i)
                }
                KeyCode::ArrowRight => {
                    form.caret += form.draft[form.caret..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8)
                }
                KeyCode::Home => form.caret = 0,
                KeyCode::End => form.caret = form.draft.len(),
                _ => {
                    if let Some(text) = &event.text {
                        // Limit is in characters here (not bytes, unlike the lobby).
                        let left = mortown_protocol::chat::INPUT_LIMIT
                            .saturating_sub(form.draft.chars().count());
                        let added: String = text
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(left)
                            .collect();
                        let at = form.caret;
                        form.draft.insert_str(at, &added);
                        form.caret += added.len();
                    }
                }
            }
        }
    }
    events.clear();
    keys.reset_all();
}
/// Fills the row pool and the input line. The many `Without<...>` filters only keep the
/// queries disjoint (Bevy B0001).
pub fn display(
    network: Res<Network>,
    form: Res<Form>,
    settings: Res<windows::Settings>,
    mut root: Single<
        (&mut Node, &mut BackgroundColor),
        (With<Root>, Without<Entry>, Without<System>, Without<Action>),
    >,
    mut system: Query<&mut Node, (With<System>, Without<Root>, Without<Entry>, Without<Action>)>,
    mut system_log: Single<
        &mut Text,
        (
            With<ConsoleSystem>,
            Without<EntryText>,
            Without<FieldText>,
            Without<Status>,
        ),
    >,
    mut entries: Query<(&Entry, &mut Node), (Without<Root>, Without<System>, Without<Action>)>,
    mut labels: Query<
        (&EntryText, &mut Text, &mut TextColor, &mut TextFont),
        (Without<FieldText>, Without<Status>, Without<ConsoleSystem>),
    >,
    mut field: Single<
        &mut Text,
        (
            With<FieldText>,
            Without<EntryText>,
            Without<Status>,
            Without<ConsoleSystem>,
        ),
    >,
    mut status: Single<
        &mut Text,
        (
            With<Status>,
            Without<FieldText>,
            Without<EntryText>,
            Without<ConsoleSystem>,
        ),
    >,
    mut buttons: Query<
        (&Action, &mut Node, &mut BackgroundColor),
        (Without<Root>, Without<System>, Without<Entry>),
    >,
) {
    root.1.0 = Color::srgba(
        0.025,
        0.04,
        0.055,
        if form.focused { 1.0 } else { settings.opacity },
    );
    root.0.display = if network.playing() {
        Display::Flex
    } else {
        Display::None
    };
    root.0.width = px(settings.width);
    system_log.0 = network.chat.system.join("\n");
    for mut system in &mut system {
        system.display = if form.channel.is_none() {
            Display::Flex
        } else {
            Display::None
        };
    }
    // Row visible if: not ignored, belongs to this tab (whispers show on every channel tab),
    // and group/alliance messages are from the player's current group/alliance.
    for (entry, mut node) in &mut entries {
        let display = if network.chat.messages.get(entry.0).is_some_and(|m| {
            !network.chat.ignore_all
                && !network.chat.ignored.contains(&m.sender)
                && (Some(m.channel) == form.channel
                    || (m.channel == Channel::Private && form.channel.is_some()))
                && (m.channel != Channel::Group
                    || m.party == network.party.group.as_ref().map(|g| g.id))
                && (m.channel != Channel::Alliance
                    || m.alliance == network.alliance.alliance.as_ref().map(|a| a.id))
        }) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    for (entry, mut text, mut color, mut font) in &mut labels {
        if font.font_size != settings.font {
            font.font_size = settings.font;
        }
        let value = network
            .chat
            .messages
            .get(entry.0)
            .map_or(String::new(), |m| {
                if let Some((_, recipient)) = &m.recipient {
                    format!("[ЛС] {} → {}: {}", m.nickname, recipient, m.text)
                } else {
                    format!("{}: {}", m.nickname, m.text)
                }
            });
        color.0 = match network.chat.messages.get(entry.0).map(|m| m.channel) {
            Some(Channel::Private) => Color::srgb(0.85, 0.68, 0.95),
            Some(Channel::Local) => Color::srgb(0.65, 0.82, 0.73),
            Some(Channel::Alliance) => Color::srgb(0.83, 0.75, 0.52),
            Some(Channel::Group) => Color::srgb(0.78, 0.72, 0.92),
            _ => Color::srgb(0.77, 0.85, 0.91),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
    // The field is one line: show the last 36 chars, or 24 before / 12 after the caret.
    let draft: String = form
        .draft
        .chars()
        .rev()
        .take(36)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    field.0 = if form.focused {
        let before: String = form.draft[..form.caret]
            .chars()
            .rev()
            .take(24)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let after: String = form.draft[form.caret..].chars().take(12).collect();
        format!("{before}│{after}")
    } else if form.draft.is_empty() {
        "Enter — чат".into()
    } else {
        draft
    };
    status.0 = if let Some((_, name)) = &form.selected {
        format!("Выбрано сообщение: {name}")
    } else {
        network.chat.status.clone()
    };
    for (action, mut node, mut color) in &mut buttons {
        let show = match action {
            Action::Ignore | Action::Report => form.selected.is_some() && !network.chat.pending,
            Action::Bottom => !form.bottom,
            Action::Field => true,
            Action::Channel(Some(Channel::Group)) => network.party.group.is_some(),
            Action::Channel(Some(Channel::Alliance)) => network.alliance.alliance.is_some(),
            _ => true,
        };
        node.display = if show { Display::Flex } else { Display::None };
        *color = BackgroundColor(if matches!(action,Action::Channel(c) if *c==form.channel) {
            Color::srgb(0.19, 0.25, 0.27)
        } else {
            BUTTON
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{input::keyboard::Key, window::PrimaryWindow};
    fn app() -> (App, Entity) {
        let mut app = App::new();
        let mut network = Network::default();
        network.mode = Mode::Online;
        app.insert_resource(network)
            .init_resource::<Form>()
            .init_resource::<crate::combat::CombatUi>()
            .init_resource::<crate::ui::CharacterWindow>()
            .init_resource::<crate::crafting::Open>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<crate::Movement>()
            .add_message::<KeyboardInput>()
            .add_systems(Update, input);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut().spawn((
            Scroll,
            ComputedNode::default(),
            UiGlobalTransform::default(),
            ScrollPosition::default(),
        ));
        (app, window)
    }
    fn event(app: &mut App, window: Entity, key: KeyCode, text: Option<&str>, state: ButtonState) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        match state {
            ButtonState::Pressed => keys.press(key),
            ButtonState::Released => keys.release(key),
        };
        app.world_mut().write_message(KeyboardInput {
            key_code: key,
            logical_key: text.map_or(
                Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                |s| Key::Character(s.into()),
            ),
            state,
            text: text.map(Into::into),
            repeat: false,
            window,
        });
        app.update();
    }
    #[test]
    fn typing_unicode_blocks_movement_windows_and_skills() {
        let (mut a, w) = app();
        event(&mut a, w, KeyCode::Enter, None, ButtonState::Pressed);
        for (key, text) in [
            (KeyCode::KeyW, "ц"),
            (KeyCode::KeyI, "ш"),
            (KeyCode::KeyG, "п"),
            (KeyCode::Digit1, "1"),
        ] {
            event(&mut a, w, key, Some(text), ButtonState::Pressed);
            let keys = a.world().resource::<ButtonInput<KeyCode>>();
            assert!(!keys.pressed(key));
            assert!(!keys.just_pressed(key));
        }
        assert_eq!(a.world().resource::<Form>().draft, "цшп1");
        assert!(!a.world().resource::<crate::ui::CharacterWindow>().open);
    }
    #[test]
    fn server_ack_keeps_focus_and_escape_waits_for_held_keys_release() {
        let (mut a, w) = app();
        event(&mut a, w, KeyCode::Enter, None, ButtonState::Pressed);
        event(&mut a, w, KeyCode::KeyW, Some("w"), ButtonState::Pressed);
        a.world_mut().resource_mut::<Network>().chat.sent = true;
        a.update();
        assert!(a.world().resource::<Form>().focused);
        event(&mut a, w, KeyCode::Escape, None, ButtonState::Pressed);
        assert!(!a.world().resource::<Form>().focused);
        event(&mut a, w, KeyCode::KeyW, Some("w"), ButtonState::Pressed);
        assert!(
            !a.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyW)
        );
        event(&mut a, w, KeyCode::KeyW, None, ButtonState::Released);
        event(&mut a, w, KeyCode::KeyW, Some("w"), ButtonState::Pressed);
        assert!(
            a.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyW)
        );
    }
    #[test]
    fn enter_from_system_tab_closes_inventory_and_focuses_chat() {
        let (mut a, w) = app();
        a.world_mut()
            .resource_mut::<crate::ui::CharacterWindow>()
            .open = true;
        a.world_mut()
            .resource_mut::<crate::ui::CharacterWindow>()
            .inventory = true;
        event(&mut a, w, KeyCode::NumpadEnter, None, ButtonState::Pressed);
        assert!(a.world().resource::<Form>().focused);
        assert_eq!(a.world().resource::<Form>().channel, Some(Channel::General));
        assert!(!a.world().resource::<crate::ui::CharacterWindow>().open);
    }
}
