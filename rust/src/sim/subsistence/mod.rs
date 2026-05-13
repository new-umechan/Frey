pub mod types;

#[allow(unused_imports)]
pub use crate::sim::subsistence::types::*;

use crate::sim::exec::lerp;
use crate::sim::world::{SubsistenceMix, World};

#[derive(Clone, Copy)]
struct AccessState {
    eco: f32,
    inland_aquatic_access: f32,
    coastal_aquatic_access: f32,
    freshwater: f32,
    ground_cover: f32,
    tree_cover: f32,
}

#[derive(Clone, Copy)]
struct CapabilityState {
    crop: f32,
    livestock: f32,
}

#[derive(Clone, Copy)]
struct PressureState {
    pop_pressure: f32,
}

#[derive(Clone, Copy)]
struct OutputState {
    food_energy_mean: f32,
    food_energy_variance: f32,
    buffer_capacity: f32,
    mobility_capacity: f32,
    land_use_intensity: f32,
}

pub(crate) fn update_subsistence(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let alpha = 0.2_f32;
    let n = world.state.geology.height.len();
    for i in 0..n {
        if !world.is_land_cell(i) {
            world.state.subsistence.subsistence_mix[i] = SubsistenceMix::default();
            world.state.subsistence.food_energy_mean[i] = 0.0;
            world.state.subsistence.food_energy_variance[i] = 1.0;
            world.state.subsistence.buffer_capacity[i] = 0.0;
            world.state.subsistence.mobility_capacity[i] = 0.0;
            world.state.subsistence.land_use_intensity[i] = 0.0;
            continue;
        }
        let access = compute_access_state(world, i);
        let capability = compute_capability_state(world, i);
        let pressure = compute_pressure_state(world, i);
        let target_mix = compute_strategy_mix(access, capability, pressure);
        let next_mix = lerp_mix(
            world.state.subsistence.subsistence_mix[i],
            target_mix,
            alpha * 0.5,
        );
        let next_mix = normalize_mix(next_mix);
        world.state.subsistence.subsistence_mix[i] = next_mix;
        let output = compute_output_state(next_mix, access, pressure);
        world.state.subsistence.food_energy_mean[i] = lerp(
            world.state.subsistence.food_energy_mean[i],
            output.food_energy_mean,
            alpha,
        );
        world.state.subsistence.food_energy_variance[i] = lerp(
            world.state.subsistence.food_energy_variance[i],
            output.food_energy_variance,
            alpha,
        );
        world.state.subsistence.buffer_capacity[i] = lerp(
            world.state.subsistence.buffer_capacity[i],
            output.buffer_capacity,
            alpha,
        );
        world.state.subsistence.mobility_capacity[i] = lerp(
            world.state.subsistence.mobility_capacity[i],
            output.mobility_capacity,
            alpha,
        );
        world.state.subsistence.land_use_intensity[i] = lerp(
            world.state.subsistence.land_use_intensity[i],
            output.land_use_intensity,
            alpha,
        );
        let _ = budget;
    }
}

fn compute_access_state(world: &World, index: usize) -> AccessState {
    let tree_cover = world.state.ecology.tree_cover[index].clamp(0.0, 1.0);
    let ground_cover = world.state.ecology.ground_cover[index].clamp(0.0, 1.0);
    let soil_fertility = world.state.ecology.soil_fertility[index].clamp(0.0, 1.0);
    let vegetation_proxy = (tree_cover + 0.6 * ground_cover * (1.0 - tree_cover)).clamp(0.0, 1.0);
    let eco = (vegetation_proxy * 0.55 + soil_fertility * 0.45).clamp(0.0, 1.0);
    let freshwater = world.state.hydrology.surface_water_access[index].clamp(0.0, 1.0);
    let inland_aquatic_access = freshwater;
    let coastal_aquatic_access = if world.projections.terrain.is_coastal[index] {
        (0.45 + freshwater * 0.55).clamp(0.0, 1.0)
    } else {
        0.0
    };
    AccessState {
        eco,
        inland_aquatic_access,
        coastal_aquatic_access,
        freshwater,
        ground_cover,
        tree_cover,
    }
}

