pub mod types;

#[allow(unused_imports)]
pub use crate::sim::subsistence::types::*;

use crate::sim::exec::lerp;
use crate::sim::world::{SubsistenceMix, World};

pub(crate) fn update_subsistence(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let alpha = 0.2_f32;
    let n = world.state.geology.height.len();
    for i in 0..n {
        if world.state.geology.height[i] <= 0.0 {
            world.state.subsistence.subsistence_mix[i] = SubsistenceMix::default();
            world.state.subsistence.food_energy_mean[i] = 0.0;
            world.state.subsistence.food_energy_variance[i] = 1.0;
            world.state.subsistence.buffer_capacity[i] = 0.0;
            world.state.subsistence.mobility_capacity[i] = 0.0;
            world.state.subsistence.land_use_intensity[i] = 0.0;
            continue;
        }
        let tree_cover = world.state.ecology.tree_cover[i].clamp(0.0, 1.0);
        let ground_cover = world.state.ecology.ground_cover[i].clamp(0.0, 1.0);
        let soil_fertility = world.state.ecology.soil_fertility[i].clamp(0.0, 1.0);
        let vegetation_proxy =
            (tree_cover + 0.6 * ground_cover * (1.0 - tree_cover)).clamp(0.0, 1.0);
        let eco = (vegetation_proxy * 0.55 + soil_fertility * 0.45).clamp(0.0, 1.0);
        let freshwater = world.state.hydrology.surface_water_access[i].clamp(0.0, 1.0);
        let inland_aquatic_access = freshwater;
        let coastal_aquatic_access = if world.projections.terrain.is_coastal[i] {
            (0.45 + freshwater * 0.55).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fishing_access = (inland_aquatic_access * 0.6 + coastal_aquatic_access * 0.4).clamp(0.0, 1.0);
        let crop = world.state.domesticates.crop_adoption[i]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
            .clamp(0.0, 1.0);
        let livestock = world.state.domesticates.livestock_adoption[i]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
            .clamp(0.0, 1.0);

        let pop = world.state.population.population[i].max(0.0);
        let pop_pressure = (pop / 120.0).clamp(0.0, 1.0);
        let target_mix = normalize_mix(SubsistenceMix {
            gathering: (eco * (1.0 - crop)).clamp(0.0, 1.0),
            hunting: ((tree_cover * 0.6 + ground_cover * 0.2) * (1.0 - crop)).clamp(0.0, 1.0),
            fishing: fishing_access,
            cultivation: (crop * (0.35 + eco * 0.45 + pop_pressure * 0.35)).clamp(0.0, 1.0),
            herding: (livestock * (0.45 + ground_cover * 0.40 + (1.0 - pop_pressure) * 0.25))
                .clamp(0.0, 1.0),
        });
        let next_mix = lerp_mix(
            world.state.subsistence.subsistence_mix[i],
            target_mix,
            alpha * 0.5,
        );
        let next_mix = normalize_mix(next_mix);
        world.state.subsistence.subsistence_mix[i] = next_mix;
        let mobility = (next_mix.hunting * 0.5 + next_mix.gathering * 0.35 + next_mix.herding * 0.9)
            .clamp(0.0, 1.0);
        let buffer = (next_mix.cultivation * 0.55
            + next_mix.herding * 0.35
            + next_mix.fishing * 0.30
            + freshwater * 0.20)
            .clamp(0.0, 1.0);
        let mean = (next_mix.gathering * 0.45
            + next_mix.hunting * 0.50
            + next_mix.fishing * 0.65
            + next_mix.cultivation * 0.95
            + next_mix.herding * 0.78)
            .clamp(0.0, 1.0);
        let variance_raw = (0.55
            - buffer * 0.25
            - mobility * 0.20
            + (1.0 - freshwater) * 0.20
            + (1.0 - next_mix.cultivation).max(0.0) * 0.10)
            .clamp(0.0, 1.0);
        world.state.subsistence.food_energy_mean[i] =
            lerp(world.state.subsistence.food_energy_mean[i], mean, alpha);
        world.state.subsistence.food_energy_variance[i] = lerp(
            world.state.subsistence.food_energy_variance[i],
            variance_raw,
            alpha,
        );
        world.state.subsistence.buffer_capacity[i] =
            lerp(world.state.subsistence.buffer_capacity[i], buffer, alpha);
        world.state.subsistence.mobility_capacity[i] = lerp(
            world.state.subsistence.mobility_capacity[i],
            mobility,
            alpha,
        );
        let land_use = (next_mix.cultivation * (0.5 + pop_pressure * 0.5)
            + next_mix.herding * 0.35
            + pop_pressure * 0.25)
            .clamp(0.0, 1.0);
        world.state.subsistence.land_use_intensity[i] = lerp(
            world.state.subsistence.land_use_intensity[i],
            land_use,
            alpha,
        );
        let _ = budget;
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
