//! One triangulated surface for drawing, walking and terrain picking.
use crate::{World, height, local, smooth};

pub const TILE_SIZE: f32 = 512.;
pub const RENDER_HALF: f32 = 9216.;
pub fn grid_step(x: f32, z: f32) -> f32 {
    if (-512. ..512.).contains(&x) && (-512. ..512.).contains(&z) {
        2.
    } else if (-1024. ..1024.).contains(&x) && (-1024. ..1024.).contains(&z) {
        4.
    } else {
        16.
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Plot {
    pub center: [f32; 2],
    pub half: [f32; 2],
    pub yaw: f32,
    pub level: f32,
    pub blend: f32,
}
impl Plot {
    pub fn grade(&self, x: f32, z: f32, original: f32) -> f32 {
        let p = local([x, z], self.center, self.yaw);
        let outside = (p[0].abs() - self.half[0]).max(p[1].abs() - self.half[1]);
        let w = 1. - smooth(0., self.blend, outside);
        original * (1. - w) + self.level * w
    }
}
impl World {
    fn grade_height(&self, x: f32, z: f32) -> f32 {
        let original = height(x, z);
        let mut weighted = original;
        let mut total = 1.;
        if x.abs() < 600. && z.abs() < 600. {
            for plot in &self.plots {
                let p = local([x, z], plot.center, plot.yaw);
                let outside = (p[0].abs() - plot.half[0]).max(p[1].abs() - plot.half[1]);
                if outside <= 0. {
                    return plot.level;
                }
                // Adjacent plot shoulders blend continuously, independent of plot order.
                // As a flat plot edge approaches, its level dominates the mixture.
                let w = 1. - smooth(0., plot.blend, outside);
                if w >= 1. {
                    return plot.level;
                }
                let weight = w / (1. - w);
                weighted += plot.level * weight;
                total += weight;
            }
        }
        weighted / total
    }
    /// Fine edge vertices follow the coarse neighbour, avoiding cracks at tile seams.
    pub fn terrain_vertex(&self, x: f32, z: f32) -> f32 {
        for (edge, step) in [(1024., 16.), (512., 4.)] {
            if (x.abs() - edge).abs() < 0.001 && z.abs() <= edge {
                let z0 = (z / step).floor() * step;
                let t = (z - z0) / step;
                return self.grade_height(x, z0) * (1. - t) + self.grade_height(x, z0 + step) * t;
            }
            if (z.abs() - edge).abs() < 0.001 && x.abs() <= edge {
                let x0 = (x / step).floor() * step;
                let t = (x - x0) / step;
                return self.grade_height(x0, z) * (1. - t) + self.grade_height(x0 + step, z) * t;
            }
        }
        self.grade_height(x, z)
    }
    pub fn ground_height(&self, x: f32, z: f32) -> f32 {
        let step = grid_step(x, z);
        let x0 = (x / step).floor() * step;
        let z0 = (z / step).floor() * step;
        let u = (x - x0) / step;
        let v = (z - z0) / step;
        let b = self.terrain_vertex(x0 + step, z0);
        let c = self.terrain_vertex(x0, z0 + step);
        if u + v <= 1. {
            self.terrain_vertex(x0, z0) * (1. - u - v) + b * u + c * v
        } else {
            self.terrain_vertex(x0 + step, z0 + step) * (u + v - 1.) + b * (1. - v) + c * (1. - u)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walking_surface_is_the_rendered_triangle_in_city_and_countryside() {
        let world = World::new();
        for (x, z) in [
            (-200., 80.),
            (10., 10.),
            (510., 40.),
            (514., 40.),
            (1020., 20.),
            (1104., 32.),
            (-4000., 1700.),
        ] {
            let step = grid_step(x, z);
            let x = (x / step).floor() * step;
            let z = (z / step).floor() * step;
            let a = world.terrain_vertex(x, z);
            let b = world.terrain_vertex(x + step, z);
            let c = world.terrain_vertex(x, z + step);
            let d = world.terrain_vertex(x + step, z + step);
            for (u, v, expected) in [
                (0.2, 0.3, a * 0.5 + b * 0.2 + c * 0.3),
                (0.8, 0.7, d * 0.5 + b * 0.3 + c * 0.2),
            ] {
                assert!((world.ground_height(x + step * u, z + step * v) - expected).abs() < 0.002);
            }
        }
    }
    #[test]
    fn neighbourhood_rises_have_no_abrupt_plot_edges() {
        let world = World::new();
        for x in -210..210 {
            for z in -210..210 {
                let (x, z) = (x as f32 * 2., z as f32 * 2.);
                if !crate::inside_city([x, z]) {
                    continue;
                }
                let y = world.ground_height(x, z);
                for (dx, dz) in [(2., 0.), (0., 2.)] {
                    assert!(
                        (world.ground_height(x + dx, z + dz) - y).abs() < 0.8,
                        "abrupt plot shoulder at {x}/{z}"
                    );
                }
            }
        }
        // Civic hill is exceptional; other neighbourhoods stay within 2–5 m.
        let base = 36.;
        for (x, z) in [(-180., -95.), (240., 90.)] {
            let rise = world.ground_height(x, z) - base;
            assert!((2. ..5.).contains(&rise), "neighbourhood rise {rise}");
        }
        assert!((5. ..7.).contains(&(world.ground_height(76., -208.) - base)));
    }
    #[test]
    fn terrain_resolution_boundaries_are_continuous() {
        let world = World::new();
        for edge in [-1024., -512., 512., 1024.] {
            for i in -200..200 {
                let p = i as f32 * 2.37;
                assert!(
                    (world.ground_height(edge - 0.001, p) - world.ground_height(edge + 0.001, p))
                        .abs()
                        < 0.02,
                    "x seam {edge}/{p}"
                );
                assert!(
                    (world.ground_height(p, edge - 0.001) - world.ground_height(p, edge + 0.001))
                        .abs()
                        < 0.02,
                    "z seam {edge}/{p}"
                );
            }
        }
    }
}
