import { roundMs, roundRatio } from "./helpers.js";

export interface Diagnostics {
    profile_attempt_count: number;
    profile_success_count: number;
    profile_fallback_count: number;
    replay_ticks_total: number;
    replay_time_ms_total: number;
    exec_world_time_ms_total: number;
    exec_world_profiled_time_ms_total: number;
    step_geology_river_time_ms_total: number;
    tick_total_time_ms_total: number;
    river_network_rebuild_count_total: number;
    river_fallback_count_total: number;
    geometry_update_skipped_count: number;
    sink_rebuild_full_count_total: number;
    sink_rebuild_partial_count_total: number;
    sink_rebuild_skipped_count_total: number;
    sink_rebuild_fallback_full_count_total: number;
}

export function createDiagnostics(): Diagnostics {
    return {
        profile_attempt_count: 0,
        profile_success_count: 0,
        profile_fallback_count: 0,
        replay_ticks_total: 0,
        replay_time_ms_total: 0,
        exec_world_time_ms_total: 0,
        exec_world_profiled_time_ms_total: 0,
        step_geology_river_time_ms_total: 0,
        tick_total_time_ms_total: 0,
        river_network_rebuild_count_total: 0,
        river_fallback_count_total: 0,
        geometry_update_skipped_count: 0,
        sink_rebuild_full_count_total: 0,
        sink_rebuild_partial_count_total: 0,
        sink_rebuild_skipped_count_total: 0,
        sink_rebuild_fallback_full_count_total: 0,
    };
}

function accumulateProfiledDiagnostics(diagnostics: Diagnostics, profiled: any) {
    diagnostics.step_geology_river_time_ms_total += Number(profiled?.exec_hydrology_ms) || 0;
    diagnostics.river_network_rebuild_count_total += Math.max(
        0,
        Math.floor(Number(profiled?.river_network_rebuild_count) || 0),
    );
    diagnostics.river_fallback_count_total += Math.max(
        0,
        Math.floor(Number(profiled?.river_fallback_count) || 0),
    );
    diagnostics.sink_rebuild_full_count_total += Math.max(
        0,
        Math.floor(Number(profiled?.sink_rebuild_full_count) || 0),
    );
    diagnostics.sink_rebuild_partial_count_total += Math.max(
        0,
        Math.floor(Number(profiled?.sink_rebuild_partial_count) || 0),
    );
    diagnostics.sink_rebuild_skipped_count_total += Math.max(
        0,
        Math.floor(Number(profiled?.sink_rebuild_skipped_count) || 0),
    );
    diagnostics.sink_rebuild_fallback_full_count_total += Math.max(
        0,
        Math.floor(Number(profiled?.sink_rebuild_fallback_full_count) || 0),
    );
}

export function recordProfiledStepSuccess(diagnostics: Diagnostics, profiled: any) {
    accumulateProfiledDiagnostics(diagnostics, profiled);
    diagnostics.profile_success_count += 1;
}

export function buildDiagnosticsSummary(diagnostics: Diagnostics, totalTicks: number, wallTimeMs: number) {
    const replayShareOfWall = wallTimeMs > 0
        ? diagnostics.replay_time_ms_total / wallTimeMs
        : 0;
    const replayShareOfStepWorld = diagnostics.exec_world_time_ms_total > 0
        ? diagnostics.replay_time_ms_total / diagnostics.exec_world_time_ms_total
        : 0;
    const stepWorldShareOfTick = diagnostics.tick_total_time_ms_total > 0
        ? diagnostics.exec_world_time_ms_total / diagnostics.tick_total_time_ms_total
        : 0;
    const riverShareOfStepWorld = diagnostics.exec_world_profiled_time_ms_total > 0
        ? diagnostics.step_geology_river_time_ms_total / diagnostics.exec_world_profiled_time_ms_total
        : 0;
    const riverRebuildRate = totalTicks > 0
        ? diagnostics.river_network_rebuild_count_total / totalTicks
        : 0;

    return {
        profile_attempt_count: diagnostics.profile_attempt_count,
        profile_success_count: diagnostics.profile_success_count,
        profile_fallback_count: diagnostics.profile_fallback_count,
        replay_ticks_total: diagnostics.replay_ticks_total,
        replay_time_ms_total: roundMs(diagnostics.replay_time_ms_total),
        exec_world_time_ms_total: roundMs(diagnostics.exec_world_time_ms_total),
        exec_world_profiled_time_ms_total: roundMs(diagnostics.exec_world_profiled_time_ms_total),
        step_geology_river_time_ms_total: roundMs(diagnostics.step_geology_river_time_ms_total),
        tick_total_time_ms_total: roundMs(diagnostics.tick_total_time_ms_total),
        replay_time_share_of_wall: roundRatio(replayShareOfWall),
        replay_time_share_of_exec_world: roundRatio(replayShareOfStepWorld),
        exec_world_share_of_tick: roundRatio(stepWorldShareOfTick),
        river_share_of_exec_world: roundRatio(riverShareOfStepWorld),
        river_network_rebuild_count_total: diagnostics.river_network_rebuild_count_total,
        river_rebuild_rate: roundRatio(riverRebuildRate),
        river_fallback_count_total: diagnostics.river_fallback_count_total,
        geometry_update_skipped_count: diagnostics.geometry_update_skipped_count,
        sink_rebuild_full_count_total: diagnostics.sink_rebuild_full_count_total,
        sink_rebuild_partial_count_total: diagnostics.sink_rebuild_partial_count_total,
        sink_rebuild_skipped_count_total: diagnostics.sink_rebuild_skipped_count_total,
        sink_rebuild_fallback_full_count_total: diagnostics.sink_rebuild_fallback_full_count_total,
    };
}
