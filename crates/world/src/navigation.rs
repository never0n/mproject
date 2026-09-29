//! Bounded A* for click-to-move. Exact segment validation supplements the coarse grid.
use crate::{WORLD_HALF, World};
use std::collections::{BinaryHeap, HashMap};

const CELL: f32 = 2.0;
const LIMIT: usize = 400_000;
type Cell = (i32, i32);
fn point(c: Cell) -> [f32; 2] {
    [c.0 as f32 * CELL, c.1 as f32 * CELL]
}
fn cell(p: [f32; 2]) -> Cell {
    ((p[0] / CELL).round() as i32, (p[1] / CELL).round() as i32)
}
fn heuristic(a: Cell, b: Cell) -> i32 {
    let dx = (a.0 - b.0).abs();
    let dz = (a.1 - b.1).abs();
    10 * dx.max(dz) + 4 * dx.min(dz)
}

#[derive(Debug, PartialEq)]
pub enum PathError {
    BlockedDestination,
    NoRoute,
    SearchLimit,
}

pub fn find_path(world: &World, from: [f32; 2], to: [f32; 2]) -> Result<Vec<[f32; 2]>, PathError> {
    if !world.walkable(to[0], to[1]) {
        return Err(PathError::BlockedDestination);
    }
    if world.segment_clear(from, to) {
        return Ok(vec![to]);
    }
    let attach = |p| {
        let c = cell(p);
        let mut candidates: Vec<_> = (-2..=2)
            .flat_map(|x| (-2..=2).map(move |z| (c.0 + x, c.1 + z)))
            .collect();
        candidates.sort_by(|a, b| {
            let pa = point(*a);
            let pb = point(*b);
            ((pa[0] - p[0]).hypot(pa[1] - p[1])).total_cmp(&((pb[0] - p[0]).hypot(pb[1] - p[1])))
        });
        candidates
            .into_iter()
            .find(|c| world.segment_clear(p, point(*c)))
    };
    let start = attach(from).ok_or(PathError::NoRoute)?;
    let goal = attach(to).ok_or(PathError::NoRoute)?;
    let mut open = BinaryHeap::new();
    let mut costs = HashMap::new();
    let mut parents = HashMap::new();
    open.push((-heuristic(start, goal), 0, start));
    costs.insert(start, 0);
    let mut expanded = 0;
    while let Some((_, negative_g, current)) = open.pop() {
        let g = -negative_g;
        if costs.get(&current).copied() != Some(g) {
            continue;
        }
        if current == goal {
            let mut path = vec![to, point(goal)];
            let mut at = goal;
            while at != start {
                at = parents[&at];
                path.push(point(at));
            }
            path.reverse();
            // Collapse grid stair steps while preserving collision clearance.
            let mut result = Vec::new();
            let mut anchor = from;
            let mut i = 0;
            while i < path.len() {
                let mut next = i;
                while next + 1 < path.len() && world.segment_clear(anchor, path[next + 1]) {
                    next += 1;
                }
                result.push(path[next]);
                anchor = path[next];
                i = next + 1;
            }
            return Ok(result);
        }
        expanded += 1;
        if expanded > LIMIT {
            return Err(PathError::SearchLimit);
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let next = (current.0 + dx, current.1 + dz);
                if next.0.abs().max(next.1.abs()) as f32 * CELL >= WORLD_HALF {
                    continue;
                }
                let next_g = g + if dx != 0 && dz != 0 { 14 } else { 10 };
                if next_g >= costs.get(&next).copied().unwrap_or(i32::MAX) {
                    continue;
                }
                if !world.segment_clear(point(current), point(next)) {
                    continue;
                }
                costs.insert(next, next_g);
                parents.insert(next, current);
                open.push((-(next_g + heuristic(next, goal)), -next_g, next));
            }
        }
    }
    Err(PathError::NoRoute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SPAWN, height};

    #[test]
    fn every_house_has_a_clear_two_way_door_and_a_hidden_upper_floor() {
        let world = World::new();
        assert_eq!(world.halls.len(), world.houses + 4);
        for (id, house) in world.halls.iter().enumerate().skip(4) {
            assert!(
                world.segment_clear(house.entrance, house.center),
                "entrance {id}"
            );
            assert!(
                world.segment_clear(house.center, house.entrance),
                "exit {id}"
            );
            assert_eq!(world.hall_at(house.center[0], house.center[1]), Some(id));
            assert!(world.blocks.iter().any(|b| b.roof_of == Some(id)));
        }
    }

    #[test]
    fn all_gates_and_public_halls_are_reachable() {
        let world = World::new();
        let mut destinations: Vec<_> = world
            .gates
            .iter()
            .map(|p| [p[0] * 1.09, p[1] * 1.09])
            .collect();
        destinations.extend(world.halls.iter().map(|h| h.center));
        for to in destinations {
            let path = find_path(&world, SPAWN, to).unwrap_or_else(|e| panic!("{to:?}: {e:?}"));
            let mut previous = SPAWN;
            for p in &path {
                assert!(world.segment_clear(previous, *p));
                previous = *p;
            }
            assert_eq!(path.last(), Some(&to));
        }
    }

    #[test]
    fn walls_and_closed_buildings_block_both_navigation_and_sweeps() {
        let world = World::new();
        let wall = world.obstacles[0];
        let a = crate::rotate(wall.center, 0., -12., wall.yaw);
        let b = crate::rotate(wall.center, 0., 12., wall.yaw);
        assert!(!world.segment_clear(a, b));
        assert_eq!(
            find_path(&world, SPAWN, wall.center),
            Err(PathError::BlockedDestination)
        );
        assert!(!world.walkable(f32::NAN, 0.0));
        assert!(!world.walkable(WORLD_HALF + 1., 0.));
    }

    #[test]
    fn civic_stairs_and_accessible_ramps_work_in_both_directions() {
        let world = World::new();
        for (id, hall) in world.halls.iter().take(4).enumerate() {
            let front = hall.half[1];
            let bottom = crate::rotate(hall.center, 0., front + 12., hall.yaw);
            let inner = crate::rotate(hall.center, 0., front - 5., hall.yaw);
            assert!(
                world.segment_clear(bottom, inner),
                "stairs {id}: {bottom:?} -> {inner:?}"
            );
            assert!(world.segment_clear(inner, bottom), "descending stairs {id}");
            let ramp = &world.ramps[id];
            let bottom = crate::rotate(ramp.center, 0., ramp.length / 2. + 1., ramp.yaw);
            let top = crate::rotate(ramp.center, 0., -ramp.length / 2. - 1., ramp.yaw);
            assert!(world.segment_clear(bottom, top), "ramp {id}");
            assert!(world.segment_clear(top, bottom), "descending ramp {id}");
            let landing = crate::rotate(hall.center, 0., front + 2., hall.yaw);
            assert!(world.segment_clear(top, landing), "landing {id}");
            assert!(world.segment_clear(landing, inner));
        }
    }

    #[test]
    fn street_centres_remain_clear_of_buildings_and_fortifications() {
        let world = World::new();
        let blocked: Vec<_> = world
            .streets
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                s.points
                    .iter()
                    .filter(|p| {
                        crate::inside_city(**p)
                            && crate::distance(**p, crate::PLAZA) > 15.
                            && !world.walkable(p[0], p[1])
                    })
                    .map(move |p| (i, *p))
            })
            .collect();
        assert!(blocked.is_empty(), "blocked street centres: {blocked:?}");
    }
    #[test]
    fn sweeps_reject_the_thin_corner_previously_missed_by_sampling() {
        let world = World::new();
        // Follow the same thin civic corner when the city footprint changes.
        let c = crate::HALL_SPECS[0].center;
        let p = |x: f32, z: f32| [c[0] + x - 82.8, c[1] + z + 226.8];
        let a = p(68., -202.);
        let b = p(68., -202.5);
        assert!(world.walkable(a[0], a[1]));
        assert!(!world.walkable(b[0], b[1]));
        assert!(!world.segment_clear(p(68., -200.), p(68., -204.)));
    }

    #[test]
    fn compact_city_is_gently_graded_and_buildings_have_flat_plots() {
        let world = World::new();
        let city: Vec<_> = (-24..=24)
            .flat_map(|x| (-24..=24).map(move |z| [x as f32 * 20., z as f32 * 20.]))
            .filter(|p| crate::inside_city(*p))
            .map(|p| world.ground_height(p[0], p[1]))
            .collect();
        let range = city.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - city.iter().copied().fold(f32::INFINITY, f32::min);
        assert!((6. ..10.).contains(&range), "city height range: {range}");
        for b in &world.buildings {
            let base = world.ground_height(b.center[0], b.center[1]);
            for x in [-b.size[0] / 2., 0., b.size[0] / 2.] {
                for z in [-b.size[1] / 2., 0., b.size[1] / 2.] {
                    let p = crate::rotate(b.center, x, z, b.yaw);
                    assert!(
                        (world.ground_height(p[0], p[1]) - base).abs() < 0.05,
                        "uneven building plot: {p:?}"
                    );
                }
            }
        }
        assert!(
            (20..=56).contains(&world.houses),
            "houses: {}",
            world.houses
        );
        assert_eq!(world.stairs.len(), 24);
        eprintln!(
            "{} buildings; city elevation range {range:.2} m",
            world.houses
        );
    }

    #[test]
    fn bridge_connects_both_banks_without_entering_water() {
        let world = World::new();
        let b = world.bridge.unwrap();
        let a = [b.center[0] - 90., b.center[1]];
        let c = [b.center[0] + 90., b.center[1]];
        assert!(world.segment_clear(a, c));
        assert!(world.segment_clear(c, a));
        assert!(!world.walkable(b.center[0], b.center[1] + 25.));
        assert!(world.surface_height(b.center[0], b.center[1]) > 8.);
    }

    #[test]
    fn mountains_close_every_direction_before_the_mesh_boundary() {
        let world = World::new();
        for i in 0..720 {
            let a = i as f32 * std::f32::consts::TAU / 720.;
            let r = crate::mountain_inner_radius(a);
            let p = [(r + 180.) * a.cos(), (r + 180.) * a.sin()];
            assert!(!world.walkable(p[0], p[1]), "open mountain boundary at {a}");
            assert!(
                height(p[0], p[1]) > 450.,
                "mountain barrier must be visible at {a}: {}",
                height(p[0], p[1])
            );
            assert!(r + 180. < WORLD_HALF - 1000.);
        }
        assert!(
            world.walkable(-4000., 0.),
            "the former map boundary must be explorable"
        );
    }
}
