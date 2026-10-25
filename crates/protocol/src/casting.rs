//! Casting session messages. The client sends a drawing and actions.
//! Temperature, mass and defects in the reply are the server's result.
use serde::{Deserialize, Serialize};

use crate::transfer::InventoryUpdate;

/// Solver edition stored on a blueprint and on a finished casting.
pub const SOLVER_VERSION: u32 = 1;
pub const MATERIAL_ID: &str = "C90700";
pub const MATERIAL_VERSION: u32 = 1;
/// Physical seconds are not sped up. Server downtime does not advance them.
pub const TIME_SCALE: u32 = 1;
/// One stock unit of C90700. Warehouse mass stays an integer number of milligrams.
pub const BRONZE_MG: u64 = 100_000;
pub const MAX_POINTS: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Draft,
    Charged,
    Hot,
    Pouring,
    Poured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectKind {
    /// Unfilled cavity ahead of the front. Not a cold shut.
    Misrun,
    /// The cell that first became solid on the only feed path.
    SolidifiedFeed,
    /// The crucible ran out before the cavity was full.
    ShortCharge,
    /// The player closed the feed before the cavity was full. Not a plug.
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub x0_mm: i32,
    pub x1_mm: i32,
    pub width_mm: u16,
    pub thickness_mm: u16,
    /// 0 is empty metal, 1000 is a full cell.
    pub fill_milli: u16,
    pub temperature_c: i16,
    /// Finished edge of the `[left, right]` side, in tenths of a millimetre.
    /// Zero keeps that side a rectangle. See [`Side`] for which side is which.
    #[serde(default)]
    pub edges_tenth_mm: [u16; 2],
}

