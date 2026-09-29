//! Explicit, disconnected UI fixtures rendered by the actual game (never a real character).
use crate::{
    Player, RunOptions,
    network::{Mode, Network},
    ui::CharacterWindow,
};
use bevy::prelude::*;
pub fn setup(
    options: Res<RunOptions>,
    mut network: ResMut<Network>,
    player: Single<&Transform, With<Player>>,
    mut panels: ResMut<CharacterWindow>,
    mut combat: ResMut<crate::combat::CombatUi>,
    mut menu: ResMut<crate::menu::Help>,
) {
    let Some(screen) = &options.ui_preview else {
        return;
    };
    if screen == "login" {
        network.mode = Mode::Menu;
        return;
    }
    let mut stats = mortown_protocol::starting_stats(1, 1).unwrap();
    for (key, value) in [
        ("level", 12.),
        ("current_exp", 640.),
        ("exp", 1200.),
        ("coins", 427.),
        ("gold", 3.),
        ("sp", 48.),
        ("current_hp", 182.),
        ("hp", 240.),
        ("current_mana", 86.),
        ("mana", 120.),
        ("current_stamina", 132.),
        ("stamina", 150.),
        ("skill_rank", 2.),
        ("secondary_learned", 1.),
    ] {
        stats.insert(key.into(), value);
    }
    let loot = [
        (1, 1),
        (3, 1),
        (4, 1),
        (6, 1),
        (7, 24),
        (8, 7),
        (9, 12),
        (10, 1),
    ]
    .into_iter()
    .collect();
    let mut inventory = mortown_protocol::items::starter(1);
    mortown_protocol::items::include_loot(&mut inventory, &loot);
    for (n, token) in [1_u64, 2, 0, 0, 8_u64 * (1 << 32), 0, 0, 0, 0, 0, 0, 0]
        .into_iter()
        .enumerate()
    {
        stats.insert(format!("hotbar_{n}"), token as f64);
    }
    for item in &mut inventory {
        item.equipped = matches!(item.kind, 1 | 3);
    }
    let c = mortown_protocol::Character {
        id: -1,
        slot: 1,
        nickname: "Эйрдан".into(),
        race: 1,
        class: 1,
        gender: 1,
        stats,
        inventory,
        loot,
    };
    let actor = mortown_protocol::Actor {
        id: c.id,
        nickname: c.nickname.clone(),
        equipment: mortown_protocol::items::EquipmentAppearance::from_inventory(&c.inventory),
        appearance: (&c).into(),
        position: player.translation.to_array(),
        yaw: 0.,
        speed: 0.,
        hp: 182.,
        max_hp: 240.,
        mana: 86.,
        stamina: 132.,
        respawn_seconds: 0,
        attack_target: None,
        actions: vec![],
        combat_status: String::new(),
        skill_cooldown_ms: 0,
        secondary_cooldown_ms: 0,
        ability_cooldowns_ms: Default::default(),
        effects: vec![],
        progress: mortown_protocol::progression::Progress::from_stats(&c.stats),
        powers: Default::default(),
        reward_status: String::new(),
        loot: c.loot.clone(),
    };
    network.mode = Mode::Online;
    network.character = Some(c);
    network.authoritative = Some(actor.clone());
    network.roster = vec![actor];
    network.status.clear();
    for (id, name, text) in [
        (1, "Мирэль", "Кто идёт к северным воротам?"),
        (2, "Бран", "Собираем группу. Есть ещё одно место."),
        (3, "Эйрдан", "Я с вами. Встречаемся на площади."),
    ] {
        network.chat.messages.push(mortown_protocol::chat::Message {
            recipient: None,
            id,
            sender: id,
            nickname: name.into(),
            text: text.into(),
            channel: mortown_protocol::chat::Channel::General,
            party: None,
            alliance: None,
        });
    }
    network.chat.revision = 1;
    if screen == "target" {
        combat.selected = Some(1);
        network.monsters.push(mortown_protocol::Monster {
            kind: mortown_protocol::monsters::Kind::Bandit,
            cast: None,
            attack: None,
            death_ms: None,
            id: 1,
            name: "Разбойник тракта".into(),
            level: 9,
            position: player.translation.to_array(),
            yaw: 0.,
            hp: 142.,
            max_hp: 200.,
            mana: None,
            stamina: None,
            slow_ms: 0,
            effects: vec![],
        });
    }
    *panels = CharacterWindow {
        open: screen != "hud" && screen != "target",
        inventory: screen == "inventory" || screen == "tooltip",
        skills: screen == "skills",
        menu: screen == "menu",
        journal: screen == "journal",
        mail: screen == "mail",
        ..default()
    };
    if screen == "menu" {
        menu.0 = false;
    }
}

