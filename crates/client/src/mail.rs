use crate::{
    Movement,
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, INK, PANEL},
};
use bevy::{
    input::{
        ButtonState,
        keyboard::KeyboardInput,
        mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    },
    prelude::*,
};
use mortown_protocol::mail::{self, Command, Folder, Letter, Page, Reply};

#[derive(Default)]
pub struct Mailbox {
    pub page: Option<Page>,
    pub letter: Option<Letter>,
    pub unread: u64,
    pub pending: bool,
    pub status: String,
    refresh: bool,
    sent_draft: Option<String>,
}
impl Mailbox {
    pub fn receive(&mut self, reply: Reply) {
        match reply {
            Reply::Unread { count } => {
                self.refresh |= count != self.unread;
                self.unread = count;
                return;
            }
            Reply::List { page } => {
                self.unread = page.unread;
                self.page = Some(page);
                self.refresh = false;
                self.status.clear();
            }
            Reply::Read { letter } => {
                if let Some(page) = &mut self.page
                    && let Some(entry) = page.entries.iter_mut().find(|e| e.id == letter.header.id)
                {
                    if page.folder == Folder::Inbox && !entry.read && letter.header.read {
                        self.unread = self.unread.saturating_sub(1);
                    }
                    entry.read = letter.header.read;
                }
                self.letter = Some(letter);
                self.status.clear();
            }
            Reply::Sent { draft, .. } => {
                self.sent_draft = Some(draft);
                self.refresh = true;
                self.status = "Письмо отправлено".into();
            }
            Reply::Claimed { id, .. } => {
                if let Some(letter) = self.letter.as_mut().filter(|l| l.header.id == id) {
                    if let Some(a) = &mut letter.header.attachment {
                        a.claimed = true;
                    }
                    letter.header.read = true;
                }
                self.refresh = true;
                self.status = "Вложение получено".into();
            }
            Reply::Returned { id } => {
                if let Some(letter) = self.letter.as_mut().filter(|l| l.header.id == id) {
                    if let Some(a) = &mut letter.header.attachment {
                        a.returned = true;
                    }
                    letter.header.read = true;
                }
                self.refresh = true;
                self.status = "Вложение возвращено отправителю отдельным бесплатным письмом".into();
            }
            Reply::Deleted { id } => {
                if self.letter.as_ref().is_some_and(|l| l.header.id == id) {
                    self.letter = None;
                }
                self.refresh = true;
                self.status = "Ваша копия письма удалена".into();
            }
            Reply::Error { message } => self.status = message,
        }
        self.pending = false;
    }
}

#[derive(Resource, Default)]
pub struct Form {
    owner: Option<i64>,
    compose: bool,
    values: [String; 4],
    focus: usize,
    caret: usize,
    draft: String,
    delete_confirmation: bool,
    payment_confirmation: Option<(i64, u32)>,
    return_confirmation: Option<i64>,
    scroll_reset: bool,
    follow_end: bool,
    attachment: Option<mail::AttachmentRequest>,
    choosing: bool,
    item_page: usize,
}
impl Form {
    fn new_draft(&mut self) {
        // A persistent request identity for this draft, including retry after a server error.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        self.draft = format!("{now:032x}");
    }
    fn focus(&mut self, field: usize) {
        self.focus = field;
        self.caret = self.values[field].len();
    }
}

#[derive(Component, Clone, Copy)]
pub enum Action {
    Open,
    Folder(Folder),
    New,
    Refresh,
    Row(usize),
    Previous,
    Next,
    Field(usize),
    Send,
    Reply,
    Delete,
    ConfirmDelete,
    CancelDelete,
    Choose,
    Detach,
    Less,
    More,
    All,
    Claim,
    Return,
    ConfirmReturn,
    CancelReturn,
    ConfirmPayment,
    CancelPayment,
}
#[derive(Component)]
pub(crate) enum Section {
    Overlay,
    Reader,
    Composer,
    Received,
}
#[derive(Component)]
pub(crate) enum Label {
    Badge,
    Row(usize),
    Page,
    Reader,
    Field(usize),
    Counter,
    Status,
    Attachment,
    Received,
}
#[derive(Component)]
pub(crate) enum Picture {
    Row(usize),
    Draft,
    Received,
}
#[derive(Component)]
pub(crate) struct Scroller(bool); // true: body editor, false: letter reader

