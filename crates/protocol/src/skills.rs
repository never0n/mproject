//! Starter values from TOP_Project/database/seed.js; ranges adapted to desktop metres.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct Skill {
    pub name: &'static str,
    pub multiplier: f64,
    pub cost: f64,
    pub range: f32,
    pub cooldown_ticks: u64,
}

pub const RACIAL_SKILL: u8 = 6;
pub const TRADER_SKILL: u8 = 7;
pub const LAST_ACTIVE: u8 = RACIAL_SKILL;
pub const LAST_SKILL: u8 = TRADER_SKILL;
pub const EXTRA_COOLDOWNS: usize = (LAST_ACTIVE - 2) as usize;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Bleed,
    Cripple,
    Blind,
    Prism,
    Quicksand,
    Overload,
    Smoke,
    Earth,
    Acid,
    Trader,
}
pub struct Ability {
    pub skill: Skill,
    pub effect: Effect,
    pub level: u32,
    pub sp: f64,
    pub icon: usize,
    pub description: &'static str,
}
impl Ability {
    pub fn needs_target(&self) -> bool {
        !matches!(self.effect, Effect::Smoke | Effect::Trader)
    }
}

pub fn definition(class: u8, race: u8, id: u8) -> Option<Ability> {
    if id == TRADER_SKILL {
        return Some(Ability {
            skill: Skill {
                name: "Торгаш",
                multiplier: 0.,
                cost: 0.,
                range: 0.,
                cooldown_ticks: 0,
            },
            effect: Effect::Trader,
            level: 2,
            sp: 5.,
            icon: 24,
            description: "Пассивный навык. Снижает цену покупок в лавке на 10%, итог округляется вверх до целой монеты. Работает постоянно после изучения; не меняет цены продажи.",
        });
    }
    if id != RACIAL_SKILL {
        return ability(class, id);
    }
    let (name, multiplier, cost, range, cooldown, effect, icon, description) = match race {
        1 | 5 => (
            "Дымовая завеса",
            0.,
            10.,
            0.,
            20,
            Effect::Smoke,
            21,
            "На 4 с снижает вероятность попадания противников по вам на 80%. Применяется на себя без выбора цели; можно двигаться и атаковать. Расовый навык людей и дварфов.",
        ),
        2..=4 => (
            "Гнев земли",
            0.7,
            12.,
            12.,
            16,
            Effect::Earth,
            22,
            "Удар по врагам в радиусе 4 м от выбранной цели. Попадание оглушает на 1,5 с: цель не движется и не атакует. Стены блокируют действие. Расовый навык эльфов и полуорков.",
        ),
        6 => (
            "Кислотный плевок",
            0.5,
            8.,
            8.,
            12,
            Effect::Acid,
            23,
            "При попадании обнуляет физическую защиту цели на 5 с. Магическая защита не меняется. Повтор обновляет длительность. Расовый навык полумутантов.",
        ),
        _ => return None,
    };
    Some(Ability {
        skill: Skill {
            name,
            multiplier,
            cost,
            range,
            cooldown_ticks: cooldown * 20,
        },
        effect,
        level: 2,
        sp: 3.,
        icon,
        description,
    })
}

