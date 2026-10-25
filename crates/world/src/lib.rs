//! Metre-based fantasy hill town. Shared terrain, street plan and oriented collision.
//! `World::new()` deterministically builds the same map on client and server (no files, no randomness).
//! Client uses it for rendering (`scene.rs`), picking and camera; server for movement validation.
//! Heights: `height()` = natural terrain; `ground_height()` = drawn terrain mesh with flattened plots;
//! `surface_height()` = what you stand on (floors, stairs, ramps, bridge). Changing collision → bump protocol `WORLD_VERSION`.
pub mod navigation;
pub mod stations;
pub mod terrain;
use std::f32::consts::{PI, TAU};
use terrain::Plot;

/// Map spans ±8 km on X and Z; anything outside is rejected as invalid.
pub const WORLD_HALF: f32 = 8000.0;
/// City layout was designed at a larger size; `city()` shrinks design coordinates by this factor
/// (building sizes are not scaled, so people keep real proportions).
pub const CITY_SCALE: f32 = 0.66;
const fn city(p: [f32; 2]) -> [f32; 2] {
    [p[0] * CITY_SCALE, p[1] * CITY_SCALE]
}
/// Character collision radius (m): obstacles are inflated by this in all walk checks.
pub const PLAYER_RADIUS: f32 = 0.45;
pub const PLAZA: [f32; 2] = city([-70.0, 80.0]);
/// Plaza stall counter, next to the spawn point. All points here are `[x, z]`.
pub const PLAZA_STALL: [f32; 2] = [SPAWN[0] + 4. - 1., SPAWN[1]];
pub const SPAWN: [f32; 2] = city([-70.0, 120.0]);
/// City wall polygon (closed loop); `GATE_SEGMENTS` are the wall sides that have gates.
pub const WALL: [[f32; 2]; 17] = [
    city([-580., -160.]),
    city([-530., -360.]),
    city([-380., -510.]),
    city([-120., -610.]),
    city([120., -575.]),
    city([260., -470.]),
    city([340., -360.]),
    city([515., -245.]),
    city([630., -40.]),
    city([605., 150.]),
    city([485., 335.]),
    city([310., 430.]),
    city([80., 520.]),
    city([-145., 485.]),
    city([-380., 410.]),
    city([-545., 265.]),
    city([-625., 70.]),
];
const GATE_SEGMENTS: [usize; 4] = [2, 8, 11, 15];

/// One of the four large enterable civic buildings; `size` is [width, depth] in meters.
#[derive(Clone, Copy)]
pub struct HallSpec {
    pub name: &'static str,
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub yaw: f32,
    pub wall_height: f32,
}
pub const HALL_SPECS: [HallSpec; 4] = [
    HallSpec {
        name: "Верхняя крепость · Ратуша",
        center: city([115., -315.]),
        size: [72., 58.],
        yaw: -0.35,
        wall_height: 30.,
    },
    HallSpec {
        name: "Собор Звёздного света",
        center: city([-205., -125.]),
        size: [60., 90.],
        yaw: 0.3,
        wall_height: 36.,
    },
    HallSpec {
        name: "Нижние торговые ряды",
        center: city([125., 155.]),
        size: [78., 46.],
        yaw: -1.9,
        wall_height: 25.,
    },
    HallSpec {
        name: "Дом гильдий",
        center: city([-315., 220.]),
        size: [66., 52.],
        yaw: 2.0,
        wall_height: 27.,
    },
];

