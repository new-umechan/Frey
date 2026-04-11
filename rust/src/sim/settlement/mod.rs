pub mod types;

#[allow(unused_imports)]
pub use crate::sim::settlement::types::*;

use crate::sim::exec::lerp;
use crate::sim::world::{
    CellId, FeedbackEntry, FeedbackPayload, FeedbackQueue, ModuleId, SettlementComponent,
    SettlementId, TargetRef, World, N_CROPS, N_LIVESTOCK,
};

pub(crate) fn update_settlement(
    world: &mut World,
    budget: u32,
    mut feedback: Option<&mut FeedbackQueue>,
) {
    if budget == 0 {
        return;
    }
    let alpha = 0.18_f32;
    let n = world.state.geology.height.len();
    let mut settlements = Vec::new();
    for i in 0..n {
        let pop = world.state.population.population[i];
        let next_size = if world.state.geology.height[i] > 0.0 {
            pop
        } else {
            0.0
        };
        let urban = (next_size / 60.0).clamp(0.0, 1.0);
        world.state.settlement.urbanization[i] =
            lerp(world.state.settlement.urbanization[i], urban, alpha);
        if next_size > 0.5 {
            settlements.push(SettlementComponent {
                settlement_id: SettlementId(i as u32 + 1),
                cell: CellId(i as u32),
            });
        }

        if let Some(queue) = feedback.as_deref_mut() {
            if world.state.geology.height[i] <= world.control.sea_level_offset {
                continue;
            }
            let network_strength = (world.state.settlement.urbanization[i] * 0.10).clamp(0.0, 0.10);
            if network_strength <= 0.002 {
                continue;
            }
            let mut crop_delta = [0.0; N_CROPS];
            for (idx, value) in crop_delta.iter_mut().enumerate() {
                *value = (world.state.domesticates.crop_adoption[i][idx] * network_strength)
                    .clamp(0.0, 0.08);
            }
            let mut livestock_delta = [0.0; N_LIVESTOCK];
            for (idx, value) in livestock_delta.iter_mut().enumerate() {
                *value = (world.state.domesticates.livestock_adoption[i][idx] * network_strength)
                    .clamp(0.0, 0.08);
            }
            queue.push(FeedbackEntry {
                source: ModuleId::Settlement,
                target_module: ModuleId::Domesticates,
                target_ref: TargetRef::Cell(CellId(i as u32)),
                enqueued_tick: world.clock.tick,
                payload: FeedbackPayload::DomesticatesSpread {
                    cell: CellId(i as u32),
                    crop_delta,
                    livestock_delta,
                },
            });

            let neighbor_strength = network_strength * 0.65;
            if neighbor_strength > 0.002 {
                for &nbr in world.cell_neighbors(i) {
                    let j = nbr as usize;
                    if j >= n || world.state.geology.height[j] <= world.control.sea_level_offset {
                        continue;
                    }
                    let mut neighbor_crop_delta = [0.0; N_CROPS];
                    for (idx, value) in neighbor_crop_delta.iter_mut().enumerate() {
                        *value = (world.state.domesticates.crop_adoption[i][idx]
                            * neighbor_strength)
                            .clamp(0.0, 0.06);
                    }
                    let mut neighbor_livestock_delta = [0.0; N_LIVESTOCK];
                    for (idx, value) in neighbor_livestock_delta.iter_mut().enumerate() {
                        *value = (world.state.domesticates.livestock_adoption[i][idx]
                            * neighbor_strength)
                            .clamp(0.0, 0.06);
                    }
                    queue.push(FeedbackEntry {
                        source: ModuleId::Settlement,
                        target_module: ModuleId::Domesticates,
                        target_ref: TargetRef::Cell(CellId(j as u32)),
                        enqueued_tick: world.clock.tick,
                        payload: FeedbackPayload::DomesticatesSpread {
                            cell: CellId(j as u32),
                            crop_delta: neighbor_crop_delta,
                            livestock_delta: neighbor_livestock_delta,
                        },
                    });
                }
            }
        }
    }
    world.entities.replace_settlements(settlements);
}
