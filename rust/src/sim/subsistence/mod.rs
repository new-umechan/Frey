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
    let max_flow = world
        .state
        .hydrology
        .river_flow
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    for i in 0..n {
        if world.state.geology.height[i] <= 0.0 {
            world.state.subsistence.subsistence_mix[i] = SubsistenceMix::default();
            world.state.subsistence.food_production[i] = 0.0;
            world.state.subsistence.freshwater_access[i] = 0.0;
            continue;
        }
        let tree_cover = world.state.ecology.tree_cover[i].clamp(0.0, 1.0);
        let ground_cover = world.state.ecology.ground_cover[i].clamp(0.0, 1.0);
        let soil_fertility = world.state.ecology.soil_fertility[i].clamp(0.0, 1.0);
        let vegetation_proxy =
            (tree_cover + 0.6 * ground_cover * (1.0 - tree_cover)).clamp(0.0, 1.0);
        let eco = (vegetation_proxy * 0.55 + soil_fertility * 0.45).clamp(0.0, 1.0);
        let river = (world.state.hydrology.river_flow[i] / max_flow).clamp(0.0, 1.0);
        let lake_bonus = if world.state.hydrology.is_lake[i] {
            0.2
        } else {
            0.0
        };
        let freshwater = (river + lake_bonus).clamp(0.0, 1.0);
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

        let target_mix = normalize_mix(SubsistenceMix {
            gathering: (eco * (1.0 - crop)).clamp(0.0, 1.0),
            hunting: ((tree_cover * 0.6 + ground_cover * 0.2) * (1.0 - crop)).clamp(0.0, 1.0),
            fishing: freshwater,
            farming: (crop * (0.4 + eco * 0.6)).clamp(0.0, 1.0),
            pastoralism: (livestock * (0.5 + ground_cover * 0.5)).clamp(0.0, 1.0),
        });
        let next_mix = lerp_mix(
            world.state.subsistence.subsistence_mix[i],
            target_mix,
            alpha * 0.5,
        );
        let next_mix = normalize_mix(next_mix);
        world.state.subsistence.subsistence_mix[i] = next_mix;
        world.state.subsistence.freshwater_access[i] = lerp(
            world.state.subsistence.freshwater_access[i],
            freshwater,
            alpha,
        );
        let food = (next_mix.gathering * 0.45
            + next_mix.hunting * 0.50
            + next_mix.fishing * 0.55
            + next_mix.farming * 0.95
            + next_mix.pastoralism * 0.80)
            .clamp(0.0, 1.0);
        world.state.subsistence.food_production[i] = lerp(
            world.state.subsistence.food_production[i],
            food,
            alpha * budget.max(1) as f32,
        );
    }
}

fn lerp_mix(from: SubsistenceMix, to: SubsistenceMix, alpha: f32) -> SubsistenceMix {
    SubsistenceMix {
        gathering: lerp(from.gathering, to.gathering, alpha),
        hunting: lerp(from.hunting, to.hunting, alpha),
        fishing: lerp(from.fishing, to.fishing, alpha),
        farming: lerp(from.farming, to.farming, alpha),
        pastoralism: lerp(from.pastoralism, to.pastoralism, alpha),
    }
}

fn normalize_mix(mut mix: SubsistenceMix) -> SubsistenceMix {
    let sum = mix.gathering + mix.hunting + mix.fishing + mix.farming + mix.pastoralism;
    if sum <= 1e-6 {
        return SubsistenceMix {
            gathering: 0.4,
            hunting: 0.3,
            fishing: 0.2,
            farming: 0.1,
            pastoralism: 0.0,
        };
    }
    mix.gathering /= sum;
    mix.hunting /= sum;
    mix.fishing /= sum;
    mix.farming /= sum;
    mix.pastoralism /= sum;
    mix
}
