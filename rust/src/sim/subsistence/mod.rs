pub mod types;

#[allow(unused_imports)]
pub use crate::sim::subsistence::types::*;

use crate::sim::exec::lerp;
use crate::sim::world::{
    CellFieldId, FeedbackEntry, FeedbackPayload, FieldValue, ModuleId, TargetRef, World,
};

pub(crate) fn update_subsistence(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let alpha = 0.2_f32;
    let n = world.state.geology.height.len();
    let mut water_withdrawal = vec![0.0_f32; n];
    let mut dam_pressure = vec![0.0_f32; n];
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
            world.state.subsistence.subsistence_mix[i] = 0.0;
            world.state.subsistence.food_production[i] = 0.0;
            world.state.subsistence.land_use[i] = 0.0;
            continue;
        }
        let tree_cover = world.state.ecology.tree_cover[i].clamp(0.0, 1.0);
        let ground_cover = world.state.ecology.ground_cover[i].clamp(0.0, 1.0);
        let soil_fertility = world.state.ecology.soil_fertility[i].clamp(0.0, 1.0);
        let vegetation_proxy =
            (tree_cover + 0.6 * ground_cover * (1.0 - tree_cover)).clamp(0.0, 1.0);
        let eco = (vegetation_proxy * 0.55 + soil_fertility * 0.45).clamp(0.0, 1.0);
        let river = (world.state.hydrology.river_flow[i] / max_flow).clamp(0.0, 1.0);
        let lake_bonus = if world.state.hydrology.is_lake[i] { 0.2 } else { 0.0 };
        let freshwater = (river + lake_bonus).clamp(0.0, 1.0);
        world.state.subsistence.freshwater_access[i] = freshwater;
        let crop = world.state.domesticates.crop_adoption[i].clamp(0.0, 1.0);
        let food = (eco * 0.65 + river * 0.25 + crop * 0.10).clamp(0.0, 1.0);
        let land_use = (food * 0.8).clamp(0.0, 1.0);
        world.state.subsistence.food_production[i] = lerp(
            world.state.subsistence.food_production[i],
            food,
            alpha * budget.max(1) as f32,
        );
        world.state.subsistence.land_use[i] = lerp(
            world.state.subsistence.land_use[i],
            land_use,
            alpha * budget.max(1) as f32,
        );
        world.state.subsistence.subsistence_mix[i] = lerp(
            world.state.subsistence.subsistence_mix[i],
            food,
            alpha * 0.5,
        );
        water_withdrawal[i] = (food * world.state.population.population[i] / 180.0).clamp(0.0, 1.0);
        dam_pressure[i] = (river * world.state.population.population[i] / 220.0).clamp(0.0, 1.0);
    }

    for i in 0..n {
        world.feedback.push(FeedbackEntry {
            source: ModuleId::Subsistence,
            target_module: ModuleId::Hydrology,
            target_ref: TargetRef::Cell(i as u32),
            enqueued_tick: world.clock.tick,
            payload: FeedbackPayload::SetValue {
                field: CellFieldId::WaterWithdrawal,
                cell: i as u32,
                value: FieldValue::F32(water_withdrawal[i]),
            },
        });
        world.feedback.push(FeedbackEntry {
            source: ModuleId::Subsistence,
            target_module: ModuleId::Hydrology,
            target_ref: TargetRef::Cell(i as u32),
            enqueued_tick: world.clock.tick,
            payload: FeedbackPayload::SetValue {
                field: CellFieldId::DamPressure,
                cell: i as u32,
                value: FieldValue::F32(dam_pressure[i]),
            },
        });
    }
}