fn text(value: &str, font: &Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}
fn button(action: Action) -> impl Bundle {
    (
        Button,
        action,
        Node {
            padding: UiRect::axes(px(12), px(8)),
            border: UiRect::all(px(1)),
            min_height: px(34),
            flex_shrink: 0.,
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(EDGE),
    )
}
fn row() -> Node {
    Node {
        column_gap: px(8),
        align_items: AlignItems::Center,
        flex_shrink: 0.,
        ..default()
    }
}

pub fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    commands.init_resource::<Form>();
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    commands
        .spawn((
            Section::Overlay,
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
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(1040),
                    max_width: percent(96),
                    height: px(680),
                    max_height: percent(94),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(18)),
                    border: UiRect::all(px(2)),
                    row_gap: px(12),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(EDGE),
            ))
            .with_children(|panel| {
                panel.spawn(text("Почта", &font, 23., GOLD));
                panel.spawn(row()).with_children(|bar| {
                    for (action, title) in [
                        (Action::Folder(Folder::Inbox), "Входящие"),
                        (Action::Folder(Folder::Sent), "Отправленные"),
                        (Action::New, "Написать"),
                        (Action::Refresh, "Обновить"),
                    ] {
                        bar.spawn(button(action)).with_children(|b| {
                            b.spawn(text(title, &font, 14., GOLD));
                        });
                    }
                });
                panel
                    .spawn(Node {
                        flex_grow: 1.,
                        min_height: px(0),
                        column_gap: px(16),
                        ..default()
                    })
                    .with_children(|columns| {
                        columns
                            .spawn(Node {
                                width: percent(38),
                                min_width: px(0),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8),
                                ..default()
                            })
                            .with_children(|list| {
                                for index in 0..mail::PAGE_SIZE {
                                    list.spawn(button(Action::Row(index))).with_children(|b| {
                                        b.spawn((
                                            Picture::Row(index),
                                            ImageNode::default(),
                                            Node {
                                                width: px(32),
                                                height: px(32),
                                                margin: UiRect::right(px(8)),
                                                display: Display::None,
                                                ..default()
                                            },
                                        ));
                                        b.spawn((Label::Row(index), text("", &font, 13., INK)));
                                    });
                                }
                                list.spawn((Label::Page, text("", &font, 13., INK)));
                                list.spawn(row()).with_children(|pager| {
                                    for (action, title) in
                                        [(Action::Previous, "← Назад"), (Action::Next, "Далее →")]
                                    {
                                        pager.spawn(button(action)).with_children(|b| {
                                            b.spawn(text(title, &font, 13., GOLD));
                                        });
                                    }
                                });
                            });
                        columns
                            .spawn(Node {
                                flex_grow: 1.,
                                flex_basis: px(0),
                                min_width: px(0),
                                min_height: px(0),
                                ..default()
                            })
                            .with_children(|right| {
                                right
                                    .spawn((
                                        Section::Reader,
                                        Node {
                                            width: percent(100),
                                            flex_direction: FlexDirection::Column,
                                            row_gap: px(10),
                                            min_height: px(0),
                                            ..default()
                                        },
                                    ))
                                    .with_children(|reader| {
                                        reader
                                            .spawn((
                                                Scroller(false),
                                                Interaction::None,
                                                ScrollPosition::default(),
                                                Node {
                                                    flex_grow: 1.,
                                                    flex_basis: px(0),
                                                    min_height: px(0),
                                                    width: percent(100),
                                                    overflow: Overflow::scroll_y(),
                                                    padding: UiRect::all(px(12)),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::srgba(
                                                    0.08, 0.06, 0.04, 0.65,
                                                )),
                                            ))
                                            .with_children(|body| {
                                                body.spawn((
                                                    Label::Reader,
                                                    text("Выберите письмо слева", &font, 15., INK),
                                                    Node {
                                                        width: percent(100),
                                                        flex_shrink: 0.,
                                                        ..default()
                                                    },
                                                ));
                                            });
                                        reader
                                            .spawn((
                                                Section::Received,
                                                Node {
                                                    column_gap: px(10),
                                                    align_items: AlignItems::Center,
                                                    ..default()
                                                },
                                            ))
                                            .with_children(|row| {
                                                row.spawn((
                                                    Picture::Received,
                                                    ImageNode::default(),
                                                    Node {
                                                        width: px(44),
                                                        height: px(44),
                                                        ..default()
                                                    },
                                                ));
                                                row.spawn((
                                                    Label::Received,
                                                    text("", &font, 13., GOLD),
                                                ));
                                            });
                                        reader.spawn(button(Action::Claim)).with_children(|b| {
                                            b.spawn(text("Забрать вложение", &font, 14., GOLD));
                                        });
                                        for (action, title) in [
                                            (
                                                Action::ConfirmPayment,
                                                "Подтвердить оплату и забрать",
                                            ),
                                            (Action::CancelPayment, "Отмена оплаты"),
                                            (Action::Return, "Вернуть вложение отправителю"),
                                            (
                                                Action::ConfirmReturn,
                                                "Подтвердить возврат без оплаты",
                                            ),
                                            (Action::CancelReturn, "Отмена возврата"),
                                        ] {
                                            reader.spawn(button(action)).with_children(|b| {
                                                b.spawn(text(title, &font, 14., GOLD));
                                            });
                                        }
                                        reader.spawn(text("", &font, 11., INK));
                                        reader.spawn(row()).with_children(|actions| {
                                            for (action, title) in [
                                                (Action::Reply, "Ответить"),
                                                (Action::Delete, "Удалить"),
                                                (Action::ConfirmDelete, "Удалить мою копию"),
                                                (Action::CancelDelete, "Отмена"),
                                            ] {
                                                actions.spawn(button(action)).with_children(|b| {
                                                    b.spawn(text(title, &font, 13., GOLD));
                                                });
                                            }
                                        });
                                    });
                                right
                                    .spawn((
                                        Section::Composer,
                                        Node {
                                            display: Display::None,
                                            width: percent(100),
                                            flex_direction: FlexDirection::Column,
                                            row_gap: px(8),
                                            min_height: px(0),
                                            ..default()
                                        },
                                    ))
                                    .with_children(|editor| {
                                        for (index, title) in [
                                            (0, "Получатель"),
                                            (1, "Тема"),
                                            (2, "Письмо"),
                                            (3, "Цена вложения · 0 — подарок"),
                                        ] {
                                            editor.spawn(text(title, &font, 12., GOLD));
                                            let mut field = editor.spawn((
                                                Button,
                                                Action::Field(index),
                                                Node {
                                                    width: percent(100),
                                                    min_height: px(if index == 2 {
                                                        64
                                                    } else {
                                                        36
                                                    }),
                                                    max_height: if index == 1 {
                                                        px(90)
                                                    } else {
                                                        Val::Auto
                                                    },
                                                    flex_grow: if index == 2 { 1. } else { 0. },
                                                    flex_basis: if index == 2 {
                                                        px(0)
                                                    } else {
                                                        Val::Auto
                                                    },
                                                    flex_shrink: 0.,
                                                    overflow: Overflow::scroll_y(),
                                                    padding: UiRect::all(px(8)),
                                                    border: UiRect::all(px(1)),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::srgb(0.025, 0.035, 0.045)),
                                                BorderColor::all(EDGE),
                                                ScrollPosition::default(),
                                            ));
                                            if index == 2 {
                                                field.insert(Scroller(true));
                                            }
                                            field.with_children(|b| {
                                                b.spawn((
                                                    Label::Field(index),
                                                    text("", &font, 15., INK),
                                                    Node {
                                                        width: percent(100),
                                                        flex_shrink: 0.,
                                                        ..default()
                                                    },
                                                ));
                                            });
                                        }
                                        editor
                                            .spawn(Node {
                                                column_gap: px(8),
                                                align_items: AlignItems::Center,
                                                ..default()
                                            })
                                            .with_children(|row| {
                                                row.spawn((
                                                    Picture::Draft,
                                                    ImageNode::default(),
                                                    Node {
                                                        width: px(32),
                                                        height: px(32),
                                                        ..default()
                                                    },
                                                ));
                                                row.spawn((
                                                    Label::Attachment,
                                                    text("", &font, 12., GOLD),
                                                ));
                                            });
                                        editor.spawn(row()).with_children(|row| {
                                            for (action, title) in [
                                                (Action::Choose, "Вложить вещь"),
                                                (Action::Detach, "Убрать"),
                                                (Action::Less, "−"),
                                                (Action::More, "+"),
                                                (Action::All, "Всё"),
                                            ] {
                                                row.spawn(button(action)).with_children(|b| {
                                                    b.spawn(text(title, &font, 12., GOLD));
                                                });
                                            }
                                        });
                                        editor.spawn((Label::Counter, text("", &font, 11., INK)));
                                        editor.spawn(button(Action::Send)).with_children(|b| {
                                            b.spawn(text(
                                                "Отправить · Ctrl+Enter",
                                                &font,
                                                14.,
                                                GOLD,
                                            ));
                                        });
                                    });
                            });
                    });
                panel.spawn((Label::Status, text("", &font, 13., GOLD)));
            });
        });
}

