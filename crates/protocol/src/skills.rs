//! Starter values from TOP_Project/database/seed.js; ranges adapted to desktop metres.
//! Every human may learn every skill; the equipped weapon (`weapons::magical`) picks the
//! physical or magical variant, its resource and its range.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct Skill {
    pub name: &'static str,
    pub multiplier: f64,
    pub cost: f64,
    pub range: f32,
    pub cooldown_ticks: u64,
}

pub const LAST_ACTIVE: u8 = 5;
pub const LAST_SKILL: u8 = LAST_ACTIVE;
pub const EXTRA_COOLDOWNS: usize = (LAST_ACTIVE - 2) as usize;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Bleed,
    Cripple,
    Blind,
    Prism,
    Quicksand,
    Overload,
}
pub struct Ability {
    pub skill: Skill,
    pub effect: Effect,
    pub icon: usize,
    pub description: &'static str,
}

/// Ideas from the user's skill brainstorm; concrete values are desktop prototype balance.
pub fn definition(magical: bool, id: u8) -> Option<Ability> {
    let (name, multiplier, cost, cooldown, effect, icon, description) = match (magical, id) {
        (false, 3) => (
            "Разрыв аорты",
            0.8,
            8.,
            8,
            Effect::Bleed,
            15,
            "При попадании: кровотечение на 6 с, каждую секунду 10% физической атаки. Каждый метр движения цели усиливает кровотечение на 10%, максимум вдвое. Повтор обновляет эффект.",
        ),
        (false, 4) => (
            "Костолом",
            1.1,
            10.,
            10,
            Effect::Cripple,
            16,
            "При попадании: интервалы между атаками цели увеличиваются на 50% на 6 с.",
        ),
        (false, 5) => (
            "Грязный приём",
            0.4,
            8.,
            12,
            Effect::Blind,
            17,
            "При попадании: ослепление на 1,5 с. Вероятность попадания цели уменьшается на 80%.",
        ),
        (true, 3) => (
            "Призма хаоса",
            1.25,
            10.,
            8,
            Effect::Prism,
            18,
            "При попадании случайная стихия: огонь — горение 4 с (15% магической атаки в секунду); лёд — замедление на 40% на 4 с; яд — отравление 6 с (10% атаки в секунду).",
        ),
        (true, 4) => (
            "Зыбучий песок",
            0.,
            14.,
            14,
            Effect::Quicksand,
            19,
            "Создаёт область радиусом 4 м у выбранного врага на 6 с. Внутри враги движутся на 50% медленнее, их вероятность попадания снижается на 30%. Стены закрывают действие области.",
        ),
        (true, 5) => (
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
            range: if magical { 12. } else { 3. },
            cooldown_ticks: cooldown * 20,
        },
        effect,
        icon,
        description,
    })
}
pub fn learned(progress: &crate::progression::Progress, id: u8) -> bool {
    progress.known(id)
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
pub fn starter(magical: bool) -> Skill {
    if magical {
        Skill {
            name: "Фаербол",
            multiplier: 1.1,
            cost: 5.,
            range: 12.,
            cooldown_ticks: 40,
        }
    } else {
        Skill {
            name: "Мощная атака",
            multiplier: 1.2,
            cost: 5.,
            range: 3.,
            cooldown_ticks: 40,
        }
    }
}

/// [PROPOSAL] Each practice rank above the first adds 0.2 to the basic attack multiplier.
pub fn ranked(magical: bool, rank: u8) -> Skill {
    let mut skill = starter(magical);
    skill.multiplier += 0.2 * f64::from(rank.saturating_sub(1));
    skill
}

pub fn secondary(magical: bool) -> Skill {
    if magical {
        Skill {
            name: "Ледяная стрела",
            multiplier: 1.2,
            cost: 8.,
            range: 12.,
            cooldown_ticks: 160,
        }
    } else {
        Skill {
            name: "Подрезать сухожилия",
            multiplier: 1.1,
            cost: 8.,
            range: 3.,
            cooldown_ticks: 160,
        }
    }
}