pub fn gallery(
    mut options: ResMut<RunOptions>,
    mut panels: ResMut<CharacterWindow>,
    mut network: ResMut<Network>,
    mut combat: ResMut<crate::combat::CombatUi>,
) {
    if options.ui_preview.as_deref() != Some("gallery") {
        return;
    }
    let name = match options.frame {
        10 => "hud",
        130 => "inventory",
        250 => "skills",
        370 => "character",
        490 => "journal",
        610 => "menu",
        730 => "mail",
        850 => "login",
        _ => return,
    };
    let directory = options
        .capture
        .as_ref()
        .and_then(|p| std::path::Path::new(p).parent())
        .map(|p| p.to_owned())
        .unwrap_or_else(|| std::path::PathBuf::from("artifacts/ui-redesign"));
    options.capture = Some(
        directory
            .join(format!("{name}.png"))
            .to_string_lossy()
            .into(),
    );
    *panels = CharacterWindow {
        open: name != "hud" && name != "login",
        inventory: name == "inventory",
        skills: name == "skills",
        journal: name == "journal",
        menu: name == "menu",
        mail: name == "mail",
        ..default()
    };
    combat.selected = None;
    if name == "mail" {
        let header = mortown_protocol::mail::Header {
            id: 1,
            return_of: None,
            sender: "Мирэль".into(),
            recipient: "Эйрдан".into(),
            subject: "До встречи у северных ворот".into(),
            sent_at: "14.09.2026 12:40".into(),
            read: true,
            attachment: Some(mortown_protocol::mail::Attachment {
                kind: 8,
                quantity: 3,
                claimed: false,
                returned: false,
                price: 0,
            }),
        };
        network.mail.receive(mortown_protocol::mail::Reply::List {
            page: mortown_protocol::mail::Page {
                folder: mortown_protocol::mail::Folder::Inbox,
                page: 0,
                total: 1,
                unread: 0,
                entries: vec![header.clone()],
            },
        });
        network.mail.receive(mortown_protocol::mail::Reply::Read{letter:mortown_protocol::mail::Letter{header,body:"Эйрдан,\n\nмы отправляемся к северному перевалу на рассвете. Возьми припасы и тёплый плащ. Встречаемся у ворот, как только город проснётся.\n\nОставила тебе несколько зелий — в дороге пригодятся.\n\nМирэль".into()}});
    }
    if name == "login" {
        network.mode = Mode::Menu;
        network.status.clear();
    }
}

/// Park the cursor over an actual rendered slot for the small-window hover capture.
pub fn hover(
    options: Res<RunOptions>,
    mut window: Single<&mut Window, With<bevy::window::PrimaryWindow>>,
    slots: Query<(&crate::actionbar::Slot, &UiGlobalTransform)>,
) {
    if options.ui_preview.as_deref() == Some("tooltip")
        && options.frame == 70
        && let Some((_, transform)) = slots
            .iter()
            .find(|(s, _)| matches!(s, crate::actionbar::Slot::Bag(0)))
    {
        window
            .set_physical_cursor_position(Some(transform.transform_point2(Vec2::ZERO).as_dvec2()));
    }
}
