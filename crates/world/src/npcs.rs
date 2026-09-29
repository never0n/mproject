//! Fixed town residents: identical placement and service access on server and client.
use crate::{HALL_SPECS, MERCHANT, World, distance, rotate};

pub const PLAZA_TRADER: u8 = 1;
pub const TALK_RANGE: f32 = 6.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Merchant,
    Leatherworker,
    Smith,
    Runekeeper,
    Postmaster,
    GuildSteward,
}
impl Role {
    pub fn title(self) -> &'static str {
        match self {
            Self::Merchant => "Торговец",
            Self::Leatherworker => "Мастер кожевничества",
            Self::Smith => "Кузнец",
            Self::Runekeeper => "Мастер рунописи",
            Self::Postmaster => "Почтмейстер",
            Self::GuildSteward => "Распорядитель гильдий",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Npc {
    pub id: u8,
    pub placement: Option<([f32; 3], f32)>,
    pub name: &'static str,
    pub role: Role,
    /// Public hall, side of its counters, and counter row. None is the plaza stall.
    pub station: Option<(usize, f32, usize)>,
    pub race: u8,
    pub gender: u8,
    pub greeting: &'static str,
}
pub const RESIDENTS: &[Npc] = &[
    Npc {
        placement: None,
        id: PLAZA_TRADER,
        name: "Марен",
        role: Role::Merchant,
        station: None,
        race: 1,
        gender: 1,
        greeting: "Добро пожаловать в Мортаун. В дороге пригодятся хорошие припасы — и надёжное оружие.",
    },
    Npc {
        placement: None,
        id: 2,
        name: "Элира",
        role: Role::Merchant,
        station: Some((2, -1., 4)),
        race: 1,
        gender: 2,
        greeting: "В торговых рядах всегда найдётся место путнику. Выбирайте оружие и припасы; ненужные трофеи я тоже принимаю.",
    },
    Npc {
        placement: None,
        id: 3,
        name: "Тесса",
        role: Role::Leatherworker,
        station: Some((2, 1., 4)),
        race: 3,
        gender: 2,
        greeting: "Шов должен держать удар, а кожа — не сковывать движение. Принесите рецепт и материалы: разберём изготовление кирасы. Мастерство растёт с каждым вашим изделием.",
    },
    Npc {
        placement: None,
        id: 4,
        name: "Борин",
        role: Role::Smith,
        station: Some((3, -1., 4)),
        race: 5,
        gender: 1,
        greeting: "Сталь требует терпения. Для первого клинка нужны рецепт, слитки и трофеи мародёров. Освоите основы — сможете выбрать кузнечное дело своей специализацией.",
    },
    Npc {
        placement: None,
        id: 5,
        name: "Саэль",
        role: Role::Runekeeper,
        station: Some((1, -1., 4)),
        race: 2,
        gender: 1,
        greeting: "Руна хранит замысел мастера. Для рунного посоха понадобятся рецепт, осколки и знаки отступников. Покажу, как соединить их силу.",
    },
    Npc {
        placement: None,
        id: 6,
        name: "Вальтер",
        role: Role::GuildSteward,
        station: Some((3, 1., 4)),
        race: 1,
        gender: 1,
        greeting: "Дом гильдий открыт для союзов искателей приключений. Здесь можно основать альянс или заняться делами своего: составом, ролями и общим складом.",
    },
    Npc {
        placement: None,
        id: 7,
        name: "Ивен",
        role: Role::Postmaster,
        station: Some((0, -1., 4)),
        race: 1,
        gender: 1,
        greeting: "Городская почта к вашим услугам. Письмо найдёт адресата, даже если его сейчас нет в городе. Вложения можно отправить бесплатно или с оплатой при получении.",
    },
];

pub fn definition(id: u8) -> Option<&'static Npc> {
    RESIDENTS.iter().find(|n| n.id == id)
}
impl Npc {
    pub fn at(mut self, id: u8, position: [f32; 3], yaw: f32) -> Self {
        self.id = id;
        self.placement = Some((position, yaw));
        self
    }

    pub fn hall(self) -> Option<usize> {
        self.station.map(|s| s.0)
    }
    pub fn counter(self) -> [f32; 2] {
        let Some((hall, side, row)) = self.station else {
            return [0., 0.];
        };
        let [w, d] = HALL_SPECS[hall].size;
        [
            side * (w / 2. - 7.),
            -d / 2. + 7. + row as f32 * (d - 16.) / 5.,
        ]
    }
    pub fn position(self) -> [f32; 2] {
        if let Some((p, _)) = self.placement {
            return [p[0], p[2]];
        }
        let Some((hall, ..)) = self.station else {
            return MERCHANT;
        };
        let spec = HALL_SPECS[hall];
        let [x, z] = self.counter();
        // 35 cm behind the rear edge: clear of the counter, within arm's reach.
        rotate(spec.center, x, z - 1.35, spec.yaw)
    }
    pub fn counter_position(self) -> [f32; 2] {
        self.hall().map_or([MERCHANT[0] - 1., MERCHANT[1]], |hall| {
            let spec = HALL_SPECS[hall];
            let [x, z] = self.counter();
            rotate(spec.center, x, z, spec.yaw)
        })
    }
    /// Rear working edge, shared by the furnishings and the client's hand targets.
    pub fn work_offset(self) -> f32 {
        if self.hall().is_some() { -0.80 } else { -0.35 }
    }
    pub fn work_height(self) -> f32 {
        // Anvil face and counter top from the reference meshes, not the goods above them.
        if self.role == Role::Smith { 0.82 } else { 1.0 }
    }
    pub fn work_surface(self, world: &World) -> [f32; 3] {
        let c = self.counter_position();
        let [x, z] = rotate(c, 0., self.work_offset(), self.yaw());
        [x, world.surface_height(c[0], c[1]) + self.work_height(), z]
    }
    pub fn yaw(self) -> f32 {
        if let Some((_, yaw)) = self.placement {
            return yaw;
        }
        self.hall()
            .map_or(-std::f32::consts::FRAC_PI_2, |h| HALL_SPECS[h].yaw)
    }
    pub fn feet(self, world: &World) -> [f32; 3] {
        if let Some((p, _)) = self.placement {
            return p;
        }
        let [x, z] = self.position();
        [x, world.surface_height(x, z), z]
    }
    pub fn same_room(self, world: &World, from: [f32; 3]) -> bool {
        world.hall_at(from[0], from[2])
            == self
                .placement
                .map_or(self.hall(), |(p, _)| world.hall_at(p[0], p[2]))
    }
    pub fn available(self, world: &World, from: [f32; 3]) -> bool {
        if from.iter().any(|v| !v.is_finite()) {
            return false;
        }
        let to = self.feet(world);
        distance([from[0], from[2]], [to[0], to[2]]).hypot(from[1] - to[1]) <= TALK_RANGE
            && self.same_room(world, from)
            && world.sight_clear(
                [from[0], from[1] + 1.5, from[2]],
                [to[0], to[1] + 1.5, to[2]],
            )
    }
}
