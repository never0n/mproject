//! Admin controls compose authenticated server commands; no local world mutations.
//! Opened by the "//admin" command; the server sends a `View` (role, online list, audit).
//! Buttons fill a "//command {id} {value}" template and send it as chat `Command::Admin`.
//! Role checks here only hide buttons; the server re-checks every command.
use super::*;
use mortown_protocol::admin::{Target, View};
/// Admin panel input state. Any change bumps `revision`, which makes `windows::input`
/// rebuild the dialog.
#[derive(Resource)]
pub struct Board {
    /// 0 players, 2 mobs, 3 items, 4 skills, 5 moderation, 6 world, 7 roles, 8 audit.
    pub tab: u8,
    /// List page (10 entries per page).
    pub page: usize,
    pub selected: Option<Target>,
    /// Text inputs: 0 → `{value}` (id / duration / role), 1 → `{amount}`, 2 → `{reason}` / search.
    pub fields: [String; 3],
    pub focused: Option<usize>,
    pub revision: u64,
    /// `revision` the open dialog was last built from.
    pub rendered: u64,
    pub open: bool,
}
impl Default for Board {
    fn default() -> Self {
        Self {
            tab: 0,
            page: 0,
            selected: None,
            fields: ["1".into(), "1".into(), String::new()],
            focused: None,
            revision: 0,
            rendered: 0,
            open: false,
        }
    }
}
#[derive(Component)]
pub enum Control {
    Tab(u8),
    Select(Target),
    Page(i32),
    Field(usize),
    /// Command template with `{id}` `{value}` `{amount}` `{reason}` placeholders.
    Run(String),
    /// Quick-pick that writes into field 0 (`{value}`).
    Value(String),
    /// Use the current combat target as the admin target.
    UseTarget,
}
fn button(p: &mut ChildSpawnerCommands, icons: &Icons, name: impl Into<String>, action: Control) {
    p.spawn((
        Button,
        action,
        crate::skin::OwnStyle,
        Node {
            padding: UiRect::axes(px(9), px(6)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(crate::ui::BUTTON),
        BorderColor::all(crate::ui::EDGE),
    ))
    .with_children(|b| {
        b.spawn(label(name, &icons.1, 13.));
    });
}
fn row(p: &mut ChildSpawnerCommands, build: impl FnOnce(&mut ChildSpawnerCommands)) {
    p.spawn(Node {
        flex_wrap: FlexWrap::Wrap,
        column_gap: px(6),
        row_gap: px(6),
        flex_shrink: 0.,
        ..default()
    })
    .with_children(build);
}
/// Command button shown only if the viewer's `role` is at least `min`.
fn action(
    p: &mut ChildSpawnerCommands,
    icons: &Icons,
    role: i16,
    min: i16,
    name: impl Into<String>,
    command: &str,
) {
    if role >= min {
        button(p, icons, name, Control::Run(command.into()));
    }
}
/// Builds the admin dialog body; called by `windows::input` on every rebuild.
pub fn content(p: &mut ChildSpawnerCommands, icons: &Icons, view: &View, b: &Board) {
    p.spawn(label(
        format!(
            "Онлайн: {} · {} · данные обновляются кнопкой",
            view.online,
            mortown_protocol::admin::role_name(view.role)
        ),
        &icons.1,
        13.,
    ));
    if view.role >= 3 {
        row(p, |r| {
            let state = |on| if on { "вкл" } else { "выкл" };
            action(
                r,
                icons,
                view.role,
                3,
                format!("Бессмертие: {}", state(view.powers.immortal)),
                "//immortal",
            );
            action(
                r,
                icons,
                view.role,
                3,
                format!("Невидимость: {}", state(view.powers.invisible)),
                "//invisible",
            );
            action(
                r,
                icons,
                view.role,
                3,
                format!("Телепорт по клику: {}", state(view.powers.click_teleport)),
                "//clicktp",
            );
        });
        p.spawn(label(
            "Режимы действуют только на вас, пока вы в этом мире. Телепорт заменяет бег по маршруту клика; WASD остаётся. Закройте окно и кликните по земле.",
            &icons.1,
            12.,
        ));
    }
    // (tab, title, minimum role). Keep in sync with `minimum` in `input`.
    row(p, |r| {
        for (id, name, min) in [
            (0, "Игроки", 2),
            (2, "Мобы", 3),
            (3, "Предметы", 4),
            (4, "Навыки", 4),
            (5, "Модерация", 2),
            (6, "Мир", 3),
            (7, "Роли", 5),
            (8, "Журнал", 2),
        ] {
            if view.role >= min {
                button(
                    r,
                    icons,
                    if b.tab == id {
                        format!("• {name}")
                    } else {
                        name.into()
                    },
                    Control::Tab(id),
                );
            }
        }
    });
    let selected = b
        .selected
        .and_then(|t| view.entries.iter().find(|e| e.target == t));
    p.spawn(label(
        selected.map_or_else(
            || format!("Цель: {:?}", b.selected),
            |e| {
                format!(
                    "Цель: {} · {:?} · уровень {} · HP {:.0} · {}",
                    e.name,
                    e.target,
                    e.level,
                    e.hp,
                    if e.position.is_some() {
                        "в мире"
                    } else {
                        "офлайн"
                    }
                )
            },
        ),
        &icons.1,
        14.,
    ));
    row(p, |r| {
        button(r, icons, "Цель из мира", Control::UseTarget);
        button(r, icons, "Обновить", Control::Run("//admin".into()));
    });
    if b.tab == 0 {
        row(p, |r| {
            button(
                r,
                icons,
                format!(
                    "Поиск: {}{}",
                    b.fields[2],
                    if b.focused == Some(2) { "│" } else { "" }
                ),
                Control::Field(2),
            );
            button(
                r,
                icons,
                "Найти ник / ID",
                Control::Run("//admin {reason}".into()),
            );
        });
    }
    if b.tab <= 2 {
        let list: Vec<_> = view
            .entries
            .iter()
            .filter(|e| {
                matches!(
                    (b.tab, e.target),
                    (0, Target::Player(_)) | (2, Target::Mob(_))
                )
            })
            .collect();
        for e in list.iter().skip(b.page * 10).take(10) {
            button(
                p,
                icons,
                format!(
                    "{} · {:?} · ур. {} · {}{}",
                    e.name,
                    e.target,
                    e.level,
                    if e.position.is_some() {
                        "онлайн"
                    } else {
                        "офлайн"
                    },
                    if e.role > 0 {
                        format!(" · {}", mortown_protocol::admin::role_name(e.role))
                    } else {
                        String::new()
                    }
                ),
                Control::Select(e.target),
            );
        }
        row(p, |r| {
            if b.page > 0 {
                button(r, icons, "←", Control::Page(-1));
            }
            if list.len() > (b.page + 1) * 10 {
                button(r, icons, "→", Control::Page(1));
            }
        });
        row(p, |r| {
            for (name, cmd) in [
                ("К цели", "//goto"),
                ("Призвать игрока", "//bring"),
                ("Убить", "//kill"),
                ("Восстановить", "//revive"),
                ("Убрать из мира…", "//del"),
            ] {
                action(r, icons, view.role, 3, name, cmd);
            }
        });
    }
    if matches!(b.tab, 3..=7) {
        row(p, |r| {
            button(
                r,
                icons,
                format!(
                    "ID / срок / роль: {}{}",
                    b.fields[0],
                    if b.focused == Some(0) { "│" } else { "" }
                ),
                Control::Field(0),
            );
            button(
                r,
                icons,
                format!(
                    "Количество: {}{}",
                    b.fields[1],
                    if b.focused == Some(1) { "│" } else { "" }
                ),
                Control::Field(1),
            );
        });
        if matches!(b.tab, 5 | 6) {
            button(
                p,
                icons,
                format!(
                    "Причина / объявление: {}{}",
                    b.fields[2],
                    if b.focused == Some(2) { "│" } else { "" }
                ),
                Control::Field(2),
            );
        }
    }
    match b.tab {
        3 => {
            row(p, |r| {
                action(
                    r,
                    icons,
                    view.role,
                    4,
                    "Выдать",
                    "//give {id} {value} {amount}",
                );
                action(
                    r,
                    icons,
                    view.role,
                    4,
                    "Забрать количество",
                    "//takeaway {id} {value} {amount}",
                );
                action(
                    r,
                    icons,
                    view.role,
                    4,
                    "Забрать весь тип",
                    "//takeaway {id} {value}",
                );
                action(
                    r,
                    icons,
                    view.role,
                    4,
                    "Очистить инвентарь…",
                    "//clearbag {id}",
                );
            });
            // Item kind ids probed up to 26; raise this when the catalogue grows.
            for id in 1..=26 {
                if let Some(item) = mortown_protocol::items::definition(id) {
                    button(
                        p,
                        icons,
                        format!("{id}: {}", item.name),
                        Control::Value(id.to_string()),
                    );
                }
            }
        }
        4 => {
            row(p, |r| {
                action(r, icons, view.role, 4, "Изучить", "//learns {id} {value}");
                action(r, icons, view.role, 4, "Забыть", "//unlearns {id} {value}");
            });
            for (id, name) in [
                (1, "Первый навык"),
                (2, "Второй навык"),
                (3, "Навык 3"),
                (4, "Навык 4"),
                (5, "Навык 5"),
            ] {
                button(
                    p,
                    icons,
                    format!("{id}: {name}"),
                    Control::Value(id.to_string()),
                );
            }
        }
        5 => {
            row(p, |r| {
                for (name, cmd, min) in [
                    ("Мут (сек.)", "//mute {id} {value}", 2),
                    ("Снять мут", "//unmute {id}", 2),
                    ("Общий мут (сек.)", "//muteg {id} {value}", 2),
                    ("Снять общий мут", "//unmuteg {id}", 2),
                    ("Бан (дни)", "//ban {id} {value} {reason}", 3),
                    ("Разбан", "//unban {id} {reason}", 3),
                    ("Отключить", "//kick {id}", 3),
                    ("Блок IP игрока", "//banip {id}", 4),
                    ("Разбан IP (поле ID)", "//unbanip {value}", 4),
                    ("Удалить персонажа из БД…", "//erase {id}", 4),
                ] {
                    action(r, icons, view.role, min, name, cmd);
                }
            });
        }
        6 => {
            row(p, |r| {
                action(
                    r,
                    icons,
                    view.role,
                    3,
                    "Создать на моём месте",
                    "//spawn {value}",
                );
                action(r, icons, view.role, 3, "Объявление", "//announce {reason}");
                action(r, icons, view.role, 4, "Восстановить мир…", "//resetworld");
            });
            // Spawn ids: 1–3 mob kinds (server uses the same scheme).
            for (id, name) in [(1, "Разбойник"), (2, "Мародёр"), (3, "Отступник")]
            {
                button(
                    p,
                    icons,
                    format!("{id} · моб · {name}"),
                    Control::Value(id.to_string()),
                );
            }
            p.spawn(label("Созданные объекты и удаления действуют до перезапуска сервера. Убийство моба сохраняет его обычное возрождение; удаление убирает его полностью.",&icons.1,12.));
        }
        7 => {
            for role in 1..=5 {
                button(
                    p,
                    icons,
                    format!("{role}: {}", mortown_protocol::admin::role_name(role)),
                    Control::Value(role.to_string()),
                );
            }
            action(
                p,
                icons,
                view.role,
                5,
                "Назначить выбранному аккаунту",
                "//role {id} {value}",
            );
        }
        8 => {
            for line in &view.audit {
                p.spawn((
                    label(line, &icons.1, 12.),
                    Node {
                        flex_shrink: 0.,
                        ..default()
                    },
                ));
            }
        }
        _ => {}
    }
}
/// Current combat selection as an admin target; priority player > mob.
pub fn target(ui: &crate::combat::CombatUi) -> Option<Target> {
    ui.player
        .map(Target::Player)
        .or_else(|| ui.selected.map(Target::Mob))
}
/// First of the chat input systems; types into the focused field and runs commands.
pub fn input(
    mut board: ResMut<Board>,
    mut network: ResMut<Network>,
    ui: Res<crate::combat::CombatUi>,
    buttons: Query<(&Control, &Interaction), Changed<Interaction>>,
    mut events: MessageReader<KeyboardInput>,
) {
    let open = network
        .chat
        .dialog
        .as_ref()
        .is_some_and(|d| d.kind == mortown_protocol::chat::DialogKind::Admin);
    if !open {
        board.open = false;
        events.clear();
        return;
    }
    let role = network
        .chat
        .dialog
        .as_ref()
        .and_then(|d| d.admin.as_ref())
        .map_or(1, |v| v.role);
    let minimum = match board.tab {
        2 | 6 => 3,
        3 | 4 => 4,
        7 => 5,
        _ => 2,
    };
    // Role dropped (or tab not allowed): fall back to the players tab.
    if role < minimum {
        board.tab = 0;
        board.page = 0;
        board.revision += 1;
    }
    // Just opened: preselect whatever is targeted in the world.
    if !board.open {
        board.open = true;
        board.selected = target(&ui);
        board.focused = None;
        board.revision += 1;
    }
    if let Some((action, _)) = buttons.iter().find(|(_, i)| **i == Interaction::Pressed) {
        match action {
            Control::Tab(tab) => {
                board.tab = *tab;
                board.page = 0;
                board.focused = None;
            }
            Control::Select(t) => board.selected = Some(*t),
            Control::UseTarget => board.selected = target(&ui),
            Control::Page(delta) => board.page = board.page.saturating_add_signed(*delta as isize),
            Control::Field(id) => {
                board.focused = Some(*id);
                board.fields[*id].clear();
            }
            Control::Value(value) => {
                board.fields[0] = value.clone();
                board.focused = None;
            }
            Control::Run(pattern) if !network.chat.pending => {
                let id = if let Some(Target::Player(id)) = board.selected {
                    id.to_string()
                } else {
                    String::new()
                };
                if pattern.contains("{id}") && id.is_empty() {
                    network.chat.status = "Выберите игрока во вкладке «Игроки»".into();
                    return;
                }
                // `{id}` is only ever a player (character) id; mob targets go in `target`.
                let text = pattern
                    .replace("{id}", &id)
                    .replace("{value}", &board.fields[0])
                    .replace("{amount}", &board.fields[1])
                    .replace("{reason}", &board.fields[2]);
                if let Err(e) = mortown_protocol::chat::commands::parse(&text) {
                    network.chat.status = e.into();
                    return;
                }
                network.chat_request(Command::Admin {
                    text,
                    target: board.selected,
                });
                board.focused = None;
            }
            _ => {}
        }
        board.revision += 1;
    }
    for event in events.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        let Some(i) = board.focused else {
            continue;
        };
        match event.key_code {
            KeyCode::Escape | KeyCode::Enter | KeyCode::NumpadEnter => board.focused = None,
            KeyCode::Backspace => {
                board.fields[i].pop();
            }
            _ => {
                if let Some(text) = &event.text {
                    // Field cap: 256 characters.
                    let remaining = 256usize.saturating_sub(board.fields[i].chars().count());
                    board.fields[i]
                        .extend(text.chars().filter(|c| !c.is_control()).take(remaining));
                }
            }
        }
        board.revision += 1;
    }
}