/// Long side of the blank. Looking from the tang toward the tip,
/// `Left` is +y of the drawing and `Right` is −y.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Both,
    Left,
    Right,
}
impl Side {
    /// `[left, right]` sides this pass touches.
    pub fn sides(self) -> [bool; 2] {
        match self {
            Self::Both => [true, true],
            Self::Left => [true, false],
            Self::Right => [false, true],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defect {
    pub kind: DefectKind,
    pub x0_mm: i32,
    pub x1_mm: i32,
    pub time_ms: u64,
    pub volume_mm3: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Store the drawing. This does not consume metal.
    Save {
        revision: u64,
        points: Vec<[i32; 2]>,
        thickness_mm: Vec<u16>,
    },
    /// Move bronze stock into the crucible. It can no longer be sold.
    Charge { revision: u64, units: u16 },
    /// Heat the reserved metal. The target is a request, not a measured result.
    Heat { revision: u64, target_c: u16 },
    /// Fill the saved cavity. `gate_mm2` is the inlet. `ideal_feed` is the
    /// explicit unmodelled feeder, not a solved neck.
    Pour {
        revision: u64,
        gate_mm2: u16,
        ideal_feed: bool,
        /// 0 pours until the cavity, a plug, or an empty crucible.
        /// A positive value closes the feed after that many physical seconds.
        #[serde(default)]
        seconds: u16,
    },
    /// Lumped cool of one blank down to 300 °C. Does not change its mass.
    Cool { item: i64 },
    /// Grind a bevel on solid metal down to `edge_tenth_mm` on the chosen side, inside
    /// `from_mm..to_mm` of the drawing (the whole blank when absent). A pass only thins an
    /// edge; it never adds metal, returns stock or adds a grip.
    Machine {
        item: i64,
        edge_tenth_mm: u16,
        #[serde(default)]
        side: Side,
        #[serde(default)]
        from_mm: Option<i32>,
        #[serde(default)]
        to_mm: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub revision: u64,
    pub phase: Phase,
    pub pourable: bool,
    pub message: String,
    pub charge_mg: u64,
    pub temperature_c: Option<i16>,
    pub spans: Vec<Span>,
    pub defects: Vec<Defect>,
    pub result_item: Option<i64>,
    /// Centre of mass along the drawing, in tenths of a millimetre. Empty when this reply has no blank.
    #[serde(default)]
    pub center_tenth_mm: Option<i32>,
    /// Inertia about that centre, in mg·m². Empty when unknown. Not a combat bonus.
    #[serde(default)]
    pub inertia_mg_m2: Option<u64>,
    #[serde(default)]
    pub update: Option<Box<InventoryUpdate>>,
}

/// Closed outline of the published sword, in millimetres. It is a starting
/// drawing the player can edit, not a catalogue weapon.
pub fn example_sword() -> (Vec<[i32; 2]>, Vec<u16>) {
    let mut forward = Vec::new();
    let mut back = Vec::new();
    let mut x = -120;
    while x <= 650 {
        let width = if x < 0 {
            20
        } else if x <= 600 {
            50 - x / 30
        } else {
            30 * (650 - x) / 50
        };
        forward.push([x, -width / 2]);
        back.push([x, width / 2]);
        x += 50;
    }
    back.reverse();
    forward.extend(back);
    (forward, vec![6, 6, 6])
}

/// One filled cell of a blank, in metres. The length axis is centred on the whole mold,
/// including cells that stayed empty, so a short pour does not look like a complete sword.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlankBox {
    pub center_m: [f32; 3],
    pub size_m: [f32; 3],
}

pub fn blank_boxes(spans: &[Span]) -> Vec<BlankBox> {
    let Some(min_x) = spans.iter().map(|span| span.x0_mm).min() else {
        return Vec::new();
    };
    let Some(max_x) = spans.iter().map(|span| span.x1_mm).max() else {
        return Vec::new();
    };
    let origin = (f64::from(min_x) + f64::from(max_x)) / 2000.;
    let mut boxes = Vec::new();
    for span in spans {
        let fill = f64::from(span.fill_milli) / 1000.;
        if fill <= 0. {
            continue;
        }
        let length = f64::from(span.x1_mm - span.x0_mm) / 1000. * fill.min(1.);
        let width = f64::from(span.width_mm) / 1000.;
        let thick = f64::from(span.thickness_mm) / 1000.;
        if length <= 0. || width <= 0. || thick <= 0. {
            continue;
        }
        let mid = f64::from(span.x0_mm) / 1000. + length / 2.;
        boxes.push(BlankBox {
            center_m: [(mid - origin) as f32, (thick / 2.) as f32, 0.],
            size_m: [length as f32, thick as f32, width as f32],
        });
    }
    boxes
}

/// Triangle shell of the filled metal. Each stored edge thins the outer quarter of the width on
/// its own side; the middle half stays at the full thickness. +z follows +y of the drawing, so
/// the left side is +z. Empty cells are omitted, and the length axis stays centred on the whole mold.
pub fn blank_shell(spans: &[Span]) -> Vec<[f32; 3]> {
    let Some(min_x) = spans.iter().map(|span| span.x0_mm).min() else {
        return Vec::new();
    };
    let Some(max_x) = spans.iter().map(|span| span.x1_mm).max() else {
        return Vec::new();
    };
    let origin = (f64::from(min_x) + f64::from(max_x)) / 2000.;
    let mut vertices = Vec::new();
    for span in spans {
        let fill = f64::from(span.fill_milli) / 1000.;
        if fill <= 0. {
            continue;
        }
        let length = f64::from(span.x1_mm - span.x0_mm) / 1000. * fill.min(1.);
        let width = f64::from(span.width_mm) / 1000.;
        let thick = f64::from(span.thickness_mm) / 1000.;
        if length <= 0. || width <= 0. || thick <= 0. {
            continue;
        }
        let mid = f64::from(span.x0_mm) / 1000. + length / 2. - origin;
        let x0 = mid as f32 - length as f32 / 2.;
        let x1 = mid as f32 + length as f32 / 2.;
        let [left, right] = span.edges_tenth_mm.map(|edge| {
            let edge = f64::from(edge) / 10_000.;
            if edge > 0. && edge < thick {
                edge as f32
            } else {
                thick as f32
            }
        });
        let thick = thick as f32;
        let half = width as f32 / 2.;
        if (left - thick).abs() < 1e-8 && (right - thick).abs() < 1e-8 {
            push_section(&mut vertices, x0, x1, -half, half, thick, thick);
        } else {
            let quarter = half / 2.;
            push_section(&mut vertices, x0, x1, -quarter, quarter, thick, thick);
            push_section(&mut vertices, x0, x1, -half, -quarter, right, thick);
            push_section(&mut vertices, x0, x1, quarter, half, thick, left);
        }
    }
    vertices
}

/// One prism along X. `z0`/`z1` are the width edges; the tops there are `top0` and `top1`.
/// The bottom sits on y = 0.
fn push_section(out: &mut Vec<[f32; 3]>, x0: f32, x1: f32, z0: f32, z1: f32, top0: f32, top1: f32) {
    let p = |sx: f32, sy: f32, sz: f32| {
        [
            if sx < 0. { x0 } else { x1 },
            if sy < 0. {
                0.
            } else if sz < 0. {
                top0
            } else {
                top1
            },
            if sz < 0. { z0 } else { z1 },
        ]
    };
    let mut quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| {
        out.extend([a, b, c, a, c, d]);
    };
    quad(
        p(-1., 1., 1.),
        p(1., 1., 1.),
        p(1., 1., -1.),
        p(-1., 1., -1.),
    );
    quad(
        p(-1., -1., -1.),
        p(1., -1., -1.),
        p(1., -1., 1.),
        p(-1., -1., 1.),
    );
    quad(
        p(1., -1., -1.),
        p(1., 1., -1.),
        p(1., 1., 1.),
        p(1., -1., 1.),
    );
    quad(
        p(-1., -1., 1.),
        p(-1., 1., 1.),
        p(-1., 1., -1.),
        p(-1., -1., -1.),
    );
    quad(
        p(-1., -1., 1.),
        p(1., -1., 1.),
        p(1., 1., 1.),
        p(-1., 1., 1.),
    );
    quad(
        p(1., -1., -1.),
        p(-1., -1., -1.),
        p(-1., 1., -1.),
        p(1., 1., -1.),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(x0: i32, x1: i32, fill: u16) -> Span {
        Span {
            x0_mm: x0,
            x1_mm: x1,
            width_mm: 20,
            thickness_mm: 6,
            fill_milli: fill,
            temperature_c: 300,
            edges_tenth_mm: [0, 0],
        }
    }

    fn top_at(shell: &[[f32; 3]], keep: impl Fn(f32) -> bool) -> f32 {
        shell
            .iter()
            .filter(|p| keep(p[2]))
            .map(|p| p[1])
            .fold(0_f32, f32::max)
    }

    #[test]
    fn an_empty_span_leaves_a_gap_in_the_blank() {
        let boxes = blank_boxes(&[cell(0, 10, 1000), cell(10, 30, 0)]);
        assert_eq!(boxes.len(), 1);
        assert!((boxes[0].size_m[0] - 0.01).abs() < 1e-5);
        assert!((boxes[0].size_m[1] - 0.006).abs() < 1e-5);
        assert!((boxes[0].size_m[2] - 0.02).abs() < 1e-5);
        assert!((boxes[0].center_m[0] + 0.010).abs() < 1e-5);
    }

    #[test]
    fn a_stored_edge_thins_the_sides_and_leaves_the_middle() {
        let mut solid = cell(0, 10, 1000);
        solid.edges_tenth_mm = [20, 20];
        let shell = blank_shell(&[solid, cell(10, 30, 0)]);
        assert!(!shell.is_empty());
        assert!(shell.iter().all(|p| p[0] <= 0. && p[0] >= -0.02));
        let middle = top_at(&shell, |z| z.abs() <= 0.0051);
        let outer = top_at(&shell, |z| z.abs() >= 0.009);
        assert!((middle - 0.006).abs() < 1e-5, "{middle}");
        assert!((outer - 0.002).abs() < 1e-5, "{outer}");
        let plain = blank_shell(&[cell(0, 10, 1000)]);
        assert!(
            plain
                .iter()
                .all(|p| (p[1] - 0.006).abs() < 1e-5 || p[1].abs() < 1e-5)
        );
    }

    #[test]
    fn one_ground_side_leaves_the_other_square() {
        let mut solid = cell(0, 10, 1000);
        solid.edges_tenth_mm = [20, 0];
        let shell = blank_shell(&[solid]);
        let left = top_at(&shell, |z| z >= 0.009);
        let right = top_at(&shell, |z| z <= -0.009);
        assert!((left - 0.002).abs() < 1e-5, "{left}");
        assert!((right - 0.006).abs() < 1e-5, "{right}");
    }

    #[test]
    fn a_pass_names_its_sides_and_defaults_to_both() {
        let machine: Command = serde_json::from_str(
            r#"{"action":"machine","item":7,"edge_tenth_mm":5,"side":"left","from_mm":0,"to_mm":300}"#,
        )
        .unwrap();
        assert!(matches!(
            machine,
            Command::Machine {
                side: Side::Left,
                from_mm: Some(0),
                to_mm: Some(300),
                ..
            }
        ));
        let whole: Command =
            serde_json::from_str(r#"{"action":"machine","item":7,"edge_tenth_mm":5}"#).unwrap();
        assert!(matches!(
            whole,
            Command::Machine {
                side: Side::Both,
                from_mm: None,
                to_mm: None,
                ..
            }
        ));
        assert!(
            serde_json::from_str::<Span>(
                r#"{"x0_mm":0,"x1_mm":1,"width_mm":1,"thickness_mm":1,"fill_milli":0,"temperature_c":0,"edge_tenth_mm":1}"#
            )
            .is_err(),
            "a stored span must be migrated to edges_tenth_mm"
        );
    }
}
