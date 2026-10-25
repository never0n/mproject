//! Fill one supported path. An empty cell is not frozen. A solid cell on the only
//! path stops the flow. The result is the filled field, not a catalogue blade.

use crate::crafting_physics::{
    enthalpy::{Material, liquid_enthalpy, temperature},
    geometry::Mold,
};

const GRAVITY: f64 = 9.81;

#[derive(Clone, Copy, Debug)]
pub struct PourRequest<'a> {
    pub mold: &'a Mold,
    pub material: &'a Material,
    pub gate_area: f64,
    pub head: f64,
    pub discharge: f64,
    pub mold_temperature: f64,
    pub film: f64,
    pub source_temperature: f64,
    pub source_mass: f64,
    pub dt_max: f64,
    pub time_limit: f64,
    pub step_budget: u64,
    pub sample_at: &'a [f64],
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub time: f64,
    pub flow: f64,
    pub front: f64,
    pub filled: f64,
    pub coldest_liquid: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    Filled,
    /// First solid cell on the only feed path. This is not a cold shut.
    Plug {
        cell: usize,
    },
    SourceSpent,
    TimeLimit,
    /// The step budget ran out. This is a technical stop, not a casting defect.
    StepBudget,
    Numerical,
}

#[derive(Clone, Debug)]
pub struct CellState {
    pub mass: f64,
    pub energy: f64,
    pub temperature: f64,
    pub liquid: f64,
    pub fill: f64,
}

#[derive(Clone, Debug)]
pub struct PourOutcome {
    pub time: f64,
    pub stop: Stop,
    pub cells: Vec<CellState>,
    pub samples: Vec<Sample>,
    pub mass_entered: f64,
    pub energy_entered: f64,
    pub energy_to_mold: f64,
    pub filled_volume: f64,
    pub front: f64,
    pub steps: u64,
    /// Peak solver arrays for this pour: the stored field plus one step of scratch.
    pub working_bytes: usize,
}