/// HUD button uses the same component as the mail window controls.
pub fn badge(font: &Handle<Font>) -> impl Bundle {
    (Label::Badge, text("Почта  [M]", font, 13., GOLD))
}

fn request_page(network: &mut Network, folder: Folder, page: u32) {
    network.mail.refresh = false;
    network.mail_request(Command::List { folder, page });
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    mut form: ResMut<Form>,
    mut window: ResMut<CharacterWindow>,
    mut network: ResMut<Network>,
    mut movement: ResMut<Movement>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut keyboard: MessageReader<KeyboardInput>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut services: MessageReader<crate::npcs::ServiceRequest>,
    world: Res<crate::Terrain>,
) {
    let at_post = services
        .read()
        .copied()
        .any(|r| r.service == crate::npcs::Service::Mail && r.valid(&network, &world.0));
    let owner = network.character.as_ref().map(|c| c.id);
    if form.owner != owner || network.mode != Mode::Online {
        *form = Form { owner, ..default() };
        if window.mail {
            window.open = false;
            window.mail = false;
        }
    }
    if network.mode != Mode::Online {
        keyboard.clear();
        return;
    }
    let pressed = buttons
        .iter()
        .find(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, a)| *a);
    let opened = at_post
        || (!window.mail || !window.open)
            && (keys.just_pressed(KeyCode::KeyM) || matches!(pressed, Some(Action::Open)));
    if opened {
        form.payment_confirmation = None;
        form.return_confirmation = None;
        *window = CharacterWindow {
            open: true,
            mail: true,
            ..default()
        };
        *movement = Movement::default();
        form.delete_confirmation = false;
        if !network.mail.pending {
            request_page(&mut network, Folder::Inbox, 0);
        }
    }
    if !window.open || !window.mail {
        keyboard.clear();
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        window.open = false;
        window.mail = false;
        // Escape closes this window only; the world must not also interpret it as exit.
        keys.clear();
        keyboard.clear();
        return;
    }
    if let Some(draft) = network.mail.sent_draft.take() {
        if draft == form.draft {
            form.values = default();
            form.draft.clear();
            form.attachment = None;
            form.choosing = false;
            form.compose = false;
            form.caret = 0;
        }
        network.mail.letter = None;
        request_page(&mut network, Folder::Sent, 0);
    }
    if !network.mail.pending {
        if let Some(action) = pressed {
            match action {
                Action::Folder(folder) => {
                    form.compose = false;
                    form.choosing = false;
                    form.delete_confirmation = false;
                    network.mail.letter = None;
                    request_page(&mut network, folder, 0);
                }
                Action::New => {
                    form.compose = true;
                    form.delete_confirmation = false;
                    if form.draft.is_empty() {
                        form.new_draft();
                    }
                    form.focus(0);
                    network.mail.status.clear();
                }
                Action::Reply if network.mail.letter.is_some() => {
                    let letter = network.mail.letter.as_ref().unwrap();
                    form.values = [
                        letter.header.sender.clone(),
                        short(
                            &format!("Ответ: {}", letter.header.subject),
                            mail::SUBJECT_LIMIT,
                        ),
                        String::new(),
                        String::new(),
                    ];
                    form.new_draft();
                    form.attachment = None;
                    form.choosing = false;
                    form.compose = true;
                    form.focus(2);
                    form.delete_confirmation = false;
                    form.scroll_reset = true;
                    network.mail.status.clear();
                }
                Action::Choose if form.compose => {
                    form.choosing = !form.choosing;
                    form.item_page = 0;
                }
                Action::Detach if form.compose => {
                    form.attachment = None;
                    form.choosing = false;
                }
                Action::Less | Action::More | Action::All if form.compose => {
                    if let Some(mut a) = form.attachment
                        && let Some(c) = network.character.as_ref()
                    {
                        let k = mortown_protocol::layout::kind(a.token).unwrap_or(0);
                        let max = if mortown_protocol::items::equippable(k) {
                            1
                        } else {
                            mortown_protocol::layout::quantity(c, k)
                                .min(mortown_protocol::transfer::MAX_AMOUNT)
                        };
                        a.quantity = match action {
                            Action::Less => a.quantity.saturating_sub(1).max(1),
                            Action::More => a.quantity.saturating_add(1).min(max),
                            _ => max,
                        };
                        form.attachment = Some(a);
                    }
                }
                Action::Return => {
                    form.return_confirmation = network.mail.letter.as_ref().map(|l| l.header.id);
                    form.payment_confirmation = None;
                }
                Action::CancelReturn => form.return_confirmation = None,
                Action::ConfirmReturn => {
                    if let Some(id) = form.return_confirmation.take()
                        && network
                            .mail
                            .letter
                            .as_ref()
                            .is_some_and(|l| l.header.id == id)
                    {
                        network.mail_request(Command::Return { id });
                    }
                }
                Action::Claim => {
                    form.return_confirmation = None;
                    if let Some(letter) = network.mail.letter.as_ref()
                        && let Some(a) = &letter.header.attachment
                    {
                        let (id, price) = (letter.header.id, a.price);
                        if price == 0 {
                            network.mail_request(Command::Claim {
                                id,
                                expected_price: 0,
                            });
                        } else {
                            form.payment_confirmation = Some((id, price));
                        }
                    }
                }
                Action::CancelPayment => form.payment_confirmation = None,
                Action::ConfirmPayment => {
                    if let Some((id, price)) = form.payment_confirmation.take()
                        && network.mail.letter.as_ref().is_some_and(|l| {
                            l.header.id == id
                                && l.header
                                    .attachment
                                    .as_ref()
                                    .is_some_and(|a| !a.claimed && !a.returned && a.price == price)
                        })
                    {
                        network.mail_request(Command::Claim {
                            id,
                            expected_price: price,
                        });
                    }
                }
                Action::Previous | Action::Next if form.choosing => {
                    let count = network
                        .character
                        .as_ref()
                        .map_or(0, |c| mortown_protocol::transfer::eligible(c).len());
                    form.item_page = if matches!(action, Action::Previous) {
                        form.item_page.saturating_sub(1)
                    } else {
                        (form.item_page + 1).min(count.saturating_sub(1) / mail::PAGE_SIZE)
                    };
                }
                Action::Row(index) if form.choosing => {
                    if let Some(c) = network.character.as_ref()
                        && let Some(token) = mortown_protocol::transfer::eligible(c)
                            .get(form.item_page * mail::PAGE_SIZE + index)
                    {
                        form.attachment = Some(mail::AttachmentRequest {
                            token: *token,
                            quantity: 1,
                            price: 0,
                        });
                        form.choosing = false;
                    }
                }
                Action::Refresh | Action::Previous | Action::Next => {
                    let (folder, page, total) = network
                        .mail
                        .page
                        .as_ref()
                        .map_or((Folder::Inbox, 0, 0), |p| (p.folder, p.page, p.total));
                    let page = match action {
                        Action::Previous => page.saturating_sub(1),
                        Action::Next => page
                            .saturating_add(1)
                            .min((total.saturating_sub(1) / mail::PAGE_SIZE as u64) as u32),
                        _ => page,
                    };
                    request_page(&mut network, folder, page);
                }
                Action::Row(index) => {
                    if let Some(id) = network
                        .mail
                        .page
                        .as_ref()
                        .and_then(|p| p.entries.get(index))
                        .map(|e| e.id)
                    {
                        form.compose = false;
                        form.payment_confirmation = None;
                        form.return_confirmation = None;
                        form.delete_confirmation = false;
                        form.scroll_reset = true;
                        network.mail.letter = None;
                        network.mail_request(Command::Read { id });
                    }
                }
                Action::Field(index) if form.compose => form.focus(index),
                Action::Delete => form.delete_confirmation = true,
                Action::CancelDelete => form.delete_confirmation = false,
                Action::ConfirmDelete => {
                    if let Some(id) = network.mail.letter.as_ref().map(|l| l.header.id) {
                        network.mail_request(Command::Delete { id });
                    }
                    form.delete_confirmation = false;
                }
                Action::Send if form.compose => send(&mut form, &mut network),
                _ => {}
            }
        }
        if network.mail.refresh && !network.mail.pending && !form.compose {
            let (folder, page) = network
                .mail
                .page
                .as_ref()
                .map_or((Folder::Inbox, 0), |p| (p.folder, p.page));
            request_page(&mut network, folder, page);
        }
    }
    // The opening M belongs to the shortcut, not to the first field of a draft.
    if opened {
        form.payment_confirmation = None;
        form.return_confirmation = None;
        keyboard.clear();
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    for event in keyboard.read() {
        if !form.compose || network.mail.pending || event.state != ButtonState::Pressed {
            continue;
        }
        let index = form.focus;
        let value = &form.values[index];
        let caret = form.caret.min(value.len());
        match event.key_code {
            KeyCode::Tab => {
                form.focus((index + if shift { 3 } else { 1 }) % 4);
            }
            KeyCode::Enter | KeyCode::NumpadEnter if ctrl => {
                send(&mut form, &mut network);
            }
            KeyCode::Enter | KeyCode::NumpadEnter if index != 2 => form.focus((index + 1) % 4),
            KeyCode::Enter | KeyCode::NumpadEnter => insert(&mut form, "\n"),
            KeyCode::ArrowLeft => {
                form.caret = previous(value, caret);
            }
            KeyCode::ArrowRight => {
                form.caret = next(value, caret);
            }
            KeyCode::Home => {
                form.caret = if ctrl {
                    0
                } else {
                    value[..caret].rfind('\n').map_or(0, |p| p + 1)
                };
            }
            KeyCode::End => {
                form.caret = if ctrl {
                    value.len()
                } else {
                    value[caret..].find('\n').map_or(value.len(), |p| caret + p)
                };
            }
            KeyCode::Backspace => {
                let start = previous(value, caret);
                form.values[index].replace_range(start..caret, "");
                form.caret = start;
            }
            KeyCode::Delete => {
                let end = next(value, caret);
                form.values[index].replace_range(caret..end, "");
            }
            _ if !ctrl => {
                if let Some(text) = &event.text {
                    insert(&mut form, text);
                }
            }
            _ => {}
        }
    }
}

