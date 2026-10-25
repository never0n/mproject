//! Enthalpy of one material. Energy is added and removed; temperature is recovered from it.

/// C90700 as used by the bronze-sword proposal. Not a measured certificate.
pub fn c90700() -> Material {
    Material {
        id: "C90700",
        version: 1,
        specific_heat: 380.,
        latent: 180_000.,
        solidus: 831.,
        liquidus: 999.,
        rho_liquid: 8_000.,
        rho_cold: 8_770.,
        viscosity: 0.004,
        conductivity: 70.,
        permeability_scale: 100e-6,
        kozeny: 180.,
        reference_c: 20.,
        elastic_modulus: 103e9,
        yield_strength: 150e6,
        hardness_hb: 80.,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub id: &'static str,
    pub version: u32,
    pub specific_heat: f64,
    pub latent: f64,
    pub solidus: f64,
    pub liquidus: f64,
    pub rho_liquid: f64,
    pub rho_cold: f64,
    pub viscosity: f64,
    pub conductivity: f64,
    pub permeability_scale: f64,
    pub kozeny: f64,
    pub reference_c: f64,
    /// Handbook inputs. Not a result of the cooling calculation.
    pub elastic_modulus: f64,
    pub yield_strength: f64,
    pub hardness_hb: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Solid,
    Transition,
    Liquid,
}

pub fn liquid_fraction(material: &Material, temperature_c: f64) -> f64 {
    let span = material.liquidus - material.solidus;
    ((temperature_c - material.solidus) / span).clamp(0., 1.)
}

/// Specific enthalpy relative to [`Material::reference_c`], J/kg.
pub fn enthalpy(material: &Material, temperature_c: f64) -> f64 {
    material.specific_heat * (temperature_c - material.reference_c)
        + material.latent * liquid_fraction(material, temperature_c)
}

/// Liquid-phase enthalpy carried by flow, including the full latent heat.
pub fn liquid_enthalpy(material: &Material, temperature_c: f64) -> f64 {
    material.specific_heat * (temperature_c - material.reference_c) + material.latent
}

/// Invert [`enthalpy`]. Solid, transition and liquid are separate pieces.
pub fn temperature(material: &Material, specific: f64) -> (f64, f64, Phase) {
    let solid_edge = enthalpy(material, material.solidus);
    let liquid_edge = enthalpy(material, material.liquidus);
    if specific <= solid_edge {
        let t = material.reference_c + specific / material.specific_heat;
        return (t, 0., Phase::Solid);
    }
    if specific >= liquid_edge {
        let t = material.reference_c + (specific - material.latent) / material.specific_heat;
        return (t, 1., Phase::Liquid);
    }
    let c_eff = material.specific_heat + material.latent / (material.liquidus - material.solidus);
    let t = material.solidus + (specific - solid_edge) / c_eff;
    let f = liquid_fraction(material, t);
    (t, f, Phase::Transition)
}

/// Heat `mass` kg from `from_c` to `to_c` at a constant net power into the metal.
pub fn heat(material: &Material, mass: f64, from_c: f64, to_c: f64, power: f64) -> (f64, f64) {
    let energy = mass * (enthalpy(material, to_c) - enthalpy(material, from_c));
    let seconds = if power > 0. {
        energy / power
    } else {
        f64::INFINITY
    };
    (energy, seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heating_the_reference_charge_matches_the_proposal() {
        let metal = c90700();
        let (energy, seconds) = heat(&metal, 1.8, 20., 1070., 1000.);
        assert!((energy - 1_042_200.).abs() < 1., "{energy}");
        assert!((seconds - 1042.2).abs() < 0.05, "{seconds}");
        let loss = 1.8 * (enthalpy(&metal, 1070.) - enthalpy(&metal, 1040.));
        assert!((loss - 20_520.).abs() < 1., "{loss}");
    }

    #[test]
    fn fraction_limits_do_not_produce_non_finite_enthalpy() {
        let metal = c90700();
        for t in [20., 831., 900., 999., 1040., 1070.] {
            let e = enthalpy(&metal, t);
            let (back, f, _) = temperature(&metal, e);
            assert!(e.is_finite() && f.is_finite() && back.is_finite(), "{t}");
            assert!((back - t).abs() < 1e-9, "{t} -> {back}");
            assert!((0. ..=1.).contains(&f));
        }
        assert_eq!(liquid_fraction(&metal, 831.), 0.);
        assert_eq!(liquid_fraction(&metal, 999.), 1.);
        let (t, f, phase) = temperature(&metal, enthalpy(&metal, 20.));
        assert!((t - 20.).abs() < 1e-9);
        assert_eq!(f, 0.);
        assert_eq!(phase, Phase::Solid);
    }
}
