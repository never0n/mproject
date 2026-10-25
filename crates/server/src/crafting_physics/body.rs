//! Operations after the cavity has a fill field: feeding account, lumped cooling,
//! machining and the resulting mass properties. Machining removes metal; it does
//! not return stock and it does not add a combat bonus.

use crate::crafting_physics::{
    enthalpy::Material,
    geometry::{Mold, sword_width},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FeedMode {
    /// Extra mass is reserved and consumed. The feeder neck is not solved.
    IdealFeeder,
    /// Keep the mass that actually entered. Pore coordinates are not invented.
    Unresolved,
}

#[derive(Clone, Copy, Debug)]
pub struct FeedAccount {
    pub mode: FeedMode,
    pub liquid_mass: f64,
    pub unfilled_volume: f64,
    /// Mass still required for a dense cold body. Zero when the mode does not supply it.
    pub feed_mass: f64,
}

pub fn feed_account(
    mold: &Mold,
    material: &Material,
    filled_volume: f64,
    mode: FeedMode,
) -> FeedAccount {
    let cavity: f64 = mold.cells.iter().map(|c| c.volume).sum();
    let liquid_mass = filled_volume * material.rho_liquid;
    let full = filled_volume >= cavity - 1e-12;
    let dense =
        crate::crafting_physics::geometry::drawing_totals(mold.thickness).0 * material.rho_cold;
    let feed_mass = if full && mode == FeedMode::IdealFeeder {
        (dense - cavity * material.rho_liquid).max(0.)
    } else {
        0.
    };
    FeedAccount {
        mode: if feed_mass > 0. {
            FeedMode::IdealFeeder
        } else {
            FeedMode::Unresolved
        },
        liquid_mass,
        unfilled_volume: (cavity - filled_volume).max(0.),
        feed_mass,
    }
}

/// Seconds for one constant-capacity stage of the lumped body.
pub fn lumped_cool(
    mass: f64,
    area: f64,
    film: f64,
    environment: f64,
    from_c: f64,
    to_c: f64,
    capacity: f64,
) -> f64 {
    mass * capacity / (film * area) * ((from_c - environment) / (to_c - environment)).ln()
}

#[derive(Clone, Copy, Debug)]
pub struct Assembly {
    pub metal_mass: f64,
    pub removed_mass: f64,
    pub total_mass: f64,
    /// From the blade root, toward the tip.
    pub center: f64,
    /// About the grip, normal to the blade plane.
    pub inertia: f64,
}

/// Bevel the blade and tip. The tang is left at full thickness. `edge` is the
/// finished edge thickness, one input of the proposal, not a reward.
pub fn machine_profile(material: &Material, edge: f64) -> Assembly {
    let thickness = 0.006;
    let rho = material.rho_cold;
    let dx = 1e-5_f64;
    let mut metal = 0.;
    let mut moment = 0.;
    let mut spin = 0.;
    let mut removed = 0.;
    for (start, end, grind) in [(-0.12, 0., false), (0., 0.60, true), (0.60, 0.65, true)] {
        let mut x: f64 = start;
        while x < end - 1e-15 {
            let x1 = (x + dx).min(end);
            let mid = 0.5 * (x + x1);
            let width = sword_width(mid);
            let full = width * thickness;
            let section = if grind {
                width * (3. * thickness + edge) / 4.
            } else {
                full
            };
            let span = x1 - x;
            let dm = rho * section * span;
            metal += dm;
            removed += rho * (full - section) * span;
            moment += dm * mid;
            let local = if grind {
                width.powi(3) * (15. * thickness + 17. * edge) / 384.
            } else {
                thickness * width.powi(3) / 12.
            };
            spin += dm * (mid + 0.06).powi(2) + rho * local * span;
            x = x1;
        }
    }
    let handle = 0.18;
    let total = metal + handle;
    spin += handle * 0.12_f64.powi(2) / 12.;
    Assembly {
        metal_mass: metal,
        removed_mass: removed,
        total_mass: total,
        center: (moment + handle * -0.06) / total,
        inertia: spin,
    }
}

pub fn assembly(material: &Material) -> Assembly {
    machine_profile(material, 0.0001)
}

/// One slice of a real blank. Coordinates are metres in the cold drawing.
#[derive(Clone, Copy, Debug)]
pub struct SolidCell {
    pub x0: f64,
    pub length: f64,
    pub width: f64,
    pub thickness: f64,
    pub fill: f64,
    pub temperature: f64,
    /// Finished edge thickness. Zero means the section is still a rectangle.
    pub edge: f64,
}

#[derive(Clone, Debug)]
pub struct Bevel {
    pub cells: Vec<SolidCell>,
    pub mass: f64,
    pub removed: f64,
    pub center: f64,
    pub inertia: f64,
}

/// Lumped cool of the actual mass from the mass-weighted cell temperature to 300 °C.
/// The field is not reset to 1040 °C. This is not a cell-by-cell continuation.
pub fn cool_lumped(
    cells: &[SolidCell],
    mass: f64,
    film: f64,
    environment: f64,
    material: &Material,
) -> (Vec<SolidCell>, f64) {
    let mut out = cells.to_vec();
    let volume = |cell: &SolidCell| {
        if cell.fill <= 0. || cell.thickness <= 0. {
            0.
        } else {
            cell.fill * cell.length * cell.width * cell.thickness
        }
    };
    let total: f64 = out.iter().map(volume).sum();
    if total <= 0. || mass <= 0. {
        return (out, 0.);
    }
    let start = out
        .iter()
        .map(|cell| cell.temperature * volume(cell))
        .sum::<f64>()
        / total;
    if start <= 300. {
        return (out, 0.);
    }
    let area = out
        .iter()
        .map(|cell| {
            let filled = volume(cell);
            if filled <= 0. {
                0.
            } else {
                let skin = cell.fill.clamp(0., 1.);
                2. * cell.length * cell.width * skin
                    + 2. * cell.length * cell.thickness * skin
                    + cell.width * cell.thickness * skin
            }
        })
        .sum::<f64>()
        .max(1e-6);
    let mut seconds = 0.;
    let mut temperature = start;
    let through = material.specific_heat + material.latent / (material.liquidus - material.solidus);
    if temperature > material.liquidus {
        seconds += lumped_cool(
            mass,
            area,
            film,
            environment,
            temperature,
            material.liquidus,
            material.specific_heat,
        );
        temperature = material.liquidus;
    }
    if temperature > material.solidus {
        seconds += lumped_cool(
            mass,
            area,
            film,
            environment,
            temperature,
            material.solidus,
            through,
        );
        temperature = material.solidus;
    }
    if temperature > 300. {
        seconds += lumped_cool(
            mass,
            area,
            film,
            environment,
            temperature,
            300.,
            material.specific_heat,
        );
    }
    if !seconds.is_finite() || seconds < 0. {
        seconds = 0.;
    }
    for cell in &mut out {
        if volume(cell) > 0. {
            cell.temperature = 300.;
        }
    }
    (out, seconds)
}

/// Bevel existing metal. Empty cells stay empty. A second call removes nothing.
/// No handle is added: a grip that was not poured is not invented.
pub fn bevel_cells(
    cells: &[SolidCell],
    edge: f64,
    mass: f64,
    solidus: f64,
) -> Result<Bevel, &'static str> {
    if edge <= 0. || !edge.is_finite() {
        return Err("Нет такой кромки");
    }
    let occupied = |cell: &SolidCell| cell.fill > 0. && cell.thickness > 0.;
    if cells
        .iter()
        .any(|cell| occupied(cell) && cell.temperature > solidus + 0.5)
    {
        return Err("Сначала остудите заготовку");
    }
    if cells.iter().any(|cell| occupied(cell) && cell.edge > 0.) {
        return Ok(bevel_report(cells, mass, 0.));
    }
    if cells
        .iter()
        .any(|cell| occupied(cell) && edge >= cell.thickness)
    {
        return Err("Кромка толще заготовки");
    }
    let mut next = cells.to_vec();
    let mut before = 0.;
    let mut after = 0.;
    for cell in &next {
        if !occupied(cell) {
            continue;
        }
        let volume = cell.fill * cell.length * cell.width * cell.thickness;
        before += volume;
        after += volume * (3. * cell.thickness + edge) / (4. * cell.thickness);
    }
    if before <= 0. || mass <= 0. {
        return Err("Снимать нечего");
    }
    let removed = mass * (1. - after / before);
    for cell in &mut next {
        if occupied(cell) {
            cell.edge = edge;
        }
    }
    Ok(bevel_report(&next, mass - removed, removed))
}