fn previous(text: &str, caret: usize) -> usize {
    text[..caret]
        .char_indices()
        .next_back()
        .map_or(0, |(n, _)| n)
}
fn next(text: &str, caret: usize) -> usize {
    text[caret..]
        .chars()
        .next()
        .map_or(caret, |c| caret + c.len_utf8())
}
fn insert(form: &mut Form, text: &str) {
    let index = form.focus;
    let limit = [16, mail::SUBJECT_LIMIT, mail::BODY_LIMIT, 7][index];
    let remaining = limit.saturating_sub(form.values[index].chars().count());
    let added: String = text
        .chars()
        .filter(|c| {
            if index == 3 {
                c.is_ascii_digit()
            } else {
                !c.is_control() || (index == 2 && *c == '\n')
            }
        })
        .take(remaining)
        .collect();
    form.values[index].insert_str(form.caret, &added);
    form.caret += added.len();
    form.follow_end = index == 2 && form.caret == form.values[index].len();
}
fn send(form: &mut Form, network: &mut Network) {
    if let Err(message) = mail::validate(
        form.values[0].trim(),
        form.values[1].trim(),
        form.values[2].trim(),
    ) {
        network.mail.status = message.into();
        return;
    }
    if let Some(mut a) = form.attachment {
        let Ok(price) = (if form.values[3].is_empty() {
            "0"
        } else {
            &form.values[3]
        })
        .parse::<u32>() else {
            network.mail.status = "Введите цену в монетах".into();
            return;
        };
        if price > mail::MAX_PRICE {
            network.mail.status = "Максимальная цена — 1 000 000 монет".into();
            return;
        }
        a.price = price;
        form.attachment = Some(a);
        let valid = network.character.as_ref().is_some_and(|c| {
            mortown_protocol::transfer::eligible(c).contains(&a.token)
                && mortown_protocol::layout::kind(a.token).is_some_and(|k| {
                    a.quantity > 0 && a.quantity <= mortown_protocol::layout::quantity(c, k)
                })
        });
        if !valid {
            network.mail.status = "Вложение изменилось. Выберите вещь из рюкзака заново".into();
            return;
        }
    }
    if form.draft.is_empty() {
        form.new_draft();
    }
    network.mail_request(Command::Send {
        recipient: form.values[0].trim().into(),
        subject: form.values[1].trim().into(),
        body: form.values[2].trim().into(),
        draft: form.draft.clone(),
        attachment: form.attachment,
    });
}
fn short(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.into()
    } else {
        value
            .chars()
            .take(limit.saturating_sub(1))
            .chain(std::iter::once('…'))
            .collect()
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn display(
    mut form: ResMut<Form>,
    window: Res<CharacterWindow>,
    network: Res<Network>,
    mut nodes: Query<
        (
            Option<&Section>,
            Option<&Action>,
            &mut Node,
            Option<&mut BorderColor>,
        ),
        Without<Picture>,
    >,
    mut labels: Query<(&Label, &mut Text)>,
    mut scrolls: Query<(&Scroller, &ComputedNode, &Interaction, &mut ScrollPosition)>,
    wheel: Res<AccumulatedMouseScroll>,
    icons: Res<crate::actionbar::Icons>,
    mut pictures: Query<(&Picture, &mut ImageNode, &mut Node)>,
) {
    let visible = window.open && window.mail && network.mode == Mode::Online;
    let box_ = &network.mail;
    let page = box_.page.as_ref();
    let letter = box_.letter.as_ref();
    let items = network
        .character
        .as_ref()
        .map_or_else(Vec::new, mortown_protocol::transfer::eligible);
    let attachment_kind = form
        .attachment
        .and_then(|a| mortown_protocol::layout::kind(a.token));
    let can_count = attachment_kind.is_some_and(|k| !mortown_protocol::items::equippable(k));
    let can_reply = letter.is_some_and(|l| {
        network
            .character
            .as_ref()
            .is_some_and(|c| c.nickname != l.header.sender)
    });
    let unclaimed = can_reply
        && letter.is_some_and(|l| {
            l.header
                .attachment
                .as_ref()
                .is_some_and(|a| !a.claimed && !a.returned)
        });
    for (picture, mut image, mut node) in &mut pictures {
        let kind = match picture {
            Picture::Draft => attachment_kind,
            Picture::Received => letter
                .and_then(|l| l.header.attachment.as_ref())
                .map(|a| a.kind),
            Picture::Row(i) => {
                if form.choosing {
                    items
                        .get(form.item_page * mail::PAGE_SIZE + i)
                        .and_then(|t| mortown_protocol::layout::kind(*t))
                } else {
                    page.and_then(|p| p.entries.get(*i))
                        .and_then(|e| e.attachment.as_ref())
                        .map(|a| a.kind)
                }
            }
        };
        node.display = if kind.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        image.image = icons.0[crate::actionbar::item_icon(kind.unwrap_or(0))].clone();
    }
    for (section, action, mut node, border) in &mut nodes {
        let show = if let Some(section) = section {
            match section {
                Section::Overlay => visible,
                Section::Reader => !form.compose,
                Section::Composer => form.compose,
                Section::Received => letter.is_some_and(|l| l.header.attachment.is_some()),
            }
        } else if let Some(action) = action {
            match action {
                Action::Claim => {
                    unclaimed
                        && !letter.is_some_and(|l| {
                            form.payment_confirmation
                                .is_some_and(|(id, _)| id == l.header.id)
                        })
                }
                Action::ConfirmPayment | Action::CancelPayment => {
                    unclaimed
                        && letter.is_some_and(|l| {
                            form.payment_confirmation.is_some_and(|(id, price)| {
                                id == l.header.id
                                    && l.header
                                        .attachment
                                        .as_ref()
                                        .is_some_and(|a| a.price == price)
                            })
                        })
                }
                Action::Return => letter.is_some_and(|l| {
                    l.header.return_of.is_none()
                        && l.header
                            .attachment
                            .as_ref()
                            .is_some_and(|a| !a.claimed && !a.returned)
                        && form.return_confirmation != Some(l.header.id)
                }),
                Action::ConfirmReturn | Action::CancelReturn => letter.is_some_and(|l| {
                    form.return_confirmation == Some(l.header.id)
                        && l.header.return_of.is_none()
                        && l.header
                            .attachment
                            .as_ref()
                            .is_some_and(|a| !a.claimed && !a.returned)
                }),
                Action::Detach => form.attachment.is_some(),
                Action::Less | Action::More | Action::All => can_count,
                Action::Row(index) if form.choosing => items
                    .get(form.item_page * mail::PAGE_SIZE + index)
                    .is_some(),
                Action::Previous if form.choosing => form.item_page > 0,
                Action::Next if form.choosing => {
                    (form.item_page + 1) * mail::PAGE_SIZE < items.len()
                }
                Action::Row(index) => page.is_some_and(|p| p.entries.get(*index).is_some()),
                Action::Previous => page.is_some_and(|p| p.page > 0),
                Action::Next => page
                    .is_some_and(|p| (u64::from(p.page) + 1) * (mail::PAGE_SIZE as u64) < p.total),
                Action::Reply => can_reply && !form.delete_confirmation,
                Action::Delete => letter.is_some() && !form.delete_confirmation && !unclaimed,
                Action::ConfirmDelete | Action::CancelDelete => {
                    letter.is_some() && form.delete_confirmation && !unclaimed
                }
                _ => true,
            }
        } else {
            continue;
        };
        node.display = if show { Display::Flex } else { Display::None };
        if let (Some(action), Some(mut border)) = (action, border) {
            let selected = match action {
                Action::Field(index) => form.compose && form.focus == *index,
                Action::Folder(folder) => page.is_some_and(|p| p.folder == *folder),
                Action::Row(index) => page
                    .and_then(|p| p.entries.get(*index))
                    .is_some_and(|e| letter.is_some_and(|l| l.header.id == e.id)),
                _ => false,
            };
            *border = BorderColor::all(if selected { GOLD } else { EDGE });
        }
    }
    for (label, mut text) in &mut labels {
        let content = match label {
            Label::Badge => if box_.unread > 0 { format!("Почта  [M]  ·  +{}", box_.unread) } else { "Почта  [M]".into() },
            Label::Attachment=>form.attachment.and_then(|a|mortown_protocol::layout::kind(a.token).map(|k|(a,k))).map_or("Без вложения".into(),|(a,k)|format!("{} ×{}",mortown_protocol::items::definition(k).unwrap().name,a.quantity)),
            Label::Received => letter.and_then(|l|l.header.attachment.as_ref()).map_or(String::new(), |a| {
                let status = if a.returned { "Возвращено отправителю" } else if a.claimed { "Получено" } else if can_reply { "К получению" } else { "Ожидает получения" };
                let price = if a.price == 0 { "Бесплатно".into() } else { format!("Цена за всё: {} монет", a.price) };
                let balance = network.character.as_ref().and_then(|c| c.stats.get("coins")).copied().unwrap_or(0.);
                format!("{} ×{} · {}\n{} · Ваши монеты: {:.0}", mortown_protocol::items::definition(a.kind).map_or("Предмет", |d| d.name), a.quantity, status, price, balance)
            }),
            Label::Row(index) if form.choosing=>items.get(form.item_page*mail::PAGE_SIZE+index).and_then(|t|mortown_protocol::layout::kind(*t).map(|k|(*t,k))).map_or(String::new(),|(t,k)|{
                let count=if mortown_protocol::items::equippable(k){1}else{network.character.as_ref().map_or(0,|c|mortown_protocol::layout::quantity(c,k))};
                format!("{} ×{}{}",short(mortown_protocol::items::definition(k).unwrap().name,25),count,if mortown_protocol::items::equippable(k){format!(" · #{}",t% (1u64<<32)+1)}else{String::new()})
            }),
            Label::Page if form.choosing=>if items.is_empty(){"В рюкзаке нет вещей для отправки".into()}else{format!("Рюкзак · страница {} / {}",form.item_page+1,items.len().div_ceil(mail::PAGE_SIZE))},
            Label::Row(index) => page.and_then(|p| p.entries.get(*index).map(|e| (p.folder, e))).map_or(String::new(), |(folder, e)| {
                let peer = if folder == Folder::Inbox { &e.sender } else { &e.recipient };
                format!("{}{} · {}\n{}", if !e.read && folder == Folder::Inbox { "● " } else { "" }, peer, e.sent_at.split_whitespace().next().unwrap_or(""), short(&e.subject, 34))
            }),
            Label::Page => page.map_or("Загружаем почту…".into(), |p| {
                if p.total == 0 { "В этой папке пока нет писем".into() }
                else { format!("Страница {} / {} · писем {}", p.page + 1, p.total.div_ceil(mail::PAGE_SIZE as u64), p.total) }
            }),
            Label::Reader => letter.map_or("Выберите письмо слева\n\nНажмите «Написать», чтобы отправить новое письмо персонажу по имени.".into(), |l| {
                format!("{}\n\nОт: {}\nКому: {}\n{} UTC\n\n{}", l.header.subject, l.header.sender, l.header.recipient, l.header.sent_at, l.body)
            }),
            Label::Field(index) => {
                let mut value = form.values[*index].clone();
                if form.compose && form.focus == *index { value.insert(form.caret.min(value.len()), '│'); }
                if value.is_empty() { " ".into() } else { value }
            }
            Label::Counter => format!("Тема: {}/128 · Письмо: {}/1024\nTab — следующее поле · Enter — новая строка · колёсико — прокрутка", form.values[1].chars().count(), form.values[2].chars().count()),
            Label::Status => if box_.status.is_empty() { format!("Непрочитанных: {} · Доставка доступна и вне игры", box_.unread) } else { box_.status.clone() },
        };
        if text.0 != content {
            text.0 = content;
        }
    }
    if visible {
        for (scroller, computed, interaction, mut position) in &mut scrolls {
            if form.scroll_reset {
                position.y = 0.;
            }
            if scroller.0 != form.compose {
                continue;
            }
            if form.follow_end && scroller.0 {
                position.y = 1_000_000.;
            } else if *interaction != Interaction::None && wheel.delta.y != 0. {
                let unit = if wheel.unit == MouseScrollUnit::Line {
                    28.
                } else {
                    1.
                };
                let max = ((computed.content_size().y - computed.size().y)
                    * computed.inverse_scale_factor())
                .max(0.);
                position.y = (position.y - wheel.delta.y * unit).clamp(0., max);
            }
        }
        form.scroll_reset = false;
        form.follow_end = false;
    }
}
