import { roundMs, roundRatio } from "./helpers";

export interface Diagnostics {
    profile_attempt_count: number;
    profile_success_count: number;
    profile_fallback_count: number;
    replay_ticks_total: number;
    replay_time_ms_total: number;
    exec_world_time_ms_total: number;
    exec_world_profiled_time_ms_total: number;
    step_geology_terrain_time_ms_total: number;
    step_climate_time_ms_total: number;
    step_hydrology_time_ms_total: number;
    step_geology_river_time_ms_total: number;
    tick_total_time_ms_total: number;
    river_network_rebuild_count_total: number;
    river_fallback_count_total: number;
    geometry_update_skipped_count: number;
    sink_rebuild_full_count_total: number;
    sink_rebuild_partial_count_total: number;
    sink_rebuild_skipped_count_total: number;
    sink_rebuild_fallback_full_count_total: number;
    sink_validation_fail_count_total: number;
    sink_affected_ratio_total: number;
}

interface ModuleExecDiagnosticsSummary {
    exec_time_ms_total: number;
    exec_time_share_of_exec_world: number;
}

interface HydrologyDiagnosticsSummary extends ModuleExecDiagnosticsSummary {
    river_network_rebuild_count_total: number;
    river_rebuild_rate: number;
    river_fallback_count_total: number;
    sink_rebuild_full_count_total: number;
    sink_rebuild_partial_count_total: number;
    sink_rebuild_skipped_count_total: number;
    sink_rebuild_fallback_full_count_total: number;
    sink_validation_fail_count_total: number;
    sink_affected_ratio_mean: number;
}

interface DiagnosticsModulesSummary {
    geology: ModuleExecDiagnosticsSummary;
    climate: ModuleExecDiagnosticsSummary;
    hydrology: HydrologyDiagnosticsSummary;
}

interface NormalizedDiagnosticsSummary {
    module_scope_stage: "geology_climate_hydrology";
    module_geology_exec_time_ms_total: number;
    module_geology_exec_time_share_of_exec_world: number;
    module_climate_exec_time_ms_total: number;
    module_climate_exec_time_share_of_exec_world: number;
    module_hydrology_exec_time_ms_total: number;
    module_hydrology_exec_time_share_of_exec_world: number;
    module_hydrology_river_network_rebuild_count_total: number;
    module_hydrology_river_rebuild_rate: number;
    module_hydrology_river_fallback_count_total: number;
    module_hydrology_sink_rebuild_full_count_total: number;
    module_hydrology_sink_rebuild_partial_count_total: number;
    module_hydrology_sink_rebuild_skipped_count_total: number;
    module_hydrology_sink_rebuild_fallback_full_count_total: number;
    module_hydrology_sink_validation_fail_count_total: number;
    module_hydrology_sink_affected_ratio_mean: number;
}

export interface DiagnosticsSummary {
    profile_attempt_count: number;
    profile_success_count: number;
    profile_fallback_count: number;
    replay_ticks_total: number;
    replay_time_ms_total: number;
    exec_world_time_ms_total: number;
    exec_world_profiled_time_ms_total: number;
    step_geology_terrain_time_ms_total: number;
    step_climate_time_ms_total: number;
    step_hydrology_time_ms_total: number;
    step_geology_river_time_ms_total: number;
    tick_total_time_ms_total: number;
    replay_time_share_of_wall: number;
    replay_time_share_of_exec_world: number;
    exec_world_share_of_tick: number;
    river_share_of_exec_world: number;
    river_network_rebuild_count_total: number;
    river_rebuild_rate: number;
    river_fallback_count_total: number;
    geometry_update_skipped_count: number;
    sink_rebuild_full_count_total: number;
    sink_rebuild_partial_count_total: number;
    sink_rebuild_skipped_count_total: number;
    sink_rebuild_fallback_full_count_total: number;
    sink_validation_fail_count_total: number;
    sink_affected_ratio_mean: number;
    modules: DiagnosticsModulesSummary;
    normalized: NormalizedDiagnosticsSummary;
}

type ProfiledResult = Record<string, unknown>;

export function createDiagnostics(): Diagnostics {
    return {
        profile_attempt_count: 0,
        profile_success_count: 0,
        profile_fallback_count: 0,
        replay_ticks_total: 0,
        replay_time_ms_total: 0,
        exec_world_time_ms_total: 0,
        exec_world_profiled_time_ms_total: 0,
        step_geology_terrain_time_ms_total: 0,
        step_climate_time_ms_total: 0,
        step_hydrology_time_ms_total: 0,
        step_geology_river_time_ms_total: 0,
        tick_total_time_ms_total: 0,
        river_network_rebuild_count_total: 0,
        river_fallback_count_total: 0,
        geometry_update_skipped_count: 0,
        sink_rebuild_full_count_total: 0,
        sink_rebuild_partial_count_total: 0,
        sink_rebuild_skipped_count_total: 0,
        sink_rebuild_fallback_full_count_total: 0,
        sink_validation_fail_count_total: 0,
        sink_affected_ratio_total: 0,
    };
}

