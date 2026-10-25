//! Fixed work counters of the starting capital: identical placement on server and client.
//! They are furniture only; no resident or service is attached to them.
use crate::{HALL_SPECS, PLAZA_STALL, World, rotate};

/// What dresses the counter: the world adds the anvil/forge or crates, the client adds small props.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Furnishing {
    Market,
    Leatherwork,
    Smithy,
    Runes,
    Letters,
    Ledger,
}

/// A counter inside a civic hall (`hall`, side of the hall, counter row) or, with `None`, the plaza stall.
#[derive(Clone, Copy, Debug)]
pub struct Station {
    pub furnishing: Furnishing,
    pub hall: Option<(usize, f32, usize)>,
}
pub const STATIONS: &[Station] = &[
    Station {
        furnishing: Furnishing::Market,
        hall: None,
    },
    Station {
        furnishing: Furnishing::Market,
        hall: Some((2, -1., 4)),
    },
    Station {
        furnishing: Furnishing::Leatherwork,
        hall: Some((2, 1., 4)),
    },
    Station {
        furnishing: Furnishing::Smithy,
        hall: Some((3, -1., 4)),
    },
    Station {
        furnishing: Furnishing::Runes,
        hall: Some((1, -1., 4)),
    },
    Station {
        furnishing: Furnishing::Ledger,
        hall: Some((3, 1., 4)),
    },
    Station {
        furnishing: Furnishing::Letters,
        hall: Some((0, -1., 4)),
    },
];

impl Station {
    pub fn hall(self) -> Option<usize> {
        self.hall.map(|s| s.0)
    }
    /// Counter position in hall-local coordinates: 7 m in from the side wall (`side` = ±1),
    /// rows spread evenly along the depth.
    fn local(self) -> [f32; 2] {
        let Some((hall, side, row)) = self.hall else {
            return [0., 0.];
        };
        let [w, d] = HALL_SPECS[hall].size;
        [
            side * (w / 2. - 7.),
            -d / 2. + 7. + row as f32 * (d - 16.) / 5.,
        ]
    }
    pub fn counter_position(self) -> [f32; 2] {
        self.hall().map_or(PLAZA_STALL, |hall| {
            let spec = HALL_SPECS[hall];
            let [x, z] = self.local();
            rotate(spec.center, x, z, spec.yaw)
        })
    }
    /// Rear working edge, shared by the furnishings and the client's counter props.
    pub fn work_offset(self) -> f32 {
        if self.hall().is_some() { -0.80 } else { -0.35 }
    }
    pub fn work_height(self) -> f32 {
        // Anvil face and counter top from the reference meshes, not the goods above them.
        if self.furnishing == Furnishing::Smithy {
            0.82
        } else {
            1.0
        }
    }
    pub fn work_surface(self, world: &World) -> [f32; 3] {
        let c = self.counter_position();
        let [x, z] = rotate(c, 0., self.work_offset(), self.yaw());
        [x, world.surface_height(c[0], c[1]) + self.work_height(), z]
    }
    pub fn yaw(self) -> f32 {
        self.hall()
            .map_or(-std::f32::consts::FRAC_PI_2, |h| HALL_SPECS[h].yaw)
    }
}
