//! The command vocabulary from MasterPrompt, adapted to the desktop protocol.
pub struct Spec {
    pub name: &'static str,
    pub syntax: &'static str,
    pub description: &'static str,
    pub role: i16,
    pub min: usize,
    pub max: usize,
}
macro_rules! spec {
    ($n:literal,$s:literal,$d:literal,$r:literal,$min:literal,$max:literal) => {
        Spec {
            name: $n,
            syntax: $s,
            description: $d,
            role: $r,
            min: $min,
            max: $max,
        }
    };
}
pub const SPECS: &[Spec] = &[
    spec!(
        "//clearbag",
        "//clearbag id",
        "Очистить вещи с подтверждением",
        4,
        1,
        1
    ),
    spec!(
        "//admin",
        "//admin [ник/ID]",
        "Открыть управление игрой",
        2,
        0,
        1
    ),
    spec!(
        "//spawn",
        "//spawn ID",
        "Создать моба (1–3) на своём месте",
        3,
        1,
        1
    ),
    spec!(
        "//kill",
        "//kill",
        "Убить выбранного игрока или моба",
        3,
        0,
        0
    ),
    spec!(
        "//del",
        "//del",
        "Убрать цель из мира (игрока — отключить)",
        3,
        0,
        0
    ),
    spec!(
        "//revive",
        "//revive",
        "Восстановить выбранную цель",
        3,
        0,
        0
    ),
    spec!(
        "//goto",
        "//goto",
        "Переместиться к выбранной цели",
        3,
        0,
        0
    ),
    spec!(
        "//immortal",
        "//immortal",
        "Переключить своё бессмертие",
        3,
        0,
        0
    ),
    spec!(
        "//invisible",
        "//invisible",
        "Переключить свою невидимость",
        3,
        0,
        0
    ),
    spec!(
        "//clicktp",
        "//clicktp",
        "Клик по земле телепортирует вместо бега",
        3,
        0,
        0
    ),
    spec!(
        "//bring",
        "//bring",
        "Переместить выбранного игрока к себе",
        3,
        0,
        0
    ),
    spec!(
        "//role",
        "//role id роль",
        "Назначить роль 1–5 аккаунту персонажа",
        5,
        2,
        2
    ),
    spec!(
        "//announce",
        "//announce текст",
        "Объявление всем игрокам",
        3,
        1,
        256
    ),
    spec!(
        "//resetworld",
        "//resetworld",
        "Восстановить исходных мобов с подтверждением",
        4,
        0,
        0
    ),
    spec!("/help", "/help", "Команды игрока", 1, 0, 0),
    spec!("/q", "/q", "Сохранить героя и выйти из аккаунта", 1, 0, 0),
    spec!(
        "/ignore",
        "/ignore id/ник",
        "Скрыть сообщения и запретить личные сообщения",
        1,
        1,
        1
    ),
    spec!(
        "/unignore",
        "/unignore id/ник",
        "Убрать персонажа из игнора",
        1,
        1,
        1
    ),
    spec!(
        "/ignoreall",
        "/ignoreall",
        "Скрыть все сообщения игроков",
        1,
        0,
        0
    ),
    spec!(
        "/unignoreall",
        "/unignoreall",
        "Очистить игнор и снова показывать чат",
        1,
        0,
        0
    ),
    spec!(
        "/silence",
        "/silence",
        "Запретить входящие личные сообщения",
        1,
        0,
        0
    ),
    spec!(
        "/unsilence",
        "/unsilence",
        "Разрешить личные сообщения",
        1,
        0,
        0
    ),
    spec!(
        "/ignorelist",
        "/ignorelist",
        "Открыть список игнора",
        1,
        0,
        0
    ),
    spec!(".cfg", ".cfg", "Настройки игры", 1, 0, 0),
    spec!(".chst", ".chst", "Настройки чата", 1, 0, 0),
    spec!("/time", "/time", "Дата и время сервера (UTC)", 1, 0, 0),
    spec!(
        "/heal",
        "/heal",
        "Лечение вне боя: сначала цена и подтверждение",
        1,
        0,
        0
    ),
    spec!("/invite", "/invite ник", "Пригласить в группу", 1, 1, 1),
    spec!("/leave", "/leave", "Покинуть группу", 1, 0, 0),
    spec!("/lead", "/lead ник", "Передать лидерство в группе", 1, 1, 1),
    spec!("/kick", "/kick ник", "Исключить из группы", 1, 1, 1),
    spec!(
        "//help",
        "//help",
        "Команды администрации для вашей роли",
        2,
        0,
        0
    ),
    spec!(
        "//mute",
        "//mute id/ник [секунды]",
        "Мут чата; без срока — навсегда",
        2,
        1,
        2
    ),
    spec!("//unmute", "//unmute id/ник", "Снять мут чата", 2, 1, 1),
    spec!(
        "//muteg",
        "//muteg id/ник [секунды]",
        "Общий мут чата и личных сообщений",
        2,
        1,
        2
    ),
    spec!("//unmuteg", "//unmuteg id/ник", "Снять общий мут", 2, 1, 1),
    spec!(
        "//ban",
        "//ban id/ник [дни] причина",
        "Заблокировать аккаунт",
        3,
        2,
        256
    ),
    spec!(
        "//unban",
        "//unban id/ник причина",
        "Снять блокировку аккаунта",
        3,
        2,
        256
    ),
    spec!("//banip", "//banip id/IP", "Запретить вход с IP", 4, 1, 1),
    spec!("//unbanip", "//unbanip IP", "Снять запрет IP", 4, 1, 1),
    spec!(
        "//kick",
        "//kick id/ник",
        "Завершить сессию аккаунта",
        3,
        1,
        1
    ),
    spec!(
        "//erase",
        "//erase id",
        "Удалить персонажа после подтверждения",
        4,
        1,
        1
    ),
    spec!(
        "//give",
        "//give id itemid количество",
        "Выдать предметы; монеты 11, золото 12",
        4,
        3,
        3
    ),
    spec!(
        "//bronze",
        "//bronze id",
        "Выдать 1,8 кг бронзы C90700 для пробы литья",
        4,
        1,
        1
    ),
    spec!(
        "//takeaway",
        "//takeaway id itemid [количество]",
        "Забрать предметы; без количества — все этого типа",
        4,
        2,
        3
    ),
    spec!(
        "//takeawayall",
        "//takeawayall id",
        "Очистить вещи и валюту",
        4,
        1,
        1
    ),
    spec!(
        "//learns",
        "//learns id skillid",
        "Изучить навык из текущего каталога",
        4,
        2,
        2
    ),
    spec!(
        "//unlearns",
        "//unlearns id skillid",
        "Забыть навык",
        4,
        2,
        2
    ),
];
pub struct Invocation<'a> {
    pub spec: &'static Spec,
    pub args: Vec<&'a str>,
}
pub fn parse(raw: &str) -> Result<Option<Invocation<'_>>, &'static str> {
    if !raw.starts_with('/') && !raw.starts_with(".cfg") && !raw.starts_with(".chst") {
        return Ok(None);
    }
    if raw.trim() != raw
        || raw.chars().count() > super::INPUT_LIMIT
        || raw
            .chars()
            .any(|c| c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
    {
        return Err("Команда вводится без пробелов в начале/конце и без переноса строки");
    }
    let mut words = raw.split_whitespace();
    let name = words.next().unwrap_or("");
    if name == "//recover" {
        return Err("Восстановление пока отключено: сначала необходимо настроить резервные копии");
    }
    let name = if name == "//unbunip" {
        "//unbanip"
    } else {
        name
    };
    let spec = SPECS
        .iter()
        .find(|s| s.name == name)
        .ok_or("Неизвестная команда. /help — справка")?;
    let args: Vec<_> = words.collect();
    if args.len() < spec.min || args.len() > spec.max {
        return Err(spec.syntax);
    }
    Ok(Some(Invocation { spec, args }))
}
pub fn help(role: i16, staff: bool) -> String {
    SPECS
        .iter()
        .filter(|s| s.role <= role && (s.role > 1) == staff)
        .map(|s| format!("{} — {}", s.syntax, s.description))
        .collect::<Vec<_>>()
        .join("\n")
}
