use super::geology_river_budget;
use super::river::run_river_step;
use super::terrain::run_terrain_step;
use crate::sim::world::World;

pub(super) fn run_geology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    run_terrain_step(world);
    run_river_step(world, geology_river_budget(world.exec.era, budget));
}