pub fn pour(request: PourRequest<'_>) -> PourOutcome {
    let n = request.mold.cells.len();
    let working_bytes = n * (std::mem::size_of::<f64>() * 7 + std::mem::size_of::<CellState>())
        + request.sample_at.len() * (std::mem::size_of::<bool>() + std::mem::size_of::<Sample>());
    let rho = request.material.rho_liquid;
    let mut mass = vec![0.; n];
    let mut energy = vec![0.; n];
    let capacity: Vec<f64> = request.mold.cells.iter().map(|c| rho * c.volume).collect();
    let mut source = request.source_mass;
    let mut time = 0.;
    let mut steps = 0u64;
    let mut mass_entered = 0.;
    let mut energy_entered = 0.;
    let mut energy_to_mold = 0.;
    let mut samples = Vec::new();
    let mut taken = vec![false; request.sample_at.len()];
    let pressure = rho * GRAVITY * request.head;
    let stop = loop {
        if steps >= request.step_budget {
            break Stop::StepBudget;
        }
        if time >= request.time_limit - 1e-12 {
            break Stop::TimeLimit;
        }
        if source <= 1e-12 {
            break Stop::SourceSpent;
        }
        let state = field(&mass, &energy, request.material);
        if state.iter().any(|c| {
            c.mass > 0.
                && (!c.temperature.is_finite() || !c.energy.is_finite() || !c.liquid.is_finite())
        }) {
            break Stop::Numerical;
        }
        let receiver = mass
            .iter()
            .zip(&capacity)
            .position(|(m, cap)| *m + 1e-12 < *cap);
        let Some(receiver) = receiver else {
            break Stop::Filled;
        };
        let path = if mass[receiver] > 0. {
            receiver + 1
        } else {
            receiver
        };
        if let Some(cell) = (0..path).find(|&i| mass[i] > 0. && state[i].liquid <= 0.) {
            break Stop::Plug { cell };
        }
        let resistance = path_resistance(&request, &state, &mass, &capacity, path);
        let flow = volume_flow(
            pressure,
            resistance,
            rho,
            request.discharge,
            request.gate_area,
        );
        if !flow.is_finite() || flow < 0. {
            break Stop::Numerical;
        }
        let mut dt = request.dt_max.min(request.time_limit - time).max(0.);
        for (i, mark) in request.sample_at.iter().enumerate() {
            if !taken[i] && *mark > time {
                dt = dt.min(mark - time);
            }
        }
        if flow > 0. {
            dt = dt.min(source / (rho * flow));
            for occupied in mass.iter().take(receiver) {
                dt = dt.min(0.25 * occupied / (rho * flow));
            }
            let room = capacity[receiver] - mass[receiver];
            dt = dt.min(room / (rho * flow));
            if mass[receiver] > 0. {
                dt = dt.min(0.25 * mass[receiver] / (rho * flow));
            }
        }
        let conduct = conductances(&request, &state, &mass, &capacity);
        for (i, sum_g) in conduct.sum.iter().enumerate() {
            if mass[i] > 0. && *sum_g > 0. {
                dt = dt.min(0.2 * mass[i] * request.material.specific_heat / sum_g);
            }
        }
        if !(dt.is_finite() && dt > 0.) {
            break Stop::Numerical;
        }
        let mut inflow_e = vec![0.; n];
        let mut outflow_e = vec![0.; n];
        if flow > 0. {
            let mut upstream = liquid_enthalpy(request.material, request.source_temperature);
            for i in 0..receiver {
                let here = liquid_enthalpy(request.material, state[i].temperature);
                inflow_e[i] = rho * flow * upstream;
                outflow_e[i] = rho * flow * here;
                upstream = here;
            }
            inflow_e[receiver] = rho * flow * upstream;
        }
        let mut numerical = false;
        for i in 0..n {
            let receiving = i == receiver && flow > 0.;
            if mass[i] <= 0. && !receiving {
                continue;
            }
            let loss = if mass[i] > 0. {
                let t = state[i].temperature;
                if !t.is_finite() {
                    numerical = true;
                    break;
                }
                request.film
                    * wet_area(&request.mold.cells[i], mass[i], capacity[i])
                    * (t - request.mold_temperature)
            } else {
                0.
            };
            let added = if i == receiver { rho * flow * dt } else { 0. };
            let de = (inflow_e[i] - outflow_e[i] + conduct.into[i] - loss) * dt;
            energy[i] += de;
            mass[i] += added;
            if i == receiver && mass[i] > capacity[i] {
                mass[i] = capacity[i];
            }
            energy_to_mold += loss * dt;
        }
        if numerical || energy.iter().any(|e| !e.is_finite()) {
            break Stop::Numerical;
        }
        let moved = rho * flow * dt;
        source -= moved;
        mass_entered += moved;
        if flow > 0. {
            energy_entered += inflow_e[0] * dt;
        }
        time += dt;
        steps += 1;
        for (i, mark) in request.sample_at.iter().enumerate() {
            if !taken[i] && time + 1e-9 >= *mark {
                taken[i] = true;
                let now = field(&mass, &energy, request.material);
                samples.push(snapshot(&request, &now, &mass, &capacity, time, flow));
            }
        }
        if source <= 1e-12 {
            break Stop::SourceSpent;
        }
    };
    let mut cells = field(&mass, &energy, request.material);
    for (cell, cap) in cells.iter_mut().zip(&capacity) {
        cell.fill = if *cap > 0. {
            (cell.mass / cap).clamp(0., 1.)
        } else {
            0.
        };
    }
    let filled_volume = mass.iter().sum::<f64>() / rho;
    PourOutcome {
        time,
        stop,
        front: front_x(request.mold, &mass, &capacity),
        cells,
        samples,
        mass_entered,
        energy_entered,
        energy_to_mold,
        filled_volume,
        steps,
        working_bytes,
    }
}