/// Smoothstep from 0 (x ≤ a) to 1 (x ≥ b); used for soft blending everywhere.
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
/// Gaussian bump: 1 at the center, fading with distance (`radius` controls width).
fn hill(x: f32, z: f32, cx: f32, cz: f32, radius: f32) -> f32 {
    (-((x - cx).powi(2) + (z - cz).powi(2)) / radius.powi(2)).exp()
}
/// River centerline X for a given Z: a wavy line around x = 850 m, east of the city.
pub fn river_x(z: f32) -> f32 {
    850. + 95. * (z * 0.004).sin() + 25. * (z * 0.009).sin()
}
/// Smooth, seeded value noise avoids repeating diagonal waves across the landscape.
pub fn noise(x: f32, z: f32, scale: f32, seed: u32) -> f32 {
    let x = x / scale;
    let z = z / scale;
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let hash = |x: i32, z: i32| {
        let mut v = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263) ^ seed;
        v = (v ^ (v >> 13)).wrapping_mul(1274126177);
        (v ^ (v >> 16)) as f32 / u32::MAX as f32 * 2. - 1.
    };
    let u = smooth(0., 1., x - x.floor());
    let v = smooth(0., 1., z - z.floor());
    let a = hash(ix, iz) * (1. - u) + hash(ix + 1, iz) * u;
    let b = hash(ix, iz + 1) * (1. - u) + hash(ix + 1, iz + 1) * u;
    a * (1. - v) + b * v
}
/// Irregular enclosing mountain belt: the steep inner face blocks travel before
/// the technical mesh boundary; the skyline continues beyond the impassable ridge.
pub fn mountain_inner_radius(angle: f32) -> f32 {
    6100. + 340. * (angle * 3. + 0.4).sin() + 190. * (angle * 7. - 0.7).sin()
}
/// Signed distance past the mountain belt's inner edge (positive = inside the mountains).
pub fn mountain_distance(x: f32, z: f32) -> f32 {
    x.hypot(z) - mountain_inner_radius(z.atan2(x))
}
/// Raw landscape: gentle town hills near the center, countryside farther out, river valley,
/// and a mountain ring beyond 3 km.
fn natural_height(x: f32, z: f32) -> f32 {
    let r = x.hypot(z);
    let country = 34.
        + 145. * hill(x, z, -1500., -1300., 750.)
        + 210. * hill(x, z, 2300., -2500., 1000.)
        + 130. * hill(x, z, -2600., 2600., 1100.)
        + 95. * hill(x, z, 2900., 1700., 1200.)
        + 26. * noise(x, z, 700., 17)
        + 8. * noise(x, z, 220., 71);
    // Broad neighbourhood rises, measured in metres; architecture keeps its real scale.
    // One higher civic hill, with lower shoulders and shallow valleys elsewhere.
    let town = 36. - z * 0.003
        + 6.0 * hill(x, z, 76., -208., 105.)
        + 4.0 * hill(x, z, -180., -95., 100.)
        + 3.5 * hill(x, z, 240., 90., 95.)
        + 3.0 * hill(x, z, -235., 190., 90.)
        - 1.5 * hill(x, z, 15., 225., 100.);
    // Town terrain within 500 m of the center, countryside beyond 1 km, blended between.
    let blend = smooth(500., 1000., r);
    let mut y = town * (1. - blend) + country * blend;
    let valley = (1. - smooth(20., 110., (x - river_x(z)).abs())) * (1. - smooth(4600., 5400., r));
    y = y * (1. - valley) + 5. * valley;
    if r < 3000. {
        return y;
    }
    // Overlapping irregular peaks form a chain, rather than extruding a circular
    // wall. Maxima preserve ridgelines and the valleys between individual massifs.
    let mut mountains = 0_f32;
    let mut foothills = 0_f32;
    for i in 0..23 {
        let i = i as f32;
        let a = i * TAU / 23. + 0.035 * (i * 2.71).sin();
        let radius = mountain_inner_radius(a) + 650. + 250. * (i * 1.83).sin();
        let width = 2150. + 300. * (i * 2.37).sin();
        let summit = 2000. + 650. * (i * 1.71 + 0.4).sin();
        let d = (x - radius * a.cos()).hypot(z - radius * a.sin());
        let peak = summit * (1. - d / width).max(0.).powf(1.3);
        mountains = mountains.max(peak);
        foothills += peak;
    }
    mountains += (foothills - mountains) * 0.2;
    let roughness = (mountains / 500.).clamp(0., 1.);
    y + mountains + roughness * (110. * noise(x, z, 190., 239) + 45. * noise(x, z, 65., 157))
}
/// World → object-local coordinates (inverse of `rotate`), so rotated boxes can be tested axis-aligned.
fn local(point: [f32; 2], center: [f32; 2], yaw: f32) -> [f32; 2] {
    let x = point[0] - center[0];
    let z = point[1] - center[1];
    [yaw.cos() * x - yaw.sin() * z, yaw.sin() * x + yaw.cos() * z]
}
/// Object-local offset (x, z) around `center` rotated by `yaw` → world `[x, z]`.
pub fn rotate(center: [f32; 2], x: f32, z: f32, yaw: f32) -> [f32; 2] {
    [
        center[0] + yaw.cos() * x + yaw.sin() * z,
        center[1] - yaw.sin() * x + yaw.cos() * z,
    ]
}
/// Natural terrain plus flattened plaza and hall platforms. Not the walkable surface:
/// use `World::surface_height` for that.
pub fn height(x: f32, z: f32) -> f32 {
    let mut y = natural_height(x, z);
    // Only the gathering space and occupied hall floors are graded; the city itself stays hilly.
    let r = ((x - PLAZA[0]) / 92.).hypot((z - PLAZA[1]) / 73.);
    let plaza_weight = 1. - smooth(0.88, 1.3, r);
    y = y * (1. - plaza_weight) + natural_height(PLAZA[0], PLAZA[1]) * plaza_weight;
    for spec in HALL_SPECS {
        let p = local([x, z], rotate(spec.center, 0., 8., spec.yaw), spec.yaw);
        let outside =
            (p[0].abs() - spec.size[0] / 2. - 6.).max(p[1].abs() - spec.size[1] / 2. - 26.);
        let w = 1. - smooth(0., 35., outside);
        y = y * (1. - w) + natural_height(spec.center[0], spec.center[1]) * w;
    }
    y
}
/// Point-in-polygon test against `WALL` (ray-crossing count).
pub fn inside_city(p: [f32; 2]) -> bool {
    let mut inside = false;
    for i in 0..WALL.len() {
        let a = WALL[i];
        let b = WALL[(i + 1) % WALL.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
pub fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
pub fn nearest_on_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let dx = b[0] - a[0];
    let dz = b[1] - a[1];
    let t =
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / (dx * dx + dz * dz).max(0.001)).clamp(0., 1.);
    [a[0] + t * dx, a[1] + t * dz]
}

/// Collision box rotated around Y: `center`/`half`/`yaw` describe the footprint,
/// `min`/`max` are a world-axis bounding box (Y range used for sight checks).
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub center: [f32; 2],
    pub half: [f32; 2],
    pub yaw: f32,
}
impl Bounds {
    fn blocks_sight(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let axz = local([a[0], a[2]], self.center, self.yaw);
        let bxz = local([b[0], b[2]], self.center, self.yaw);
        let start = [axz[0], a[1], axz[1]];
        let end = [bxz[0], b[1], bxz[1]];
        let lo = [-self.half[0], self.min[1], -self.half[1]];
        let hi = [self.half[0], self.max[1], self.half[1]];
        let mut near = 0_f32;
        let mut far = 1_f32;
        for i in 0..3 {
            let d = end[i] - start[i];
            if d.abs() < 0.000001 {
                if start[i] < lo[i] || start[i] > hi[i] {
                    return false;
                }
            } else {
                let t0 = (lo[i] - start[i]) / d;
                let t1 = (hi[i] - start[i]) / d;
                near = near.max(t0.min(t1));
                far = far.min(t0.max(t1));
                if near > far {
                    return false;
                }
            }
        }
        true
    }
    /// Exact sweep against the expanded oriented footprint. Sampling alone can
    /// miss a thin corner even when both endpoints are outside the obstacle.
    pub fn intersects_segment(&self, a: [f32; 2], b: [f32; 2], padding: f32) -> bool {
        let a = local(a, self.center, self.yaw);
        let b = local(b, self.center, self.yaw);
        let mut near = 0_f32;
        let mut far = 1_f32;
        for axis in 0..2 {
            let d = b[axis] - a[axis];
            let half = self.half[axis] + padding;
            if d.abs() < 0.000001 {
                if a[axis].abs() > half {
                    return false;
                }
            } else {
                let t0 = (-half - a[axis]) / d;
                let t1 = (half - a[axis]) / d;
                near = near.max(t0.min(t1));
                far = far.min(t0.max(t1));
                if near > far {
                    return false;
                }
            }
        }
        true
    }
    pub fn contains_xz(&self, x: f32, z: f32, padding: f32) -> bool {
        let p = local([x, z], self.center, self.yaw);
        p[0].abs() <= self.half[0] + padding && p[1].abs() <= self.half[1] + padding
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Surface {
    Stone,
    Plaster,
    Roof,
    Wood,
    Gold,
    Water,
    Paving,
    Brick,
    BluePlaster,
    RedRoof,
    GreenRoof,
    Glass,
    Banner,
    DarkStone,
}
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Box,
    Gable,
    Cylinder,
    Cone,
    Arch,
}
/// One drawable piece of city geometry (wall, roof, column…). `detail` = small decoration;
/// `roof_of` = hall whose interior hides this roof; `camera_solid` = camera can't pass through.
#[derive(Clone, Debug)]
pub struct Block {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub surface: Surface,
    pub shape: Shape,
    pub yaw: f32,
    pub tilt: f32,
    pub detail: bool,
    pub roof_of: Option<usize>,
    pub camera_solid: bool,
    /// Procedural stand-in kept for collision and camera; a reference model is drawn instead.
    pub hidden: bool,
}
/// Enterable interior. The first `HALL_SPECS.len()` entries are civic halls; the rest are house interiors.
#[derive(Clone, Debug)]
pub struct Hall {
    pub name: &'static str,
    pub center: [f32; 2],
    pub half: [f32; 2],
    pub entrance: [f32; 2],
    pub yaw: f32,
}
#[derive(Clone, Debug)]
pub struct Street {
    pub points: Vec<[f32; 2]>,
    pub width: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct BuildingSite {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub height: f32,
    pub yaw: f32,
    pub variant: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Bridge {
    pub center: [f32; 2],
    pub level: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Tree {
    pub position: [f32; 3],
    pub scale: f32,
    pub tint: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneryKind {
    Fountain,
    MarketStall,
    Counter,
    Anvil,
    Forge,
    BlacksmithSign,
    StreetLantern,
    WallLantern,
    Banner,
    Crates,
    SquarePlanter,
    LongPlanter,
    Fence,
    Cobblestone,
}
#[derive(Clone, Copy, Debug)]
pub struct Scenery {
    pub kind: SceneryKind,
    pub position: [f32; 3],
    pub yaw: f32,
}
/// Walkable slope: height goes from `base` to `base + rise` along its `length`.
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    pub center: [f32; 2],
    pub yaw: f32,
    pub width: f32,
    pub length: f32,
    pub base: f32,
    pub rise: f32,
}
type RoadSegment = ([f32; 2], [f32; 2], f32);
/// The whole generated map. `blocks` = what is drawn; `obstacles` = what blocks walking.
/// They overlap but are separate lists, so a visual change may not change collision.
#[derive(Clone, Debug)]
pub struct World {
    pub blocks: Vec<Block>,
    pub obstacles: Vec<Bounds>,
    pub halls: Vec<Hall>,
    pub streets: Vec<Street>,
    pub gates: Vec<[f32; 2]>,
    pub houses: usize,
    pub stairs: Vec<Bounds>,
    pub plots: Vec<Plot>,
    pub buildings: Vec<BuildingSite>,
    pub ramps: Vec<Ramp>,
    pub trees: Vec<Tree>,
    pub scenery: Vec<Scenery>,
    pub bridge: Option<Bridge>,
    // Spatial indexes on a 32 m grid: which road segments / obstacles touch each cell,
    // so collision checks don't scan every object.
    street_cells: std::collections::HashMap<(i32, i32), Vec<RoadSegment>>,
    obstacle_cells: std::collections::HashMap<(i32, i32), Vec<usize>>,
}
impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
impl World {
    /// Builds the entire map. Order matters: streets and plots first, then buildings that read them,
    /// and the obstacle index last. Fairly expensive; create once and share via `Arc`.
    pub fn new() -> Self {
        let mut w = Self {
            blocks: vec![],
            obstacles: vec![],
            halls: vec![],
            streets: vec![],
            gates: vec![],
            houses: 0,
            stairs: vec![],
            plots: vec![],
            buildings: vec![],
            ramps: vec![],
            trees: vec![],
            scenery: vec![],
            bridge: None,
            street_cells: Default::default(),
            obstacle_cells: Default::default(),
        };
        w.street_plan();
        // Reserve the building plots before constructing either terrain or architecture.
        for spec in HALL_SPECS {
            w.plots.push(Plot {
                center: rotate(spec.center, 0., 8., spec.yaw),
                half: [spec.size[0] / 2. + 6., spec.size[1] / 2. + 26.],
                yaw: spec.yaw,
                level: height(spec.center[0], spec.center[1]),
                blend: 35.,
            });
        }
        w.housing();
        w.fortifications();
        for (id, spec) in HALL_SPECS.into_iter().enumerate() {
            w.hall(spec, id);
        }
        w.fountain();
        for site in w.buildings.clone() {
            w.house(site.center, site.size, site.height, site.yaw, site.variant);
        }
        w.river_bridge();
        w.market_stalls();
        w.work_stations();
        w.forest();
        w.plaza_decor();
        // Register each obstacle in every 32 m cell its padded bounding box touches.
        for (id, b) in w.obstacles.iter().enumerate() {
            for x in ((b.min[0] - PLAYER_RADIUS) / 32.).floor() as i32
                ..=((b.max[0] + PLAYER_RADIUS) / 32.).floor() as i32
            {
                for z in ((b.min[2] - PLAYER_RADIUS) / 32.).floor() as i32
                    ..=((b.max[2] + PLAYER_RADIUS) / 32.).floor() as i32
                {
                    w.obstacle_cells.entry((x, z)).or_default().push(id);
                }
            }
        }
        w
    }
    /// Adds a block in world coordinates; `solid` also adds a collision obstacle.
    /// Gable/cone roofs always stop the camera even if not walk-solid.
    #[allow(clippy::too_many_arguments)]
    fn piece(
        &mut self,
        center: [f32; 3],
        size: [f32; 3],
        yaw: f32,
        surface: Surface,
        shape: Shape,
        solid: bool,
    ) -> usize {
        let id = self.blocks.len();
        self.blocks.push(Block {
            center,
            size,
            surface,
            shape,
            yaw,
            tilt: 0.,
            detail: false,
            roof_of: None,
            camera_solid: solid || matches!(shape, Shape::Gable | Shape::Cone),
            hidden: false,
        });
        if solid {
            let hx = yaw.cos().abs() * size[0] / 2. + yaw.sin().abs() * size[2] / 2.;
            let hz = yaw.sin().abs() * size[0] / 2. + yaw.cos().abs() * size[2] / 2.;
            self.obstacles.push(Bounds {
                min: [center[0] - hx, center[1] - size[1] / 2., center[2] - hz],
                max: [center[0] + hx, center[1] + size[1] / 2., center[2] + hz],
                center: [center[0], center[2]],
                half: [size[0] / 2., size[2] / 2.],
                yaw,
            });
        }
        id
    }
    fn conceal(&mut self, id: usize) {
        self.blocks[id].hidden = true;
    }
    fn scenery(&mut self, kind: SceneryKind, position: [f32; 3], yaw: f32) {
        self.scenery.push(Scenery {
            kind,
            position,
            yaw,
        });
    }
    /// Like `piece`, but `p` is an offset in a building's local frame (center `c`, ground `base`, `yaw`).
    #[allow(clippy::too_many_arguments)]
    fn part(
        &mut self,
        c: [f32; 2],
        base: f32,
        yaw: f32,
        p: [f32; 3],
        s: [f32; 3],
        surface: Surface,
        shape: Shape,
        solid: bool,
    ) -> usize {
        let at = rotate(c, p[0], p[2], yaw);
        self.piece([at[0], base + p[1], at[1]], s, yaw, surface, shape, solid)
    }
    fn street(&mut self, control: &[[f32; 2]], width: f32) {
        let mut points = vec![];
        for i in 0..control.len() - 1 {
            let a = control[i.saturating_sub(1)];
            let b = control[i];
            let c = control[i + 1];
            let d = control[(i + 2).min(control.len() - 1)];
            let steps = (distance(b, c) / 5.).ceil().max(2.) as usize;
            for n in 0..steps {
                let t = n as f32 / steps as f32;
                points.push(std::array::from_fn(|axis| {
                    0.5 * ((2. * b[axis])
                        + (-a[axis] + c[axis]) * t
                        + (2. * a[axis] - 5. * b[axis] + 4. * c[axis] - d[axis]) * t * t
                        + (-a[axis] + 3. * b[axis] - 3. * c[axis] + d[axis]) * t * t * t)
                }));
            }
        }
        points.push(*control.last().unwrap());
        for pair in points.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            for x in ((a[0].min(b[0]) - width) / 64.).floor() as i32
                ..=((a[0].max(b[0]) + width) / 64.).floor() as i32
            {
                for z in ((a[1].min(b[1]) - width) / 64.).floor() as i32
                    ..=((a[1].max(b[1]) + width) / 64.).floor() as i32
                {
                    self.street_cells
                        .entry((x, z))
                        .or_default()
                        .push((a, b, width));
                }
            }
        }
        self.streets.push(Street { points, width });
    }
    fn street_plan(&mut self) {
        let gates: Vec<_> = GATE_SEGMENTS
            .iter()
            .map(|i| {
                let a = WALL[*i];
                let b = WALL[(*i + 1) % WALL.len()];
                [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.]
            })
            .collect();
        self.gates = gates.clone();
        self.street(
            &[
                [-1700., -4800.],
                [-1300., -2800.],
                [-850., -1550.],
                [-480., -920.],
                gates[0],
                city([-220., -400.]),
                city([-290., -280.]),
                city([-325., -165.]),
                city([-320., -75.]),
                city([-275., -20.]),
                city([-180., -25.]),
                PLAZA,
                city([40., 245.]),
                city([70., 350.]),
                gates[2],
                [390., 790.],
                [210., 1100.],
                [380., 1750.],
                [180., 2800.],
                [-350., 4900.],
            ],
            20.,
        );
        self.street(
            &[
                PLAZA,
                city([60., 40.]),
                city([210., 0.]),
                city([340., 80.]),
                city([480., 80.]),
                gates[1],
                city([725., 45.]),
                [river_x(45.) - 100., 45.],
                [river_x(45.) + 100., 45.],
                [1400., 120.],
                [2600., -350.],
                [4400., 650.],
            ],
            22.,
        );
        self.street(
            &[
                PLAZA,
                city([-190., 120.]),
                city([-350., 70.]),
                city([-470., 180.]),
                gates[3],
                [-930., 240.],
                [-1500., 350.],
                [-2200., 120.],
                [-2900., 500.],
                [-4800., 950.],
            ],
            18.,
        );
        self.street(
            &[
                city([-220., -400.]),
                city([-70., -435.]),
                city([115., -410.]),
                city([290., -265.]),
                city([340., 80.]),
            ],
            13.,
        );
        self.street(
            &[
                city([-180., -25.]),
                city([-400., -50.]),
                city([-450., -185.]),
                city([-290., -280.]),
            ],
            11.,
        );
        self.street(
            &[
                city([-470., 180.]),
                city([-440., 320.]),
                city([-255., 360.]),
                city([-95., 315.]),
                city([70., 350.]),
            ],
            12.,
        );
        self.street(
            &[
                city([40., 245.]),
                city([190., 275.]),
                city([300., 175.]),
                city([340., 80.]),
            ],
            13.,
        );
        self.street(
            &[
                city([210., 0.]),
                city([160., -95.]),
                city([75., -140.]),
                city([70., -230.]),
            ],
            15.,
        );
        self.street(
            &[city([-350., 70.]), city([-370., -20.]), city([-290., -80.])],
            10.,
        );
        self.street(
            &[
                city([300., 175.]),
                city([430., 240.]),
                city([390., 355.]),
                city([225., 420.]),
            ],
            10.,
        );
        self.street(
            &[
                city([-70., -435.]),
                city([-25., -300.]),
                city([-55., -200.]),
                city([-80., -90.]),
                city([-180., -25.]),
            ],
            10.,
        );
        for spec in HALL_SPECS {
            let entrance = rotate(spec.center, 0., spec.size[1] / 2. + 18., spec.yaw);
            let nearest = self.nearest_street(entrance).0;
            self.street(
                &[
                    entrance,
                    [
                        (entrance[0] + nearest[0]) / 2.,
                        (entrance[1] + nearest[1]) / 2.,
                    ],
                    nearest,
                ],
                12.,
            );
        }
    }
    pub fn on_road(&self, x: f32, z: f32, padding: f32) -> bool {
        self.street_cells
            .get(&((x / 64.).floor() as i32, (z / 64.).floor() as i32))
            .is_some_and(|segments| {
                segments.iter().any(|(a, b, w)| {
                    distance([x, z], nearest_on_segment([x, z], *a, *b)) < w / 2. + padding
                })
            })
    }
    pub fn nearest_street(&self, p: [f32; 2]) -> ([f32; 2], f32, f32) {
        let mut best = ([0., 0.], f32::INFINITY, 0.);
        for street in &self.streets {
            for pair in street.points.windows(2) {
                let q = nearest_on_segment(p, pair[0], pair[1]);
                let d = distance(p, q);
                if d < best.1 {
                    best = (q, d, street.width);
                }
            }
        }
        best
    }
    fn tower(&mut self, p: [f32; 2], radius: f32, h: f32, roof: Surface) {
        let base = self.ground_height(p[0], p[1]) - 1.;
        self.piece(
            [p[0], base + h / 2., p[1]],
            [radius * 2., h, radius * 2.],
            0.,
            Surface::Stone,
            Shape::Cylinder,
            true,
        );
        self.piece(
            [p[0], base + h - 1., p[1]],
            [radius * 2.5, 2.4, radius * 2.5],
            0.,
            Surface::Plaster,
            Shape::Cylinder,
            false,
        );
        self.piece(
            [p[0], base + h + radius, p[1]],
            [radius * 2.8, radius * 2.7, radius * 2.8],
            0.,
            roof,
            Shape::Cone,
            false,
        );
        self.piece(
            [p[0], base + h + radius * 2.5, p[1]],
            [0.6, 5., 0.6],
            0.,
            Surface::Gold,
            Shape::Box,
            false,
        );
        for a in 0..6 {
            let yaw = a as f32 * TAU / 6.;
            let q = rotate(p, 0., radius + 0.05, yaw);
            let id = self.piece(
                [q[0], base + h * 0.66, q[1]],
                [1.5, 4., 0.15],
                yaw,
                Surface::Wood,
                Shape::Box,
                false,
            );
            self.blocks[id].detail = true;
        }
    }
    fn fortifications(&mut self) {
        for i in 0..WALL.len() {
            let a = WALL[i];
            let b = WALL[(i + 1) % WALL.len()];
            let len = distance(a, b);
            let yaw = (-(b[1] - a[1])).atan2(b[0] - a[0]);
            let segments = (len / 18.).ceil() as usize;
            let gate = GATE_SEGMENTS.contains(&i);
            for n in 0..segments {
                let lo = n as f32 / segments as f32;
                let hi = (n + 1) as f32 / segments as f32;
                let t = (lo + hi) / 2.;
                if gate && (t - 0.5).abs() * len < 23. {
                    continue;
                }
                let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                let floor = self.ground_height(p[0], p[1]);
                let dh = ((self
                    .ground_height(a[0] + (b[0] - a[0]) * lo, a[1] + (b[1] - a[1]) * lo)
                    - self.ground_height(a[0] + (b[0] - a[0]) * hi, a[1] + (b[1] - a[1]) * hi))
                .abs()
                    / 2.)
                    .max(1.);
                self.piece(
                    [p[0], floor + 10., p[1]],
                    [len / segments as f32 + 0.2, 20. + dh * 2., 6.],
                    yaw,
                    Surface::Stone,
                    Shape::Box,
                    true,
                );
                self.piece(
                    [p[0], floor + 20. + dh, p[1]],
                    [len / segments as f32 + 0.5, 1.3, 7.5],
                    yaw,
                    Surface::Plaster,
                    Shape::Box,
                    false,
                );
                for dx in [-6., 0., 6.] {
                    let q = rotate(p, dx, 0., yaw);
                    let id = self.piece(
                        [q[0], floor + 22. + dh, q[1]],
                        [2.6, 3., 7.],
                        yaw,
                        Surface::Stone,
                        Shape::Box,
                        false,
                    );
                    self.blocks[id].detail = true;
                }
            }
            self.tower(a, 9., 31. + (i % 3) as f32 * 3., Surface::Roof);
            if gate {
                let mid = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
                for dx in [-31., 31.] {
                    self.tower(rotate(mid, dx, 0., yaw), 10., 38., Surface::Roof);
                }
                self.piece(
                    [mid[0], self.ground_height(mid[0], mid[1]) + 12., mid[1]],
                    [38., 24., 6.],
                    yaw,
                    Surface::Plaster,
                    Shape::Arch,
                    false,
                );
            }
        }
    }
    fn hall(&mut self, s: HallSpec, id: usize) {
        let c = s.center;
        let [w, d] = s.size;
        let yard = self.ground_height(c[0], c[1]);
        let base = yard + 1.2;
        let h = s.wall_height;
        let yaw = s.yaw;
        let entrance = rotate(c, 0., d / 2. + 18., yaw);
        self.halls.push(Hall {
            name: s.name,
            center: c,
            half: [w / 2., d / 2.],
            entrance,
            yaw,
        });
        self.part(
            c,
            base,
            yaw,
            [0., -0.6, 0.],
            [w, 1.2, d],
            Surface::Paving,
            Shape::Box,
            false,
        );
        for x in [-w / 2., w / 2.] {
            self.part(
                c,
                base,
                yaw,
                [x, h / 2., 0.],
                [1.5, h, d],
                Surface::Plaster,
                Shape::Box,
                true,
            );
        }
        self.part(
            c,
            base,
            yaw,
            [0., h / 2., -d / 2.],
            [w, h, 1.5],
            Surface::Plaster,
            Shape::Box,
            true,
        );
        for sign in [-1., 1.] {
            self.part(
                c,
                base,
                yaw,
                [sign * (w / 4. + 3.), h / 2., d / 2.],
                [w / 2. - 6., h, 1.5],
                Surface::Plaster,
                Shape::Box,
                true,
            );
        }
        self.part(
            c,
            base,
            yaw,
            [0., (h + 10.) / 2., d / 2.],
            [12., h - 10., 1.5],
            Surface::Stone,
            Shape::Box,
            false,
        );
        self.part(
            c,
            base,
            yaw,
            [0., 6., d / 2. + 0.3],
            [15., 12., 2.5],
            Surface::Stone,
            Shape::Arch,
            false,
        );
        let rid = self.part(
            c,
            base,
            yaw,
            [0., h + w * 0.23, 0.],
            [w + 5., w * 0.46, d + 5.],
            if id == 2 {
                Surface::RedRoof
            } else {
                Surface::Roof
            },
            Shape::Gable,
            false,
        );
        self.blocks[rid].roof_of = Some(id);
        // Buttresses, Gothic windows and projecting cornices.
        for sign in [-1., 1.] {
            for row in 0..5 {
                let z = -d / 2. + 6. + row as f32 * (d - 12.) / 4.;
                self.part(
                    c,
                    base,
                    yaw,
                    [sign * (w / 2. + 2.), h * 0.42, z],
                    [4., h * 0.84, 3.],
                    Surface::Stone,
                    Shape::Box,
                    true,
                );
                self.part(
                    c,
                    base,
                    yaw,
                    [sign * (w / 2. + 0.8), h * 0.62, z + 3.],
                    [0.2, h * 0.4, 3.],
                    Surface::Glass,
                    Shape::Box,
                    false,
                );
            }
        }
        for z in [-d / 2., d / 2.] {
            self.part(
                c,
                base,
                yaw,
                [0., h - 0.5, z],
                [w + 4., 1.5, 3.],
                Surface::Stone,
                Shape::Box,
                false,
            );
        }
        for side in [-1., 1.] {
            for row in 0..6 {
                let z = -d / 2. + 7. + row as f32 * (d - 16.) / 5.;
                let x = side * (w / 2. - 7.);
                self.part(
                    c,
                    base,
                    yaw,
                    [x, 0.45, z],
                    [5., 0.9, 2.],
                    Surface::Wood,
                    Shape::Box,
                    true,
                );
            }
        }
        match id {
            0 => {
                self.tower(
                    rotate(c, -w / 2. - 13., -d / 2. + 4., yaw),
                    13.,
                    65.,
                    Surface::Roof,
                );
                self.tower(
                    rotate(c, w / 2. + 10., -d / 2. + 8., yaw),
                    10.,
                    42.,
                    Surface::GreenRoof,
                );
            }
            1 => {
                self.tower(
                    rotate(c, -w / 2. - 12., d / 2. - 7., yaw),
                    10.,
                    82.,
                    Surface::Roof,
                );
                self.tower(
                    rotate(c, w / 2. + 10., -d / 2. + 8., yaw),
                    8.,
                    52.,
                    Surface::Roof,
                );
            }
            3 => {
                self.tower(
                    rotate(c, -w / 2. - 8., -d / 2. + 5., yaw),
                    8.,
                    43.,
                    Surface::GreenRoof,
                );
            }
            _ => {}
        }
        self.civic_access(c, s.size, yaw, yard);
        for x in [-10., 10.] {
            let at = rotate(c, x, d / 2. + 0.4, yaw);
            self.scenery(SceneryKind::WallLantern, [at[0], base + 2.4, at[1]], yaw);
        }
        // Grounded banner beside the entry, in place of the old hanging cloth.
        let banner = rotate(c, -10., d / 2. + 1., yaw);
        self.scenery(SceneryKind::Banner, [banner[0], yard, banner[1]], yaw);
    }
    fn fountain(&mut self) {
        let y = self.ground_height(PLAZA[0], PLAZA[1]);
        let p = PLAZA;
        self.scenery(SceneryKind::Fountain, [p[0], y, p[1]], 0.);
        let id = self.piece(
            [p[0], y + 1.935, p[1]],
            [4.94, 3.87, 4.70],
            0.,
            Surface::Stone,
            Shape::Box,
            true,
        );
        self.conceal(id);
    }
    fn housing(&mut self) {
        let mut seed = 914_u32;
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed as f32 / u32::MAX as f32
        };
        let mut occupied: Vec<([f32; 2], f32)> = vec![];
        for _ in 0..18000 {
            let c = city([-580. + random() * 1200., -560. + random() * 1050.]);
            if !inside_city(c) {
                continue;
            }
            let w = 26. + random() * 13.;
            let d = 29. + random() * 17.;
            let radius = w.hypot(d) / 2. + 5.;
            if WALL.iter().enumerate().any(|(i, a)| {
                distance(c, nearest_on_segment(c, *a, WALL[(i + 1) % WALL.len()])) < radius + 16.
            }) {
                continue;
            }
            if ((c[0] - PLAZA[0]) / 115.).hypot((c[1] - PLAZA[1]) / 96.) < 1. {
                continue;
            }
            if HALL_SPECS
                .iter()
                .any(|s| distance(c, s.center) < s.size[0].hypot(s.size[1]) / 2. + radius + 22.)
            {
                continue;
            }
            if occupied.iter().any(|(p, r)| distance(c, *p) < radius + r) {
                continue;
            }
            let (q, dist, width) = self.nearest_street(c);
            if dist < width / 2. + radius + 2. || dist > 110. {
                continue;
            }
            let yaw = (q[0] - c[0]).atan2(q[1] - c[1]) + (random() - 0.5) * 0.22;
            let h = 18. + random() * 11.;
            let variant = (random() * 6.) as usize;
            let corners = [-1., 1.]
                .into_iter()
                .flat_map(|sx| [-1., 1.].map(|sz| rotate(c, sx * w / 2., sz * d / 2., yaw)));
            let center_height = height(c[0], c[1]);
            if corners
                .map(|p| (height(p[0], p[1]) - center_height).abs())
                .fold(0_f32, f32::max)
                > 1.2
            {
                continue;
            }
            self.buildings.push(BuildingSite {
                center: c,
                size: [w, d],
                height: h,
                yaw,
                variant,
            });
            self.plots.push(Plot {
                center: c,
                half: [w / 2. + 3., d / 2. + 3.],
                yaw,
                level: center_height,
                blend: 14.,
            });
            occupied.push((c, radius));
            self.houses += 1;
            if self.houses >= 56 {
                break;
            }
        }
    }
    fn house(&mut self, c: [f32; 2], size: [f32; 2], h: f32, yaw: f32, variant: usize) {
        let [w, d] = size;
        let base = self.ground_height(c[0], c[1]);
        let id = self.halls.len();
        let names = [
            "Купеческий дом",
            "Таверна путешественников",
            "Дом ремесленника",
            "Дом лекаря",
            "Гостевой дом",
            "Склад торговой гильдии",
        ];
        self.halls.push(Hall {
            name: names[variant % names.len()],
            center: c,
            half: [w / 2., d / 2.],
            entrance: rotate(c, 0., d / 2. + 6., yaw),
            yaw,
        });
        self.part(
            c,
            base,
            yaw,
            [0., -0.09, 0.],
            [w, 0.24, d],
            Surface::Wood,
            Shape::Box,
            false,
        );
        let plaster = if variant.is_multiple_of(3) {
            Surface::Plaster
        } else {
            Surface::Stone
        };
        for x in [-w / 2., w / 2.] {
            self.part(
                c,
                base,
                yaw,
                [x, 3., 0.],
                [1.2, 6., d],
                plaster,
                Shape::Box,
                true,
            );
        }
        self.part(
            c,
            base,
            yaw,
            [0., 3., -d / 2.],
            [w, 6., 1.2],
            plaster,
            Shape::Box,
            true,
        );
        for sign in [-1., 1.] {
            self.part(
                c,
                base,
                yaw,
                [sign * (w / 4. + 1.), 3., d / 2.],
                [w / 2. - 2., 6., 1.2],
                plaster,
                Shape::Box,
                true,
            );
        }
        self.part(
            c,
            base,
            yaw,
            [0., 5.4, d / 2.],
            [4., 1.2, 1.2],
            plaster,
            Shape::Box,
            false,
        );
        self.part(
            c,
            base,
            yaw,
            [0., 2.4, d / 2. + 0.2],
            [5.6, 4.8, 1.5],
            Surface::Plaster,
            Shape::Arch,
            false,
        );
        // Upper floors are hidden together with the roof while inside the ground floor.
        let upper = self.part(
            c,
            base,
            yaw,
            [0., (h + 6.) / 2., 0.],
            [w + 1.2, h - 6., d + 1.2],
            plaster,
            Shape::Box,
            false,
        );
        self.blocks[upper].roof_of = Some(id);
        self.blocks[upper].camera_solid = true;
        let roof = match variant % 3 {
            0 => Surface::RedRoof,
            1 => Surface::Roof,
            _ => Surface::GreenRoof,
        };
        let roof_block = self.part(
            c,
            base,
            yaw,
            [0., h + w * 0.26, 0.],
            [w + 3., w * 0.52, d + 3.],
            roof,
            Shape::Gable,
            false,
        );
        self.blocks[roof_block].roof_of = Some(id);
        let detail_start = self.blocks.len();
        // Monumental masonry bays, pointed windows and projecting buttresses.
        for (facade, (yaw, w, d)) in [(yaw, w, d), (yaw + PI / 2., d, w)].into_iter().enumerate() {
            for z in [-d / 2. - 0.8, d / 2. + 0.8] {
                let front = facade == 0 && z > 0.;
                for y in [1., h * 0.48, h] {
                    if front && y < 2. {
                        continue;
                    }
                    self.part(
                        c,
                        base,
                        yaw,
                        [0., y, z],
                        [w + 3., 1.1, 1.6],
                        Surface::Plaster,
                        Shape::Box,
                        false,
                    );
                }
                for x in [-w * 0.44, -w * 0.22, 0., w * 0.22, w * 0.44] {
                    if front && x.abs() < 2. {
                        continue;
                    }
                    self.part(
                        c,
                        base,
                        yaw,
                        [x, h * 0.46, z],
                        [1.4, h * 0.92, 2.4],
                        Surface::DarkStone,
                        Shape::Box,
                        true,
                    );
                }
                for x in [-w * 0.33, -w * 0.11, w * 0.11, w * 0.33] {
                    for y in [h * 0.26, h * 0.72] {
                        if front && x.abs() < 6. && y < h * 0.5 {
                            continue;
                        }
                        self.part(
                            c,
                            base,
                            yaw,
                            [x, y, z],
                            [4., h * 0.29, 0.4],
                            Surface::Glass,
                            Shape::Box,
                            false,
                        );
                        self.part(
                            c,
                            base,
                            yaw,
                            [x, y + 0.4, z + z.signum() * 0.35],
                            [5.2, h * 0.34, 0.7],
                            Surface::Plaster,
                            Shape::Arch,
                            false,
                        );
                    }
                }
            }
        }
        // An offset turret breaks up the silhouette without imposing a symmetric plan.
        let turret = rotate(c, -w * 0.38, -d * 0.35, yaw);
        self.piece(
            [turret[0], base + h * 0.65, turret[1]],
            [9., h * 1.3, 9.],
            yaw,
            Surface::Stone,
            Shape::Cylinder,
            true,
        );
        self.piece(
            [turret[0], base + h * 1.3 + 9., turret[1]],
            [12., 18., 12.],
            yaw,
            roof,
            Shape::Cone,
            false,
        );
        let banner = rotate(c, w * 0.32, d / 2. + 2., yaw);
        self.scenery(SceneryKind::Banner, [banner[0], base, banner[1]], yaw);
        self.part(
            c,
            base,
            yaw,
            [w * 0.3, h + w * 0.43, -d * 0.2],
            [1.8, 6., 2.],
            Surface::Stone,
            Shape::Box,
            false,
        );
        if variant == 0 {
            self.part(
                c,
                base,
                yaw,
                [-w * 0.2, h + 1., d * 0.35],
                [3.8, 3.5, 4.],
                plaster,
                Shape::Box,
                false,
            );
            self.part(
                c,
                base,
                yaw,
                [-w * 0.2, h + 3.8, d * 0.35],
                [4.8, 3.5, 5.],
                roof,
                Shape::Gable,
                false,
            );
        }
        for block in &mut self.blocks[detail_start..] {
            if block.center[1] - block.size[1] / 2. >= base + 5. {
                block.roof_of = Some(id);
            }
        }
        // Furnishings keep a wide, unobstructed central aisle from door to rear.
        for sign in [-1., 1.] {
            for z in [-d * 0.18, d * 0.16] {
                let x = sign * w * 0.25;
                self.part(
                    c,
                    base,
                    yaw,
                    [x, 0.4, z],
                    [3.2, 0.8, 2.],
                    Surface::Wood,
                    Shape::Box,
                    true,
                );
                self.part(
                    c,
                    base,
                    yaw,
                    [x, 0.85, z],
                    [3.5, 0.1, 2.3],
                    Surface::Plaster,
                    Shape::Box,
                    false,
                );
                self.part(
                    c,
                    base,
                    yaw,
                    [x, 0.22, z + 2.],
                    [3., 0.44, 0.7],
                    Surface::Wood,
                    Shape::Box,
                    true,
                );
            }
        }
        self.part(
            c,
            base,
            yaw,
            [w / 2. - 2.5, 1.2, -d / 2. + 3.],
            [3., 2.4, 2.],
            Surface::Wood,
            Shape::Box,
            true,
        );
        self.part(
            c,
            base,
            yaw,
            [-w / 2. + 3., 0.25, d / 2. - 5.],
            [2., 0.5, 2.4],
            Surface::Wood,
            Shape::Box,
            true,
        );
        self.part(
            c,
            base,
            yaw,
            [-w / 2. + 3., 0.6, d / 2. - 5.],
            [1.9, 0.2, 2.3],
            Surface::RedRoof,
            Shape::Box,
            false,
        );
    }
    fn market_stalls(&mut self) {
        for i in 0..15 {
            let a = 0.1 + i as f32 * 0.13;
            let c = [PLAZA[0] + 92. * a.cos(), PLAZA[1] + 75. * a.sin()];
            if self.nearest_street(c).1 < 17. {
                continue;
            }
            let base = self.ground_height(c[0], c[1]);
            let yaw = PI / 2. - a;
            self.scenery(SceneryKind::MarketStall, [c[0], base, c[1]], yaw);
            let id = self.piece(
                [c[0], base + 1.5, c[1]],
                [3.67, 3.0, 2.15],
                yaw,
                Surface::Wood,
                Shape::Box,
                true,
            );
            self.conceal(id);
        }
    }
    fn river_bridge(&mut self) {
        let c = [river_x(45.), 45.];
        let left = self.ground_height(c[0] - 85., c[1]);
        let right = self.ground_height(c[0] + 85., c[1]);
        let level = left.max(right) + 0.3;
        self.bridge = Some(Bridge { center: c, level });
        self.piece(
            [c[0], level - 0.4, c[1]],
            [110., 0.8, 16.],
            0.,
            Surface::Paving,
            Shape::Box,
            false,
        );
        for z in [-8.5, 8.5] {
            self.piece(
                [c[0], level + 0.6, c[1] + z],
                [110., 1.2, 1.],
                0.,
                Surface::Stone,
                Shape::Box,
                true,
            );
        }
        for dx in [-36., 0., 36.] {
            self.piece(
                [c[0] + dx, (level - 0.8 + 7.) / 2., c[1]],
                [37., level - 0.8 - 7., 14.],
                0.,
                Surface::Stone,
                Shape::Arch,
                false,
            );
        }
        for (sign, base) in [(-1., left), (1., right)] {
            self.ramps.push(Ramp {
                center: [c[0] + sign * 70., c[1]],
                yaw: sign * PI / 2.,
                width: 16.,
                length: 30.,
                base,
                rise: level - base,
            });
        }
    }
    fn forest(&mut self) {
        let mut seed = 719_u32;
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed as f32 / u32::MAX as f32
        };
        for _ in 0..14000 {
            let x = (random() * 2. - 1.) * 5100.;
            let z = (random() * 2. - 1.) * 5100.;
            if inside_city([x, z])
                || (x - river_x(z)).abs() < 45.
                || (x * 0.005).sin() + (z * 0.007).cos() < -0.2
                || self.on_road(x, z, 8.)
            {
                continue;
            }
            let y = self.ground_height(x, z);
            if y > 300. {
                continue;
            }
            let scale = 0.75 + random() * 1.3;
            let tint = (random() * 4.) as usize;
            self.trees.push(Tree {
                position: [x, y, z],
                scale,
                tint,
            });
            let radius = 0.55 * scale;
            self.obstacles.push(Bounds {
                center: [x, z],
                half: [radius, radius],
                yaw: 0.,
                min: [x - radius, y, z - radius],
                max: [x + radius, y + 8. * scale, z + radius],
            });
        }
    }
    fn civic_access(&mut self, c: [f32; 2], size: [f32; 2], yaw: f32, yard: f32) {
        let front = size[1] / 2.;
        // Full-width landing gives room to turn into the side ramp.
        self.part(
            c,
            yard,
            yaw,
            [4., 0.6, front + 2.],
            [28., 1.2, 4.],
            Surface::Paving,
            Shape::Box,
            false,
        );
        for i in 0..6 {
            let top = yard + (6 - i) as f32 * 0.2;
            let p = rotate(c, 0., front + 4. + (i as f32 + 0.5), yaw);
            self.piece(
                [p[0], (yard + top) / 2., p[1]],
                [12., top - yard, 1.],
                yaw,
                Surface::Paving,
                Shape::Box,
                false,
            );
            self.stairs.push(Bounds {
                center: p,
                half: [6., 0.5],
                yaw,
                min: [p[0] - 7., yard, p[1] - 7.],
                max: [p[0] + 7., top, p[1] + 7.],
            });
        }
        self.ramps.push(Ramp {
            center: rotate(c, 13., front + 10., yaw),
            yaw,
            width: 8.,
            length: 12.,
            base: yard,
            rise: 1.2,
        });
        // Stone cheeks keep the landing's sides out of the walking route.
        for x in [-10.4, 18.4] {
            self.part(
                c,
                yard,
                yaw,
                [x, 1.2, front + 2.],
                [0.8, 2.4, 4.],
                Surface::Stone,
                Shape::Box,
                true,
            );
        }
        self.part(
            c,
            yard,
            yaw,
            [-8., 1.2, front + 4.],
            [4., 2.4, 0.8],
            Surface::Stone,
            Shape::Box,
            true,
        );
        self.part(
            c,
            yard,
            yaw,
            [7.5, 1.2, front + 4.],
            [3., 2.4, 0.8],
            Surface::Stone,
            Shape::Box,
            true,
        );
    }
    fn work_stations(&mut self) {
        use stations::Furnishing;
        for station in stations::STATIONS {
            let c = station.counter_position();
            let yaw = station.yaw();
            let base = self.surface_height(c[0], c[1]);
            self.scenery(SceneryKind::Counter, [c[0], base, c[1]], yaw);
            if station.hall().is_none() {
                let id = self.piece(
                    [c[0], base + 0.76, c[1]],
                    [3.17, 1.52, 1.07],
                    yaw,
                    Surface::Wood,
                    Shape::Box,
                    true,
                );
                self.conceal(id);
            }
            match station.furnishing {
                Furnishing::Smithy => {
                    let anvil = rotate(c, 0., station.work_offset(), yaw);
                    self.scenery(SceneryKind::Anvil, [anvil[0], base, anvil[1]], yaw);
                    let id = self.piece(
                        [anvil[0], base + 0.76, anvil[1]],
                        [1.47, 1.51, 0.99],
                        yaw,
                        Surface::DarkStone,
                        Shape::Box,
                        true,
                    );
                    self.blocks[id].hidden = true;
                    self.blocks[id].camera_solid = false;
                    let forge = rotate(c, 0., station.work_offset() - 2.4, yaw);
                    self.scenery(SceneryKind::Forge, [forge[0], base, forge[1]], yaw);
                    let sign = rotate(c, 2.15, station.work_offset(), yaw);
                    self.scenery(SceneryKind::BlacksmithSign, [sign[0], base, sign[1]], yaw);
                }
                Furnishing::Market => {
                    let crates = rotate(c, -2.2, station.work_offset(), yaw);
                    self.scenery(SceneryKind::Crates, [crates[0], base, crates[1]], yaw);
                }
                _ => {}
            }
        }
    }
    fn plaza_decor(&mut self) {
        let spots = [
            (SceneryKind::StreetLantern, [24., 18.], 0.2),
            (SceneryKind::StreetLantern, [-26., 14.], 2.4),
            (SceneryKind::StreetLantern, [16., -28.], -0.6),
            (SceneryKind::StreetLantern, [-18., -22.], 1.1),
            (SceneryKind::SquarePlanter, [14., 12.], 0.3),
            (SceneryKind::SquarePlanter, [-12., 20.], 1.0),
            (SceneryKind::LongPlanter, [28., -8.], 0.5),
            (SceneryKind::LongPlanter, [-30., -6.], 1.6),
            (SceneryKind::Fence, [34., 8.], 0.15),
            (SceneryKind::Fence, [37.1, 8.], 0.15),
            (SceneryKind::Fence, [40.2, 8.], 0.15),
        ];
        for (kind, local, yaw) in spots {
            let p = [PLAZA[0] + local[0], PLAZA[1] + local[1]];
            let (_, dist, width) = self.nearest_street(p);
            if dist < width / 2. + 2.2 || distance(p, PLAZA) < 6. {
                continue;
            }
            let y = self.ground_height(p[0], p[1]);
            self.scenery(kind, [p[0], y, p[1]], yaw);
            if kind == SceneryKind::Fence {
                let id = self.piece(
                    [p[0], y + 0.93, p[1]],
                    [3.12, 1.86, 0.72],
                    yaw,
                    Surface::Stone,
                    Shape::Box,
                    true,
                );
                self.conceal(id);
            }
        }
        let patch = [PLAZA[0] + 8., PLAZA[1] + 3.];
        let (_, dist, width) = self.nearest_street(patch);
        if dist > width / 2. + 2.2 && distance(patch, PLAZA) > 6. {
            let y = self.ground_height(patch[0], patch[1]);
            self.scenery(
                SceneryKind::Cobblestone,
                [patch[0], y - 0.17952, patch[1]],
                0.4,
            );
        }
    }
    /// Eye-level visibility: counters block walking but not conversation above them.
    pub fn sight_clear(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        if a.into_iter()
            .chain(b)
            .any(|v| !v.is_finite() || v.abs() > WORLD_HALF)
        {
            return false;
        }
        if self.obstacles.iter().any(|o| o.blocks_sight(a, b)) {
            return false;
        }
        // Terrain occlusion: sample every 0.5 m; the line must stay 5 cm above the surface.
        let steps = (distance([a[0], a[2]], [b[0], b[2]]) / 0.5).ceil().max(1.) as usize;
        (0..=steps).all(|i| {
            let t = i as f32 / steps as f32;
            let p: [f32; 3] = std::array::from_fn(|j| a[j] + (b[j] - a[j]) * t);
            p[1] > self.surface_height(p[0], p[2]) + 0.05
        })
    }
    /// Height of whatever you stand on at (x, z), checked in priority order: bridge deck,
    /// civic hall floor (1.2 m above its platform) and entrance steps, house floors, ramps, terrain.
    pub fn surface_height(&self, x: f32, z: f32) -> f32 {
        if let Some(b) = &self.bridge
            && (x - b.center[0]).abs() <= 55.0001
            && (z - b.center[1]).abs() <= 8.0001
        {
            return b.level;
        }
        for (id, h) in self.halls.iter().take(HALL_SPECS.len()).enumerate() {
            let p = local([x, z], h.center, h.yaw);
            let level = height(HALL_SPECS[id].center[0], HALL_SPECS[id].center[1]) + 1.2;
            if p[0].abs() <= h.half[0] && p[1].abs() <= h.half[1] {
                return level;
            }
            // Entrance stairs: 12 m wide, six 1 m treads dropping 0.2 m each, 4–10 m outside the door.
            // The 0.0001 tolerances keep exact edge points inside despite float rounding.
            if p[0].abs() <= 6.0001
                && p[1] >= h.half[1] + 4. - 0.0001
                && p[1] <= h.half[1] + 10. + 0.0001
            {
                let step = (p[1] - h.half[1] - 4.).floor().clamp(0., 5.);
                return level - step * 0.2;
            }
            // Landing just outside the door, at floor level (wider on the side ramp's side).
            if (-10. ..=18.).contains(&p[0]) && (h.half[1]..=h.half[1] + 4.).contains(&p[1]) {
                return level;
            }
        }
        for h in self.halls.iter().skip(HALL_SPECS.len()) {
            let p = local([x, z], h.center, h.yaw);
            if p[0].abs() <= h.half[0] && p[1].abs() <= h.half[1] {
                return self.ground_height(h.center[0], h.center[1]) + 0.03;
            }
        }
        for ramp in &self.ramps {
            let p = local([x, z], ramp.center, ramp.yaw);
            if p[0].abs() <= ramp.width / 2. + 0.0001 && p[1].abs() <= ramp.length / 2. + 0.0001 {
                return (ramp.base + ramp.rise * (0.5 - p[1] / ramp.length).clamp(0., 1.))
                    .max(self.ground_height(x, z));
            }
        }
        self.ground_height(x, z)
    }
    /// Can a character stand here: inside the map, not in an obstacle (inflated by `PLAYER_RADIUS`),
    /// not in the river (except on the bridge), not in the mountains, and terrain slope below 0.8.
    pub fn walkable(&self, x: f32, z: f32) -> bool {
        if !x.is_finite()
            || !z.is_finite()
            || x.abs() > WORLD_HALF - 2.
            || z.abs() > WORLD_HALF - 2.
        {
            return false;
        }
        let cell = ((x / 32.).floor() as i32, (z / 32.).floor() as i32);
        if self.obstacle_cells.get(&cell).is_some_and(|ids| {
            ids.iter()
                .any(|id| self.obstacles[*id].contains_xz(x, z, PLAYER_RADIUS))
        }) {
            return false;
        }
        let on_bridge = self
            .bridge
            .is_some_and(|b| (x - b.center[0]).abs() < 85. && (z - b.center[1]).abs() < 7.5);
        if !on_bridge
            && z.abs() <= 5000.
            && (x - river_x(z)).abs() < 24.
            && self.ground_height(x, z) < 8.5
        {
            return false;
        }
        // The impassable boundary lies on the visible escarpment, kilometres
        // before the mesh ends. No walkable route can reach the technical edge.
        if mountain_distance(x, z) > 100. {
            return false;
        }
        if self
            .bridge
            .is_some_and(|b| (x - b.center[0]).abs() < 85. && (z - b.center[1]).abs() < 7.5)
        {
            return true;
        }
        // Slope from central differences over ±1 m; 0.8 rise per meter ≈ 39°.
        let dx = (self.ground_height(x + 1., z) - self.ground_height(x - 1., z)) / 2.;
        let dz = (self.ground_height(x, z + 1.) - self.ground_height(x, z - 1.)) / 2.;
        dx.hypot(dz) < 0.8
    }
    /// The core movement rule (client and server): can a character walk straight from `a` to `b`?
    /// Exact sweep against obstacles, then samples every 0.35 m for walkability and step height.
    pub fn segment_clear(&self, a: [f32; 2], b: [f32; 2]) -> bool {
        if a.into_iter()
            .chain(b)
            .any(|v| !v.is_finite() || v.abs() > WORLD_HALF)
        {
            return false;
        }
        for x in (a[0].min(b[0]) / 32.).floor() as i32..=(a[0].max(b[0]) / 32.).floor() as i32 {
            for z in (a[1].min(b[1]) / 32.).floor() as i32..=(a[1].max(b[1]) / 32.).floor() as i32 {
                if self.obstacle_cells.get(&(x, z)).is_some_and(|ids| {
                    ids.iter()
                        .any(|id| self.obstacles[*id].intersects_segment(a, b, PLAYER_RADIUS))
                }) {
                    return false;
                }
            }
        }
        let steps = (distance(a, b) / 0.35).ceil().max(1.) as usize;
        let mut previous = self.surface_height(a[0], a[1]);
        (0..=steps).all(|i| {
            let t = i as f32 / steps as f32;
            let x = a[0] + (b[0] - a[0]) * t;
            let z = a[1] + (b[1] - a[1]) * t;
            let y = self.surface_height(x, z);
            // Max climb between samples: 0.32 m (stair treads are 0.2 m; walls/ledges are taller).
            let step_ok = (y - previous).abs() <= 0.32;
            previous = y;
            step_ok && self.walkable(x, z)
        })
    }
    /// Index in `halls` of the interior containing (x, z), if any.
    pub fn hall_at(&self, x: f32, z: f32) -> Option<usize> {
        self.halls.iter().position(|h| {
            let p = local([x, z], h.center, h.yaw);
            p[0].abs() < h.half[0] && p[1].abs() < h.half[1]
        })
    }
}
#[cfg(test)]
mod reference_city {
    use super::*;
    #[test]
    fn props_replace_the_fountain_market_and_workstations() {
        let world = World::new();
        assert!(
            world
                .scenery
                .iter()
                .any(|item| item.kind == SceneryKind::Fountain)
        );
        assert!(
            world
                .scenery
                .iter()
                .any(|item| item.kind == SceneryKind::MarketStall)
        );
        assert_eq!(
            world
                .scenery
                .iter()
                .filter(|item| item.kind == SceneryKind::Counter)
                .count(),
            stations::STATIONS.len()
        );
        assert!(
            world
                .scenery
                .iter()
                .any(|item| item.kind == SceneryKind::Anvil)
        );
        assert!(world.walkable(PLAZA[0] + 7., PLAZA[1]));
        assert!(!world.walkable(PLAZA[0], PLAZA[1]));
        for item in &world.scenery {
            if !matches!(
                item.kind,
                SceneryKind::StreetLantern
                    | SceneryKind::SquarePlanter
                    | SceneryKind::LongPlanter
                    | SceneryKind::Fence
                    | SceneryKind::Cobblestone
            ) {
                continue;
            }
            let (_, dist, width) = world.nearest_street([item.position[0], item.position[2]]);
            assert!(dist > width / 2. + 2., "{:?} overlaps a street", item.kind);
        }
    }
}