/// Ideas from the user's skill brainstorm; concrete values are desktop prototype balance.
pub fn ability(class: u8, id: u8) -> Option<Ability> {
    let (name, multiplier, cost, cooldown, effect, icon, description) = match (class, id) {
        (1, 3) => (
            "Разрыв аорты",
            0.8,
            8.,
            8,
            Effect::Bleed,
            15,
            "При попадании: кровотечение на 6 с, каждую секунду 10% физической атаки. Каждый метр движения цели усиливает кровотечение на 10%, максимум вдвое. Повтор обновляет эффект.",
        ),
        (1, 4) => (
            "Костолом",
            1.1,
            10.,
            10,
            Effect::Cripple,
            16,
            "При попадании: интервалы между атаками цели увеличиваются на 50% на 6 с.",
        ),
        (1, 5) => (
            "Грязный приём",
            0.4,
            8.,
            12,
            Effect::Blind,
            17,
            "При попадании: ослепление на 1,5 с. Вероятность попадания цели уменьшается на 80%.",
        ),
        (2, 3) => (
            "Призма хаоса",
            1.25,
            10.,
            8,
            Effect::Prism,
            18,
            "При попадании случайная стихия: огонь — горение 4 с (15% магической атаки в секунду); лёд — замедление на 40% на 4 с; яд — отравление 6 с (10% атаки в секунду).",
        ),
        (2, 4) => (
            "Зыбучий песок",
            0.,
            14.,
            14,
            Effect::Quicksand,
            19,
            "Создаёт область радиусом 4 м у выбранного врага на 6 с. Внутри враги движутся на 50% медленнее, их вероятность попадания снижается на 30%. Стены закрывают действие области.",
        ),
        (2, 5) => (
            "Перегрузка",
            2.6,
            20.,
            20,
            Effect::Overload,
            20,
            "Мощный выброс маны. После применения все активные навыки недоступны не менее 10 с, включая промах. Более долгие перезарядки сохраняются.",
        ),
        _ => return None,
    };
    Some(Ability {
        skill: Skill {
            name,
            multiplier,
            cost,
            range: if class == 1 { 3. } else { 12. },
            cooldown_ticks: cooldown * 20,
        },
        effect,
        level: u32::from(id - 1),
        sp: f64::from(id),
        icon,
        description,
    })
}
pub fn learned(progress: &crate::progression::Progress, id: u8) -> bool {
    match id {
        1 => !progress.primary_disabled,
        2 => progress.secondary_learned,
        3..=LAST_SKILL => progress.extra_skills & (1 << (id - 3)) != 0,
        _ => false,
    }
}
pub fn cooldown(actor: &crate::Actor, id: u8) -> u32 {
    match id {
        1 => actor.skill_cooldown_ms,
        2 => actor.secondary_cooldown_ms,
        3..=LAST_ACTIVE => actor.ability_cooldowns_ms[usize::from(id - 3)],
        _ => 0,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    Bleeding,
    Burning,
    Poisoned,
    Blinded,
    Crippled,
    Quicksand,
    Smoke,
    Stunned,
    Corroded,
}
impl StatusKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bleeding => "Кровотечение",
            Self::Burning => "Горение",
            Self::Poisoned => "Яд",
            Self::Blinded => "Ослепление",
            Self::Crippled => "Травма рук",
            Self::Quicksand => "Зыбучий песок",
            Self::Smoke => "Дымовая завеса",
            Self::Stunned => "Оглушение",
            Self::Corroded => "Разъеденная броня",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub kind: StatusKind,
    pub remaining_ms: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Area {
    pub id: u64,
    pub position: [f32; 3],
    pub radius: f32,
    pub remaining_ms: u32,
}
pub fn starter(class: u8) -> Option<Skill> {
    match class {
        1 => Some(Skill {
            name: "Мощная атака",
            multiplier: 1.2,
            cost: 5.,
            range: 3.,
            cooldown_ticks: 40,
        }),
        2 => Some(Skill {
            name: "Фаербол",
            multiplier: 1.1,
            cost: 5.,
            range: 12.,
            cooldown_ticks: 40,
        }),
        _ => None,
    }
}

/// Prototype rank progression; rank one retains the legacy starter parameters.
pub fn ranked(class: u8, rank: u8) -> Option<Skill> {
    let mut skill = starter(class)?;
    skill.multiplier += 0.2 * f64::from(rank.clamp(1, 3) - 1);
    Some(skill)
}
pub fn upgrade_cost(target_rank: u8) -> Option<(u32, f64)> {
    match target_rank {
        2 => Some((2, 5.)),
        3 => Some((3, 7.)),
        _ => None,
    }
}

pub fn secondary(class: u8) -> Option<Skill> {
    match class {
        1 => Some(Skill {
            name: "Подрезать сухожилия",
            multiplier: 1.1,
            cost: 8.,
            range: 3.,
            cooldown_ticks: 160,
        }),
        2 => Some(Skill {
            name: "Ледяная стрела",
            multiplier: 1.2,
            cost: 8.,
            range: 12.,
            cooldown_ticks: 160,
        }),
        _ => None,
    }
}