struct Conduct {
    into: Vec<f64>,
    sum: Vec<f64>,
}

fn conductances(
    request: &PourRequest<'_>,
    state: &[CellState],
    mass: &[f64],
    capacity: &[f64],
) -> Conduct {
    let n = mass.len();
    let mut into = vec![0.; n];
    let mut sum = vec![0.; n];
    for i in 0..n.saturating_sub(1) {
        if mass[i] <= 0. || mass[i + 1] <= 0. {
            continue;
        }
        let left = &request.mold.cells[i];
        let right = &request.mold.cells[i + 1];
        let gap = (left.length - wet_offset(left.length, mass[i], capacity[i])
            + wet_offset(right.length, mass[i + 1], capacity[i + 1]))
        .max(1e-9);
        let g = request.material.conductivity * left.section.min(right.section) / gap;
        let drive = g * (state[i + 1].temperature - state[i].temperature);
        into[i] += drive;
        into[i + 1] -= drive;
        sum[i] += g;
        sum[i + 1] += g;
    }
    Conduct { into, sum }
}

fn wet_offset(length: f64, mass: f64, capacity: f64) -> f64 {
    0.5 * (mass / capacity).clamp(0., 1.) * length
}

fn wet_area(cell: &crate::crafting_physics::geometry::Cell, mass: f64, capacity: f64) -> f64 {
    if mass <= 0. {
        return 0.;
    }
    cell.wall * (mass / capacity).clamp(0., 1.) + cell.shoulder
}

fn path_resistance(
    request: &PourRequest<'_>,
    state: &[CellState],
    mass: &[f64],
    capacity: &[f64],
    path: usize,
) -> f64 {
    let mut resistance = 0.;
    for i in 0..path {
        if mass[i] <= 0. {
            continue;
        }
        let cell = &request.mold.cells[i];
        let occupied = cell.length * (mass[i] / capacity[i]).clamp(0., 1.);
        let liquid =
            channel_resistance(request.material.viscosity, occupied, cell.wide, cell.narrow);
        let mushy = mushy_resistance(request.material, state[i].liquid, occupied, cell.section);
        if !liquid.is_finite() || !mushy.is_finite() {
            return f64::INFINITY;
        }
        resistance += liquid + mushy;
    }
    resistance
}

fn channel_resistance(mu: f64, length: f64, wide: f64, narrow: f64) -> f64 {
    if length <= 0. {
        return 0.;
    }
    let factor = rectangular_factor(wide, narrow);
    if factor <= 0. {
        return f64::INFINITY;
    }
    12. * mu * length / (wide * narrow.powi(3) * factor)
}

fn mushy_resistance(material: &Material, liquid: f64, length: f64, section: f64) -> f64 {
    if length <= 0. || liquid >= 1. {
        return 0.;
    }
    if liquid <= 0. || section <= 0. {
        return f64::INFINITY;
    }
    let permeability = material.permeability_scale.powi(2) * liquid.powi(3)
        / (material.kozeny * (1. - liquid).powi(2));
    if permeability <= 0. {
        return f64::INFINITY;
    }
    material.viscosity * length / (permeability * section)
}

fn rectangular_factor(wide: f64, narrow: f64) -> f64 {
    let ratio = std::f64::consts::PI * wide / (2. * narrow);
    let mut sum = 0.;
    let mut n = 1.;
    while n <= 59. {
        sum += (n * ratio).tanh() / n.powi(5);
        n += 2.;
    }
    1. - 192. * narrow / (std::f64::consts::PI.powi(5) * wide) * sum
}

pub fn volume_flow(pressure: f64, resistance: f64, rho: f64, discharge: f64, gate: f64) -> f64 {
    if pressure <= 0. || discharge <= 0. || gate <= 0. {
        return 0.;
    }
    if !resistance.is_finite() || resistance < 0. {
        return 0.;
    }
    let bernoulli = rho / (2. * discharge.powi(2) * gate.powi(2));
    if resistance == 0. {
        return (pressure / bernoulli).sqrt();
    }
    let under = resistance * resistance + 4. * bernoulli * pressure;
    if !under.is_finite() {
        return pressure / resistance;
    }
    2. * pressure / (resistance + under.sqrt())
}

