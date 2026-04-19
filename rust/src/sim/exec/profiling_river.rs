use crate::sim::hydrology::HydrologyStepDetailBreakdown;

use super::profiling::ExecWorldRiverBreakdown;

pub(super) fn apply_hydrology_profile(
    river_breakdown: &mut ExecWorldRiverBreakdown,
    river_profile: HydrologyStepDetailBreakdown,
) {
    river_breakdown.step_geology_river_prepare_ms = river_profile.river_prepare_ms;
    river_breakdown.step_geology_river_automaton_ms = river_profile.river_automaton_ms;
    river_breakdown.step_geology_river_automaton_sink_ms = river_profile.river_automaton_sink_ms;
    river_breakdown.step_geology_river_automaton_cell_ms = river_profile.river_automaton_cell_ms;
    river_breakdown.step_geology_river_automaton_queue_ms = river_profile.river_automaton_queue_ms;
    river_breakdown.step_geology_river_network_ms = river_profile.river_network_ms;
    river_breakdown.step_geology_river_sync_ms = river_profile.river_sync_ms;
    river_breakdown.step_geology_river_fallback_ms = river_profile.river_fallback_ms;
    river_breakdown.river_network_rebuild_count = river_profile.network_rebuild_count;
    river_breakdown.river_fallback_count = river_profile.fallback_count;
    river_breakdown.sink_rebuild_full_count = river_profile.sink_rebuild_full_count;
    river_breakdown.sink_rebuild_partial_count = river_profile.sink_rebuild_partial_count;
    river_breakdown.sink_rebuild_skipped_count = river_profile.sink_rebuild_skipped_count;
    river_breakdown.sink_rebuild_fallback_full_count =
        river_profile.sink_rebuild_fallback_full_count;
    river_breakdown.step_geology_river_sink_incremental_rebuild_ms =
        river_profile.sink_incremental_rebuild_ms;
    river_breakdown.step_geology_river_sink_full_rebuild_ms = river_profile.sink_full_rebuild_ms;
    river_breakdown.sink_affected_ratio = river_profile.sink_affected_ratio;
    river_breakdown.sink_validation_fail_count = river_profile.sink_validation_fail_count;
}