fn compute_capability_state(world: &World, index: usize) -> CapabilityState {
    let crop = world.state.domesticates.crop_adoption[index]
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0);
    let livestock = world.state.domesticates.livestock_adoption[index]
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0);
    CapabilityState { crop, livestock }
}

fn compute_pressure_state(world: &World, index: usize) -> PressureState {
    let pop = world.state.population.population[index].max(0.0);
    PressureState {
        pop_pressure: (pop / 120.0).clamp(0.0, 1.0),
    }
}

fn compute_strategy_mix(
    access: AccessState,
    capability: CapabilityState,
    pressure: PressureState,
) -> SubsistenceMix {
    let fishing_access =
        (access.inland_aquatic_access * 0.6 + access.coastal_aquatic_access * 0.4).clamp(0.0, 1.0);
    normalize_mix(SubsistenceMix {
        gathering: (access.eco * (1.0 - capability.crop)).clamp(0.0, 1.0),
        hunting: ((access.tree_cover * 0.6 + access.ground_cover * 0.2) * (1.0 - capability.crop))
            .clamp(0.0, 1.0),
        fishing: fishing_access,
        cultivation: (capability.crop * (0.35 + access.eco * 0.45 + pressure.pop_pressure * 0.35))
            .clamp(0.0, 1.0),
        herding: (capability.livestock
            * (0.45 + access.ground_cover * 0.40 + (1.0 - pressure.pop_pressure) * 0.25))
            .clamp(0.0, 1.0),
    })
}

fn compute_output_state(
    mix: SubsistenceMix,
    access: AccessState,
    pressure: PressureState,
) -> OutputState {
    let mobility = (mix.hunting * 0.5 + mix.gathering * 0.35 + mix.herding * 0.9).clamp(0.0, 1.0);
    let buffer = (mix.cultivation * 0.55
        + mix.herding * 0.35
        + mix.fishing * 0.30
        + access.freshwater * 0.20)
        .clamp(0.0, 1.0);
    let mean = (mix.gathering * 0.45
        + mix.hunting * 0.50
        + mix.fishing * 0.65
        + mix.cultivation * 0.95
        + mix.herding * 0.78)
        .clamp(0.0, 1.0);
    let variance = (0.55 - buffer * 0.25 - mobility * 0.20
        + (1.0 - access.freshwater) * 0.20
        + (1.0 - mix.cultivation).max(0.0) * 0.10)
        .clamp(0.0, 1.0);
    let land_use = (mix.cultivation * (0.5 + pressure.pop_pressure * 0.5)
        + mix.herding * 0.35
        + pressure.pop_pressure * 0.25)
        .clamp(0.0, 1.0);
    OutputState {
        food_energy_mean: mean,
        food_energy_variance: variance,
        buffer_capacity: buffer,
        mobility_capacity: mobility,
        land_use_intensity: land_use,
    }
}

fn lerp_mix(from: SubsistenceMix, to: SubsistenceMix, alpha: f32) -> SubsistenceMix {
    SubsistenceMix {
        gathering: lerp(from.gathering, to.gathering, alpha),
        hunting: lerp(from.hunting, to.hunting, alpha),
        fishing: lerp(from.fishing, to.fishing, alpha),
        cultivation: lerp(from.cultivation, to.cultivation, alpha),
        herding: lerp(from.herding, to.herding, alpha),
    }
}

fn normalize_mix(mut mix: SubsistenceMix) -> SubsistenceMix {
    let sum = mix.gathering + mix.hunting + mix.fishing + mix.cultivation + mix.herding;
    if sum <= 1e-6 {
        return SubsistenceMix {
            gathering: 0.4,
            hunting: 0.3,
            fishing: 0.2,
            cultivation: 0.1,
            herding: 0.0,
        };
    }
    mix.gathering /= sum;
    mix.hunting /= sum;
    mix.fishing /= sum;
    mix.cultivation /= sum;
    mix.herding /= sum;
    mix
}
