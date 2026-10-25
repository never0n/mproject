//! Numerical casting core. No database, UI, or network.
//!
//! Coefficients are the [PROPOSAL] in `game-design/economy/crafting-bronze-example.md`
//! and `CURSOR_CRAFTING_PHYSICS.md`. They are not measured bronze properties and
//! they do not change combat. SI units inside the solver: m, m², m³, kg, s, Pa, J.
//! Temperature is °C with enthalpy relative to 20 °C.

mod body;
mod enthalpy;
mod geometry;
mod pour;

pub use body::{
    Assembly, Bevel, FeedAccount, FeedMode, SolidCell, assembly, bevel_cells, cool_lumped,
    feed_account, lumped_cool, machine_profile, mass_axes,
};
pub use enthalpy::{Material, Phase, c90700, enthalpy, heat, liquid_fraction, temperature};
pub use geometry::{Mold, cavity_volume, drawing_totals, extrude, reference_sword, shrinkage};
pub use pour::{PourOutcome, PourRequest, Sample, Stop, pour};