function accumulateProfiledDiagnostics(diagnostics: Diagnostics, profiled: ProfiledResult) {
    diagnostics.step_geology_terrain_time_ms_total += Number(profiled?.exec_geology_terrain_ms) || 0;
    diagnostics.step_climate_time_ms_total += Number(profiled?.exec_climate_ms) || 0;
    const hydrologyMs = Number(profiled?.exec_hydrology_ms) || 0;
    diagnostics.step_hydrology_time_ms_total += hydrologyMs;
    diagnostics.step_geology_river_time_ms_total += hydrologyMs;
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
    diagnostics.sink_validation_fail_count_total += Math.max(
        0,
        Math.floor(Number(profiled?.sink_validation_fail_count) || 0),
    );
    diagnostics.sink_affected_ratio_total += Math.max(
        0,
        Number(profiled?.sink_affected_ratio) || 0,
    );
}

export function recordProfiledStepSuccess(diagnostics: Diagnostics, profiled: ProfiledResult) {
    accumulateProfiledDiagnostics(diagnostics, profiled);
    diagnostics.profile_success_count += 1;
}

export function buildDiagnosticsSummary(
    diagnostics: Diagnostics,
    totalTicks: number,
    wallTimeMs: number,
): DiagnosticsSummary {
    const replayShareOfWall = wallTimeMs > 0
        ? diagnostics.replay_time_ms_total / wallTimeMs
        : 0;
    const replayShareOfStepWorld = diagnostics.exec_world_time_ms_total > 0
        ? diagnostics.replay_time_ms_total / diagnostics.exec_world_time_ms_total
        : 0;
    const stepWorldShareOfTick = diagnostics.tick_total_time_ms_total > 0
        ? diagnostics.exec_world_time_ms_total / diagnostics.tick_total_time_ms_total
        : 0;
    const geologyShareOfStepWorld = diagnostics.exec_world_profiled_time_ms_total > 0
        ? diagnostics.step_geology_terrain_time_ms_total / diagnostics.exec_world_profiled_time_ms_total
        : 0;
    const climateShareOfStepWorld = diagnostics.exec_world_profiled_time_ms_total > 0
        ? diagnostics.step_climate_time_ms_total / diagnostics.exec_world_profiled_time_ms_total
        : 0;
    const riverShareOfStepWorld = diagnostics.exec_world_profiled_time_ms_total > 0
        ? diagnostics.step_geology_river_time_ms_total / diagnostics.exec_world_profiled_time_ms_total
        : 0;
    const riverRebuildRate = totalTicks > 0
        ? diagnostics.river_network_rebuild_count_total / totalTicks
        : 0;
    const sinkAffectedRatioMean = diagnostics.profile_success_count > 0
        ? diagnostics.sink_affected_ratio_total / diagnostics.profile_success_count
        : 0;
    const geologyExecTimeMsTotal = roundMs(diagnostics.step_geology_terrain_time_ms_total);
    const climateExecTimeMsTotal = roundMs(diagnostics.step_climate_time_ms_total);
    const hydrologyExecTimeMsTotal = roundMs(diagnostics.step_hydrology_time_ms_total);
    const geologyShare = roundRatio(geologyShareOfStepWorld);
    const climateShare = roundRatio(climateShareOfStepWorld);
    const hydrologyShare = roundRatio(riverShareOfStepWorld);
    const riverRebuildRateRounded = roundRatio(riverRebuildRate);
    const sinkAffectedRatioMeanRounded = roundRatio(sinkAffectedRatioMean);

    const modules: DiagnosticsModulesSummary = {
        geology: {
            exec_time_ms_total: geologyExecTimeMsTotal,
            exec_time_share_of_exec_world: geologyShare,
        },
        climate: {
            exec_time_ms_total: climateExecTimeMsTotal,
            exec_time_share_of_exec_world: climateShare,
        },
        hydrology: {
            exec_time_ms_total: hydrologyExecTimeMsTotal,
            exec_time_share_of_exec_world: hydrologyShare,
            river_network_rebuild_count_total: diagnostics.river_network_rebuild_count_total,
            river_rebuild_rate: riverRebuildRateRounded,
            river_fallback_count_total: diagnostics.river_fallback_count_total,
            sink_rebuild_full_count_total: diagnostics.sink_rebuild_full_count_total,
            sink_rebuild_partial_count_total: diagnostics.sink_rebuild_partial_count_total,
            sink_rebuild_skipped_count_total: diagnostics.sink_rebuild_skipped_count_total,
            sink_rebuild_fallback_full_count_total: diagnostics.sink_rebuild_fallback_full_count_total,
            sink_validation_fail_count_total: diagnostics.sink_validation_fail_count_total,
            sink_affected_ratio_mean: sinkAffectedRatioMeanRounded,
        },
    };

    const normalized: NormalizedDiagnosticsSummary = {
        module_scope_stage: "geology_climate_hydrology",
        module_geology_exec_time_ms_total: modules.geology.exec_time_ms_total,
        module_geology_exec_time_share_of_exec_world: modules.geology.exec_time_share_of_exec_world,
        module_climate_exec_time_ms_total: modules.climate.exec_time_ms_total,
        module_climate_exec_time_share_of_exec_world: modules.climate.exec_time_share_of_exec_world,
        module_hydrology_exec_time_ms_total: modules.hydrology.exec_time_ms_total,
        module_hydrology_exec_time_share_of_exec_world: modules.hydrology.exec_time_share_of_exec_world,
        module_hydrology_river_network_rebuild_count_total: modules.hydrology.river_network_rebuild_count_total,
        module_hydrology_river_rebuild_rate: modules.hydrology.river_rebuild_rate,
        module_hydrology_river_fallback_count_total: modules.hydrology.river_fallback_count_total,
        module_hydrology_sink_rebuild_full_count_total: modules.hydrology.sink_rebuild_full_count_total,
        module_hydrology_sink_rebuild_partial_count_total: modules.hydrology.sink_rebuild_partial_count_total,
        module_hydrology_sink_rebuild_skipped_count_total: modules.hydrology.sink_rebuild_skipped_count_total,
        module_hydrology_sink_rebuild_fallback_full_count_total: modules.hydrology.sink_rebuild_fallback_full_count_total,
        module_hydrology_sink_validation_fail_count_total: modules.hydrology.sink_validation_fail_count_total,
        module_hydrology_sink_affected_ratio_mean: modules.hydrology.sink_affected_ratio_mean,
    };

    return {
        profile_attempt_count: diagnostics.profile_attempt_count,
        profile_success_count: diagnostics.profile_success_count,
        profile_fallback_count: diagnostics.profile_fallback_count,
        replay_ticks_total: diagnostics.replay_ticks_total,
        replay_time_ms_total: roundMs(diagnostics.replay_time_ms_total),
        exec_world_time_ms_total: roundMs(diagnostics.exec_world_time_ms_total),
        exec_world_profiled_time_ms_total: roundMs(diagnostics.exec_world_profiled_time_ms_total),
        step_geology_terrain_time_ms_total: geologyExecTimeMsTotal,
        step_climate_time_ms_total: climateExecTimeMsTotal,
        step_hydrology_time_ms_total: hydrologyExecTimeMsTotal,
        // Backward-compatible alias kept during staged diagnostics rollout.
        step_geology_river_time_ms_total: hydrologyExecTimeMsTotal,
        tick_total_time_ms_total: roundMs(diagnostics.tick_total_time_ms_total),
        replay_time_share_of_wall: roundRatio(replayShareOfWall),
        replay_time_share_of_exec_world: roundRatio(replayShareOfStepWorld),
        exec_world_share_of_tick: roundRatio(stepWorldShareOfTick),
        river_share_of_exec_world: hydrologyShare,
        river_network_rebuild_count_total: diagnostics.river_network_rebuild_count_total,
        river_rebuild_rate: riverRebuildRateRounded,
        river_fallback_count_total: diagnostics.river_fallback_count_total,
        geometry_update_skipped_count: diagnostics.geometry_update_skipped_count,
        sink_rebuild_full_count_total: diagnostics.sink_rebuild_full_count_total,
        sink_rebuild_partial_count_total: diagnostics.sink_rebuild_partial_count_total,
        sink_rebuild_skipped_count_total: diagnostics.sink_rebuild_skipped_count_total,
        sink_rebuild_fallback_full_count_total: diagnostics.sink_rebuild_fallback_full_count_total,
        sink_validation_fail_count_total: diagnostics.sink_validation_fail_count_total,
        sink_affected_ratio_mean: sinkAffectedRatioMeanRounded,
        modules,
        normalized,
    };
}
