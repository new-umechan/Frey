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
            world.state.domesticates.crop_adopted[i] = 0;
            world.state.domesticates.livestock_adopted[i] = 0;
            continue;
        }
        let hab = world.state.ecology.habitability[i];
        world.state.domesticates.crop_available[i] = if hab > 0.35 { 1 } else { 0 };
        world.state.domesticates.livestock_available[i] = if hab > 0.25 { 1 } else { 0 };
    }
}
