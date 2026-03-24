pub mod types;

#[allow(unused_imports)]
pub use crate::sim::domesticates::types::*;

use crate::sim::world::World;

pub(crate) fn update_domesticates(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let n = world.state.geology.height.len();
    for i in 0..n {
        if world.state.geology.height[i] <= 0.0 {
            world.state.domesticates.crop_available[i] = 0;
            world.state.domesticates.livestock_available[i] = 0;
            world.state.domesticates.crop_adoption[i] = 0.0;
            world.state.domesticates.livestock_adoption[i] = 0.0;
            continue;
        }
        let tree_cover = world.state.ecology.tree_cover[i].clamp(0.0, 1.0);
        let ground_cover = world.state.ecology.ground_cover[i].clamp(0.0, 1.0);
        let soil_fertility = world.state.ecology.soil_fertility[i].clamp(0.0, 1.0);
        let vegetation_proxy =
            (tree_cover + 0.6 * ground_cover * (1.0 - tree_cover)).clamp(0.0, 1.0);
        let eco_suitability = (vegetation_proxy * 0.6 + soil_fertility * 0.4).clamp(0.0, 1.0);
        world.state.domesticates.crop_available[i] = if eco_suitability > 0.35 { 1 } else { 0 };
        world.state.domesticates.livestock_available[i] =
            if eco_suitability > 0.25 { 1 } else { 0 };
        world.state.domesticates.crop_adoption[i] =
            if world.state.domesticates.crop_available[i] > 0 {
                eco_suitability
            } else {
                0.0
            };
        world.state.domesticates.livestock_adoption[i] =
            if world.state.domesticates.livestock_available[i] > 0 {
                (eco_suitability * 0.9).clamp(0.0, 1.0)
            } else {
                0.0
            };
    }
}
