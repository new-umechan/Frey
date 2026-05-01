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
        let is_land = world.state.geology.height[i] > world.control.sea_level_offset;
        let next_size = if is_land { pop } else { 0.0 };
        let urban = (next_size / 60.0).clamp(0.0, 1.0);
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

                let mut crop_delta = [0.0; N_CROPS];
                for (idx, value) in crop_delta.iter_mut().enumerate() {
                    *value = (world.state.domesticates.crop_adoption[src][idx] * network_strength)
                        .clamp(0.0, 0.06);
                }
                let mut livestock_delta = [0.0; N_LIVESTOCK];
                for (idx, value) in livestock_delta.iter_mut().enumerate() {
                    *value = (world.state.domesticates.livestock_adoption[src][idx]
                        * network_strength)
                        .clamp(0.0, 0.06);
                }
                let has_crop_pressure = crop_delta.iter().any(|&value| value > 0.0);
                let has_livestock_pressure = livestock_delta.iter().any(|&value| value > 0.0);
                if !has_crop_pressure && !has_livestock_pressure {
                    continue;
                }

                queue.push(FeedbackEntry {
                    source: ModuleId::Settlement,
                    target_module: ModuleId::Domesticates,
                    target_ref: TargetRef::Cell(CellId(dst as u32)),
                    enqueued_tick: world.clock.tick,
                    payload: FeedbackPayload::DomesticatesSpread {
                        cell: CellId(dst as u32),
                        crop_delta,
                        livestock_delta,
                    },
                });
            }
        }
    }
    world.entities.replace_settlements(settlements);
}
