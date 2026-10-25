//! One-path sword cavity used as the numerical fixture.
//! Drawing coordinates are the cold design. The mold multiplies every length by `scale`.

use crate::crafting_physics::enthalpy::Material;

/// Cold drawing width of the reference sword, metres. Outside the part the width is zero.
pub fn sword_width(x: f64) -> f64 {
    if (-0.12..0.).contains(&x) {
        0.02
    } else if (0. ..=0.60).contains(&x) {
        0.05 - x / 30.
    } else if (0.60..=0.65).contains(&x) {
        0.03 * (0.65 - x) / 0.05
    } else {
        0.
    }
}

#[derive(Clone, Debug)]
pub struct Cell {
    /// Drawing-space interval, before the mold allowance.
    pub x0: f64,
    pub x1: f64,
    /// Mold volume, m³.
    pub volume: f64,
    /// Average mold cross-section, m².
    pub section: f64,
    /// Mold length along the flow, m.
    pub length: f64,
    pub wide: f64,
    pub narrow: f64,
    /// Top, bottom and tapered sides, full cell, m². Shared faces are not included.
    pub wall: f64,
    /// Shoulder where the blade is wider than the tang, m². Zero elsewhere.
    pub shoulder: f64,
}

#[derive(Clone, Debug)]
pub struct Mold {
    pub scale: f64,
    pub thickness: f64,
    pub cells: Vec<Cell>,
}

/// Reference sword split so tang, blade and tip boundaries fall on cell edges.
pub fn reference_sword(pitch: f64) -> Mold {
    let scale = 1.015;
    let thickness = 0.006;
    let mut cells = Vec::new();
    for (start, end, part) in [
        (-0.12, 0., Part::Tang),
        (0., 0.60, Part::Blade),
        (0.60, 0.65, Part::Tip),
    ] {
        let n = ((end - start) / pitch).round() as i32;
        assert!(
            n > 0 && ((end - start) - f64::from(n) * pitch).abs() < 1e-9,
            "pitch {pitch} does not meet the sword joints"
        );
        for i in 0..n {
            let x0 = start + f64::from(i) * pitch;
            let x1 = x0 + pitch;
            cells.push(cell(part, x0, x1, thickness, scale));
        }
    }
    Mold {
        scale,
        thickness,
        cells,
    }
}

#[derive(Clone, Copy)]
enum Part {
    Tang,
    Blade,
    Tip,
}

fn part_width(part: Part, x: f64) -> f64 {
    match part {
        // The root is a step: the tang stays 20 mm and the blade starts at 50 mm.
        Part::Tang => 0.02,
        Part::Blade => 0.05 - x / 30.,
        Part::Tip => 0.03 * (0.65 - x) / 0.05,
    }
}

fn cell(part: Part, x0: f64, x1: f64, thickness: f64, scale: f64) -> Cell {
    let w0 = part_width(part, x0);
    let w1 = part_width(part, x1);
    let dx = x1 - x0;
    let average = 0.5 * (w0 + w1);
    let volume = average * thickness * dx * scale.powi(3);
    let section = average * thickness * scale.powi(2);
    let length = dx * scale;
    let width = average * scale;
    let thick = thickness * scale;
    let (wide, narrow) = if width >= thick {
        (width, thick)
    } else {
        (thick, width)
    };
    let edge = (dx * dx + (0.5 * (w1 - w0)).powi(2)).sqrt();
    let wall = (2. * average * dx + 2. * thickness * edge) * scale.powi(2);
    let shoulder = if x0 == 0. {
        (sword_width(0.) - 0.02) * thickness * scale.powi(2)
    } else {
        0.
    };
    Cell {
        x0,
        x1,
        volume,
        section,
        length,
        wide,
        narrow,
        wall,
        shoulder,
    }
}

/// One sequential cavity from drawing-space slices `[x0, x1, width0, width1, thickness]`, metres.
/// A wider next slice gets a shoulder wall. This is the supported topology, not a general solid.
pub fn extrude(slices: &[[f64; 5]]) -> Mold {
    let scale: f64 = 1.015;
    let mut cells = Vec::new();
    let mut previous = None;
    for slice in slices {
        let [x0, x1, w0, w1, thickness] = *slice;
        let dx = x1 - x0;
        let average = 0.5 * (w0 + w1);
        let volume = average * thickness * dx * scale.powi(3);
        let section = average * thickness * scale.powi(2);
        let length = dx * scale;
        let width = average * scale;
        let thick = thickness * scale;
        let (wide, narrow) = if width >= thick {
            (width, thick)
        } else {
            (thick, width)
        };
        let edge = (dx * dx + (0.5 * (w1 - w0)).powi(2)).sqrt();
        let wall = (2. * average * dx + 2. * thickness * edge) * scale.powi(2);
        let shoulder = previous
            .map(|end: f64| (w0 - end).max(0.) * thickness * scale.powi(2))
            .unwrap_or(0.);
        previous = Some(w1);
        cells.push(Cell {
            x0,
            x1,
            volume,
            section,
            length,
            wide,
            narrow,
            wall,
            shoulder,
        });
    }
    Mold {
        scale,
        thickness: slices.first().map(|s| s[4]).unwrap_or(0.006),
        cells,
    }
}

/// Cold drawing volume and the surface used by the lumped cooling estimate.
pub fn drawing_totals(material_thickness: f64) -> (f64, f64) {
    let tang = 0.12 * 0.02 * material_thickness;
    let blade = 0.60 * (0.05 + 0.03) / 2. * material_thickness;
    let tip = 0.05 * 0.03 / 2. * material_thickness;
    let volume = tang + blade + tip;
    let plan = tang / material_thickness + blade / material_thickness + tip / material_thickness;
    let blade_edge = (0.60_f64.powi(2) + 0.01_f64.powi(2)).sqrt();
    let tip_edge = (0.05_f64.powi(2) + 0.015_f64.powi(2)).sqrt();
    let sides = 2. * material_thickness * (0.12 + blade_edge + tip_edge);
    let inlet = 0.02 * material_thickness;
    let shoulder = (0.05 - 0.02) * material_thickness;
    (volume, 2. * plan + sides + inlet + shoulder)
}

pub fn cavity_volume(mold: &Mold) -> f64 {
    mold.cells.iter().map(|c| c.volume).sum()
}

/// Liquid mass of a full cavity, and the extra mass an ideal feeder must add
/// before the cold drawing can be dense. The feeder neck is not simulated.
pub fn shrinkage(mold: &Mold, material: &Material) -> (f64, f64) {
    let liquid = cavity_volume(mold) * material.rho_liquid;
    let cold = drawing_totals(mold.thickness).0 * material.rho_cold;
    (liquid, cold - liquid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crafting_physics::c90700;

    #[test]
    fn the_reference_cavity_has_the_published_volume_and_surface() {
        let (volume, area) = drawing_totals(0.006);
        assert!((volume - 0.000_162_9).abs() < 1e-12, "{volume}");
        assert!((area - 0.063_867_4).abs() < 1e-6, "{area}");
        let mold = reference_sword(0.0025);
        let hot = cavity_volume(&mold) * 1e6;
        assert!((hot - 170.341_007).abs() < 1e-4, "{hot}");
        let (liquid, feed) = shrinkage(&mold, &c90700());
        assert!((liquid - 1.362_728).abs() < 1e-4, "{liquid}");
        assert!((feed - 0.065_905).abs() < 1e-4, "{feed}");
    }
}