/// Centre of mass along the drawing and inertia about that centre. A grip that was not poured is not included.
pub fn mass_axes(cells: &[SolidCell], mass: f64) -> (f64, f64) {
    let report = bevel_report(cells, mass, 0.);
    (report.center, report.inertia)
}

fn bevel_report(cells: &[SolidCell], mass: f64, removed: f64) -> Bevel {
    let mut moment = 0.;
    let mut spin = 0.;
    let mut metal = 0.;
    let share: f64 = cells
        .iter()
        .map(|cell| {
            if cell.fill <= 0. || cell.thickness <= 0. {
                0.
            } else {
                let factor = if cell.edge > 0. {
                    (3. * cell.thickness + cell.edge) / (4. * cell.thickness)
                } else {
                    1.
                };
                cell.fill * cell.length * cell.width * cell.thickness * factor
            }
        })
        .sum();
    for cell in cells {
        if cell.fill <= 0. || cell.thickness <= 0. || share <= 0. {
            continue;
        }
        let factor = if cell.edge > 0. {
            (3. * cell.thickness + cell.edge) / (4. * cell.thickness)
        } else {
            1.
        };
        let part = mass * cell.fill * cell.length * cell.width * cell.thickness * factor / share;
        let mid = cell.x0 + 0.5 * cell.length;
        moment += part * mid;
        metal += part;
    }
    let center = if metal > 0. { moment / metal } else { 0. };
    for cell in cells {
        if cell.fill <= 0. || cell.thickness <= 0. || share <= 0. {
            continue;
        }
        let factor = if cell.edge > 0. {
            (3. * cell.thickness + cell.edge) / (4. * cell.thickness)
        } else {
            1.
        };
        let part = mass * cell.fill * cell.length * cell.width * cell.thickness * factor / share;
        let mid = cell.x0 + 0.5 * cell.length;
        spin += part * (mid - center).powi(2)
            + part * (cell.width.powi(2) + cell.thickness.powi(2)) / 12.;
    }
    Bevel {
        cells: cells.to_vec(),
        mass,
        removed,
        center,
        inertia: spin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crafting_physics::{c90700, geometry::drawing_totals, reference_sword};

    #[test]
    fn lumped_cooling_matches_the_uniform_body() {
        let area = drawing_totals(0.006).1 * 1.015_f64.powi(2);
        let mass = 1.428_633;
        let film = 60.;
        let mold = 200.;
        let c = 380.;
        let c_eff = 380. + 180_000. / 168.;
        let to_liquidus = lumped_cool(mass, area, film, mold, 1040., 999., c);
        let through = lumped_cool(mass, area, film, mold, 999., 831., c_eff);
        let to_warm = lumped_cool(mass, area, film, mold, 831., 300., c);
        assert!((to_liquidus - 6.88).abs() < 0.05, "{to_liquidus}");
        assert!(
            (to_liquidus + through - 130.87).abs() < 0.15,
            "{}",
            to_liquidus + through
        );
        assert!((to_liquidus + through + to_warm - 384.18).abs() < 0.3);
        let air = lumped_cool(mass, drawing_totals(0.006).1, 20., 20., 300., 40., c);
        assert!((air - 1121.61).abs() < 0.5, "{air}");
    }

    #[test]
    fn machining_keeps_the_published_mass_and_balance() {
        let part = assembly(&c90700());
        assert!(
            (part.metal_mass - 1.108_473).abs() < 1e-4,
            "{}",
            part.metal_mass
        );
        assert!(
            (part.removed_mass - 0.320_160).abs() < 1e-4,
            "{}",
            part.removed_mass
        );
        assert!(
            (part.total_mass - 1.288_473).abs() < 1e-4,
            "{}",
            part.total_mass
        );
        assert!((part.center - 0.203_258).abs() < 1e-4, "{}", part.center);
        assert!((part.inertia - 0.148_953).abs() < 1e-4, "{}", part.inertia);
    }

    #[test]
    fn an_unfed_short_fill_does_not_invent_a_dense_blank() {
        let mold = reference_sword(0.0025);
        let account = feed_account(&mold, &c90700(), 105.26e-6, FeedMode::IdealFeeder);
        assert_eq!(account.mode, FeedMode::Unresolved);
        assert_eq!(account.feed_mass, 0.);
        assert!(account.unfilled_volume > 60e-6);
        let full = feed_account(
            &mold,
            &c90700(),
            mold.cells.iter().map(|c| c.volume).sum(),
            FeedMode::IdealFeeder,
        );
        assert_eq!(full.mode, FeedMode::IdealFeeder);
        assert!(
            (full.feed_mass - 0.065_905).abs() < 1e-3,
            "{}",
            full.feed_mass
        );
    }

    #[test]
    fn lumped_cool_starts_from_the_cells_and_keeps_the_mass() {
        let cell = SolidCell {
            x0: 0.,
            length: 0.1,
            width: 0.04,
            thickness: 0.006,
            fill: 1.,
            temperature: 1004.,
            edge: 0.,
        };
        let (cooled, seconds) = cool_lumped(&[cell], 1.2, 60., 200., &c90700());
        assert!(seconds > 0.);
        assert!((cooled[0].temperature - 300.).abs() < 1e-9);
        let (again, more) = cool_lumped(&cooled, 1.2, 60., 200., &c90700());
        assert_eq!(more, 0.);
        assert!((again[0].temperature - 300.).abs() < 1e-9);
        let from_hotter = cool_lumped(
            &[SolidCell {
                temperature: 1040.,
                ..cell
            }],
            1.2,
            60.,
            200.,
            &c90700(),
        )
        .1;
        assert!(from_hotter > seconds);
    }

    #[test]
    fn a_bevel_removes_only_metal_that_is_there() {
        let full = SolidCell {
            x0: 0.,
            length: 0.1,
            width: 0.04,
            thickness: 0.006,
            fill: 1.,
            temperature: 300.,
            edge: 0.,
        };
        let empty = SolidCell {
            x0: 0.1,
            fill: 0.,
            temperature: 0.,
            ..full
        };
        let mut molten = full;
        molten.temperature = 1040.;
        assert!(bevel_cells(&[molten], 0.0001, 1., 831.).is_err());
        let mut thin = full;
        thin.thickness = 0.0001;
        assert!(
            bevel_cells(&[thin], 0.0001, 1., 831.)
                .unwrap_err()
                .contains("толще")
        );
        let cut = bevel_cells(&[full, empty], 0.0001, 1., 831.).unwrap();
        let remain = (3. * 0.006 + 0.0001) / (4. * 0.006);
        assert!((cut.mass - remain).abs() < 1e-9, "{}", cut.mass);
        assert!((cut.removed - (1. - remain)).abs() < 1e-9);
        assert_eq!(cut.cells[1].fill, 0.);
        assert_eq!(cut.cells[1].edge, 0.);
        assert!((cut.cells[0].edge - 0.0001).abs() < 1e-12);
        let twice = bevel_cells(&cut.cells, 0.0001, cut.mass, 831.).unwrap();
        assert_eq!(twice.removed, 0.);
        assert!((twice.mass - cut.mass).abs() < 1e-12);
    }
}