fn field(mass: &[f64], energy: &[f64], material: &Material) -> Vec<CellState> {
    mass.iter()
        .zip(energy)
        .map(|(m, e)| {
            if *m <= 0. {
                CellState {
                    mass: 0.,
                    energy: 0.,
                    temperature: f64::NAN,
                    liquid: f64::NAN,
                    fill: 0.,
                }
            } else {
                let (t, f, _) = temperature(material, e / m);
                CellState {
                    mass: *m,
                    energy: *e,
                    temperature: t,
                    liquid: f,
                    fill: 0.,
                }
            }
        })
        .collect()
}

fn snapshot(
    request: &PourRequest<'_>,
    state: &[CellState],
    mass: &[f64],
    capacity: &[f64],
    time: f64,
    flow: f64,
) -> Sample {
    let coldest = state
        .iter()
        .filter(|c| c.mass > 0.)
        .map(|c| c.liquid)
        .fold(1., f64::min);
    Sample {
        time,
        flow,
        front: front_x(request.mold, mass, capacity),
        filled: mass.iter().sum::<f64>() / request.material.rho_liquid,
        coldest_liquid: coldest,
    }
}

fn front_x(mold: &Mold, mass: &[f64], capacity: &[f64]) -> f64 {
    let mut front = mold.cells.first().map(|c| c.x0).unwrap_or(0.);
    for (i, cell) in mold.cells.iter().enumerate() {
        if mass[i] <= 0. {
            break;
        }
        let frac = (mass[i] / capacity[i]).clamp(0., 1.);
        front = cell.x0 + frac * (cell.x1 - cell.x0);
        if frac < 1. - 1e-8 {
            break;
        }
    }
    front
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crafting_physics::{c90700, reference_sword};

    #[test]
    fn permeability_falls_as_the_metal_freezes_and_flow_stays_possible() {
        let metal = c90700();
        let half = mushy_resistance(&metal, 0.5, 0.01, 240e-6);
        let thinner = mushy_resistance(&metal, 0.2, 0.01, 240e-6);
        assert!(thinner > half);
        let flow = volume_flow(11_772., half, metal.rho_liquid, 0.65, 25e-6);
        assert!(flow.is_finite() && flow > 0.);
        assert_eq!(mushy_resistance(&metal, 1., 0.01, 240e-6), 0.);
        assert!(mushy_resistance(&metal, 0., 0.01, 240e-6).is_infinite());
        assert_eq!(volume_flow(0., 0., metal.rho_liquid, 0.65, 25e-6), 0.);
        let open = volume_flow(11_772., 0., metal.rho_liquid, 0.65, 25e-6);
        assert!((open - 27.877e-6).abs() < 0.02e-6, "{open}");
    }

    fn reference<'a>(
        mold: &'a Mold,
        material: &'a crate::crafting_physics::Material,
        gate: f64,
        dt_max: f64,
        samples: &'a [f64],
    ) -> PourRequest<'a> {
        PourRequest {
            mold,
            material,
            gate_area: gate,
            head: 0.15,
            discharge: 0.65,
            mold_temperature: 200.,
            film: 60.,
            source_temperature: 1040.,
            source_mass: 1.8,
            dt_max,
            time_limit: 200.,
            step_budget: 2_000_000,
            sample_at: samples,
        }
    }

    #[test]
    fn a_wide_gate_fills_the_cavity_while_it_is_still_liquid() {
        let mold = reference_sword(0.0025);
        let metal = c90700();
        let started = std::time::Instant::now();
        let outcome = pour(reference(&mold, &metal, 25e-6, 0.005, &[]));
        let elapsed = started.elapsed();
        eprintln!(
            "wide reference pour: {elapsed:?}, {} bytes, {} cells",
            outcome.working_bytes,
            outcome.cells.len()
        );
        assert!(
            elapsed.as_secs_f64() < 5.,
            "wide reference took {elapsed:?}"
        );
        assert!(
            outcome.working_bytes < 1_048_576,
            "wide working set {}",
            outcome.working_bytes
        );
        assert_eq!(outcome.stop, Stop::Filled);
        assert!((outcome.time - 6.13).abs() < 0.01, "{}", outcome.time);
        let coldest = outcome
            .cells
            .iter()
            .filter(|c| c.mass > 0.)
            .map(|c| c.temperature)
            .fold(f64::INFINITY, f64::min);
        assert!((coldest - 1004.).abs() < 1., "{coldest}");
        assert!((outcome.filled_volume - cavity(&mold)).abs() < 1e-8);
        assert!(outcome.mass_entered <= 1.8);
        balance(&outcome);
    }

    #[test]
    fn closing_the_feed_early_keeps_a_partial_fill() {
        let mold = reference_sword(0.0025);
        let metal = c90700();
        let mut request = reference(&mold, &metal, 25e-6, 0.005, &[]);
        request.time_limit = 2.;
        let outcome = pour(request);
        assert_eq!(outcome.stop, Stop::TimeLimit);
        assert!((outcome.time - 2.).abs() < 0.01, "{}", outcome.time);
        assert!(outcome.mass_entered > 0.);
        assert!(outcome.filled_volume < cavity(&mold) * 0.5);
        balance(&outcome);
    }

    #[test]
    fn an_empty_crucible_is_not_a_numerical_failure() {
        let mold = reference_sword(0.0025);
        let metal = c90700();
        let mut request = reference(&mold, &metal, 25e-6, 0.005, &[]);
        request.source_mass = 0.;
        let outcome = pour(request);
        assert_eq!(outcome.stop, Stop::SourceSpent);
        assert_eq!(outcome.mass_entered, 0.);
    }

    #[test]
    fn a_narrow_gate_freezes_downstream_of_the_tang() {
        let mold = reference_sword(0.0025);
        let metal = c90700();
        let marks = [20., 40., 70., 100.];
        let started = std::time::Instant::now();
        let outcome = pour(reference(&mold, &metal, 2e-6, 0.005, &marks));
        let elapsed = started.elapsed();
        eprintln!(
            "narrow reference pour: {elapsed:?}, {} bytes, {} cells",
            outcome.working_bytes,
            outcome.cells.len()
        );
        assert!(
            elapsed.as_secs_f64() < 45.,
            "narrow reference took {elapsed:?}"
        );
        assert!(
            outcome.working_bytes < 1_048_576,
            "narrow working set {}",
            outcome.working_bytes
        );
        let Stop::Plug { cell } = outcome.stop else {
            panic!("{:?}", outcome.stop);
        };
        assert!((outcome.time - 137.41).abs() < 0.05, "{}", outcome.time);
        let plug = &mold.cells[cell];
        assert!((plug.x0 - 0.095).abs() < 1e-9 && (plug.x1 - 0.0975).abs() < 1e-9);
        assert!((outcome.front - 0.3221).abs() < 0.0005, "{}", outcome.front);
        assert!((outcome.filled_volume * 1e6 - 105.26).abs() < 0.05);
        let at = |t: f64| {
            outcome
                .samples
                .iter()
                .find(|s| (s.time - t).abs() < 1e-6)
                .unwrap()
        };
        let s20 = at(20.);
        assert!((s20.flow * 1e6 - 2.192).abs() < 0.01, "{}", s20.flow);
        assert!((s20.front * 100. - 9.69).abs() < 0.05, "{}", s20.front);
        let s40 = at(40.);
        assert!((s40.flow * 1e6 - 1.518).abs() < 0.01);
        assert!((s40.front * 100. - 23.71).abs() < 0.05);
        let s70 = at(70.);
        assert!((s70.flow * 1e6 - 0.156).abs() < 0.005);
        assert!((s70.coldest_liquid - 0.499).abs() < 0.01);
        let s100 = at(100.);
        assert!((s100.flow * 1e6 - 0.0113).abs() < 0.001);
        assert!((s100.front * 100. - 32.17).abs() < 0.05);
        balance(&outcome);
    }

    #[test]
    fn the_same_request_pours_the_same_blank() {
        let mold = reference_sword(0.0025);
        let metal = c90700();
        let first = pour(reference(&mold, &metal, 25e-6, 0.005, &[]));
        let second = pour(reference(&mold, &metal, 25e-6, 0.005, &[]));
        assert_eq!(first.stop, second.stop);
        assert!((first.time - second.time).abs() < 1e-9);
        assert!((first.mass_entered - second.mass_entered).abs() < 1e-9);
        assert!((first.filled_volume - second.filled_volume).abs() < 1e-12);
    }

    #[test]
    fn a_finer_step_does_not_move_the_plug() {
        let mold = reference_sword(0.0025);
        let metal = c90700();
        let coarse = pour(reference(&mold, &metal, 2e-6, 0.005, &[]));
        let fine = pour(reference(&mold, &metal, 2e-6, 0.0025, &[]));
        assert!(
            (fine.time - coarse.time).abs() < 0.01,
            "{} {}",
            fine.time,
            coarse.time
        );
        assert!((fine.front - coarse.front).abs() < 0.0001);
    }

    #[test]
    fn grid_and_permeability_move_the_defect_as_published() {
        let metal = c90700();
        let mid = reference_sword(0.005);
        let base = pour(reference(&mid, &metal, 2e-6, 0.005, &[]));
        let Stop::Plug { cell } = base.stop else {
            panic!("{:?}", base.stop)
        };
        assert!((base.time - 137.76).abs() < 0.2, "{}", base.time);
        assert!((mid.cells[cell].x0 - 0.095).abs() < 1e-9);
        assert!((base.front - 0.3235).abs() < 0.002, "{}", base.front);
        assert!((base.filled_volume * 1e6 - 105.61).abs() < 0.2);

        let mut tight = metal;
        tight.permeability_scale = 50e-6;
        let early = pour(reference(&mid, &tight, 2e-6, 0.005, &[]));
        assert!((early.front - 0.2186).abs() < 0.01, "{}", early.front);
        let mut open = metal;
        open.permeability_scale = 200e-6;
        let late = pour(reference(&mid, &open, 2e-6, 0.005, &[]));
        assert!((late.front - 0.4581).abs() < 0.01, "{}", late.front);

        let dense = reference_sword(0.00125);
        let small = pour(reference(&dense, &metal, 2e-6, 0.0025, &[]));
        let Stop::Plug { cell } = small.stop else {
            panic!("{:?}", small.stop)
        };
        assert!((small.time - 137.23).abs() < 0.2, "{}", small.time);
        assert!((dense.cells[cell].x0 - 0.095).abs() < 1e-9);
        assert!((small.front - 0.3214).abs() < 0.002, "{}", small.front);
        assert!((small.filled_volume * 1e6 - 105.07).abs() < 0.2);
    }

    fn cavity(mold: &Mold) -> f64 {
        mold.cells.iter().map(|c| c.volume).sum()
    }

    fn balance(outcome: &PourOutcome) {
        let stored: f64 = outcome.cells.iter().map(|c| c.energy).sum();
        let residual = outcome.energy_entered - stored - outcome.energy_to_mold;
        assert!(residual.abs() < 1e-6, "{residual}");
        let mass: f64 = outcome.cells.iter().map(|c| c.mass).sum();
        assert!((mass - outcome.mass_entered).abs() < 1e-9);
    }
}
