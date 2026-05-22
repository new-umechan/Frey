pub mod types;

#[allow(unused_imports)]
pub use crate::sim::settlement::types::*;

use crate::sim::exec::lerp;
use crate::sim::world::{
    CellFieldId, CellId, FeedbackEntry, FeedbackPayload, FeedbackQueue, ModuleId,
    SettlementComponent, SettlementId, TargetRef, World,
};

pub(crate) fn update_settlement(
    world: &mut World,
    budget: u32,
    feedback: Option<&mut FeedbackQueue>,
) {
    if budget == 0 {
        return;
    }
    let alpha = 0.18_f32;
    let n = world.state.geology.height.len();
    let mut settlements = Vec::new();
    let mut active_cells = Vec::new();

    for i in 0..n {
        let pop = world.state.population.population[i];
        let food_mean = world.state.subsistence.food_energy_mean[i].clamp(0.0, 1.0);
        let variance = world.state.subsistence.food_energy_variance[i].clamp(0.0, 1.0);
        let buffer = world.state.subsistence.buffer_capacity[i].clamp(0.0, 1.0);
        let mobility = world.state.subsistence.mobility_capacity[i].clamp(0.0, 1.0);
        let water = world.state.hydrology.surface_water_access[i].clamp(0.0, 1.0);
        let effective_stability = (1.0 - variance * (1.0 - buffer)).clamp(0.0, 1.0);
        let is_land = world.state.geology.height[i] > world.control.sea_level_offset;
        let next_size = if is_land { pop } else { 0.0 };
        let sedentary_factor = (food_mean * 0.45 + effective_stability * 0.35 + water * 0.20
            - mobility * 0.15)
            .clamp(0.0, 1.0);
        let urban = ((next_size / 60.0) * (0.55 + sedentary_factor * 0.9)).clamp(0.0, 1.0);
        world.state.settlement.urbanization[i] =
            lerp(world.state.settlement.urbanization[i], urban, alpha);
        if next_size > 0.5 {
            settlements.push(SettlementComponent {
                settlement_id: SettlementId(i as u32 + 1),
                cell: CellId(i as u32),
            });
            active_cells.push(i);
        }
    }

    if let Some(queue) = feedback {
        if active_cells.len() < 2 {
            world.entities.replace_settlements(settlements);
            return;
        }
        for &src in &active_cells {
            let src_urban = world.state.settlement.urbanization[src].clamp(0.0, 1.0);
            let source_strength = (src_urban * 0.10).clamp(0.0, 0.10);
            if source_strength <= 0.002 {
                continue;
            }

            for &dst_u32 in world.cell_neighbors(src) {
                let dst = dst_u32 as usize;
                if dst >= n || dst == src {
                    continue;
                }
                if world.state.geology.height[dst] <= world.control.sea_level_offset {
                    continue;
                }
                if world.state.population.population[dst] <= 0.5 {
                    continue;
                }
                let dst_urban = world.state.settlement.urbanization[dst].clamp(0.0, 1.0);
                let network_strength =
                    (source_strength * (0.45 + 0.55 * dst_urban)).clamp(0.0, 0.08);
                if network_strength <= 0.002 {
                    continue;
                }

                for (idx, adoption) in world.state.domesticates.crop_adoption[src]
                    .iter()
                    .copied()
                    .enumerate()
                {
                    let delta = (adoption * network_strength).clamp(0.0, 0.06);
                    if delta <= 0.0 {
                        continue;
                    }
                    queue.push(FeedbackEntry {
                        source: ModuleId::Settlement,
                        target_module: ModuleId::Domesticates,
                        target_ref: TargetRef::Cell(CellId(dst as u32)),
                        enqueued_tick: world.clock.tick,
                        payload: FeedbackPayload::DeltaF32 {
                            field: CellFieldId::DomesticatesRoutedCropFeedback(idx as u8),
                            cell: CellId(dst as u32),
                            delta,
                        },
                    });
                }
                for (idx, adoption) in world.state.domesticates.livestock_adoption[src]
                    .iter()
                    .copied()
                    .enumerate()
                {
                    let delta = (adoption * network_strength).clamp(0.0, 0.06);
                    if delta <= 0.0 {
                        continue;
                    }
                    queue.push(FeedbackEntry {
                        source: ModuleId::Settlement,
                        target_module: ModuleId::Domesticates,
                        target_ref: TargetRef::Cell(CellId(dst as u32)),
                        enqueued_tick: world.clock.tick,
                        payload: FeedbackPayload::DeltaF32 {
                            field: CellFieldId::DomesticatesRoutedLivestockFeedback(idx as u8),
                            cell: CellId(dst as u32),
                            delta,
                        },
                    });
                }
            }
        }
    }
    world.entities.replace_settlements(settlements);
}
