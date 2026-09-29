//! The merchant's introductory chain. IDs and stages remain stable in saved characters.
use crate::{items, progression::Progress};
use std::collections::BTreeMap;

pub const CRAFT_QUEST: u8 = 3;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Locked,
    Available,
    Active,
    Completed,
}
impl Status {
    pub fn tab(self) -> Option<u8> {
        match self {
            Self::Locked => None,
            Self::Available => Some(0),
            Self::Active => Some(1),
            Self::Completed => Some(2),
        }
    }
}
pub enum Objective {
    Bandits(u32),
    Supplies(&'static [(u8, u32)]),
    Craft(u8),
}
pub struct Quest {
    pub id: u8,
    pub title: &'static str,
    pub story: &'static str,
    pub hint: &'static str,
    pub thanks: &'static str,
    pub objective: Objective,
    pub coins: u32,
    pub rewards: &'static [(u8, u32)],
    pub sp: u32,
}
pub const QUESTS: [Quest; 3] = [
    Quest {
        id: 1,
        title: "Безопасная дорога",
        story: "Обозы застряли в городе: разбойники обосновались за южными воротами. Разберитесь с тремя из них, и караванщики снова решатся выйти на дорогу.",
        hint: "Победите трёх разбойников после принятия поручения. Лагерь находится за южными воротами Мортауна.",
        thanks: "Караванщики уже передают добрые вести. Дорога свободна — получите заслуженную награду.",
        objective: Objective::Bandits(3),
        coins: 75,
        rewards: &[(8, 2)],
        sp: 0,
    },
    Quest {
        id: 2,
        title: "Запас для каравана",
        story: "Люди готовы в путь, но поклажу изрядно потрепало. Принесите четыре отреза ткани и четыре куска кожи: залатаем тенты и ремни. В благодарность поделюсь рецептом кирасы.",
        hint: "Ткань и кожа встречаются в добыче разбойников. Подойдут и уже собранные материалы. Принесите их в своём рюкзаке: при сдаче торговец заберёт по 4 единицы.",
        thanks: "Прочная кожа и сухая ткань — то, что нужно. Эти припасы пойдут каравану, а рецепт достанется вам.",
        objective: Objective::Supplies(&[(7, 4), (9, 4)]),
        coins: 100,
        rewards: &[(10, 1)],
        sp: 0,
    },
    Quest {
        id: 3,
        title: "Работа на совесть",
        story: "Теперь попробуйте рецепт в деле. Изготовьте усиленную кожаную кирасу своими руками. Доспех оставьте себе: в дороге он пригодится больше, чем на полке моей лавки.",
        hint: "После принятия поручения откройте рюкзак → Ремесло и изготовьте кирасу. Точный расход материалов указан в рецепте с учётом вашего мастерства. Покупка или получение готового доспеха не засчитываются. Кираса остаётся у вас.",
        thanks: "Хорошая работа! Теперь вы умеете не только добывать материалы, но и превращать их в полезную вещь. Удачи на дороге, мастер.",
        objective: Objective::Craft(6),
        coins: 150,
        rewards: &[(8, 3)],
        sp: 5,
    },
];
pub fn definition(id: u8) -> Option<&'static Quest> {
    QUESTS.iter().find(|q| q.id == id)
}
pub fn current(p: &Progress) -> Option<&'static Quest> {
    QUESTS
        .iter()
        .find(|q| matches!(q.status(p), Status::Available | Status::Active))
}
impl Quest {
    pub fn active_stage(&self) -> u8 {
        self.id * 2 - 1
    }
    pub fn status(&self, p: &Progress) -> Status {
        let active = self.active_stage();
        if p.quest_stage > active {
            Status::Completed
        } else if p.quest_stage == active {
            Status::Active
        } else if p.quest_stage == active - 1 {
            Status::Available
        } else {
            Status::Locked
        }
    }
    pub fn ready(&self, p: &Progress, loot: &BTreeMap<u8, u32>) -> bool {
        self.status(p) == Status::Active
            && match self.objective {
                Objective::Bandits(count) => p.quest_kills >= count,
                Objective::Supplies(materials) => materials
                    .iter()
                    .all(|(kind, count)| loot.get(kind).copied().unwrap_or(0) >= *count),
                Objective::Craft(_) => p.quest_crafted,
            }
    }
    pub fn progress_text(&self, p: &Progress, loot: &BTreeMap<u8, u32>) -> String {
        if self.status(p) == Status::Completed {
            return "Завершено · награда получена".into();
        }
        let active = self.status(p) == Status::Active;
        match self.objective {
            Objective::Bandits(count) => format!(
                "Разбойники: {} / {count}",
                if active { p.quest_kills.min(count) } else { 0 }
            ),
            Objective::Supplies(materials) => materials
                .iter()
                .map(|(kind, count)| {
                    format!(
                        "{}: {} / {count}",
                        items::definition(*kind).unwrap().name,
                        loot.get(kind).copied().unwrap_or(0)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Objective::Craft(kind) => format!(
                "Изготовить «{}»: {} / 1",
                items::definition(kind).unwrap().name,
                u8::from(active && p.quest_crafted)
            ),
        }
    }
    pub fn reward_text(&self) -> String {
        let mut parts = vec![format!("{} монет", self.coins)];
        parts.extend(
            self.rewards.iter().map(|(kind, count)| {
                format!("{} ×{count}", items::definition(*kind).unwrap().name)
            }),
        );
        if self.sp > 0 {
            parts.push(format!("{} SP", self.sp));
        }
        parts.join(" · ")
    }
}
