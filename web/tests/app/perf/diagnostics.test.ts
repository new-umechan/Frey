import { describe, expect, it } from "vitest";

import {
    buildDiagnosticsSummary,
    createDiagnostics,
    recordProfiledStepSuccess,
} from "../../../src/app/perf/diagnostics";

describe("perf/diagnostics", () => {
    it("groups geology, climate, and hydrology diagnostics with normalized keys", () => {
        const diagnostics = createDiagnostics();
        diagnostics.profile_attempt_count = 3;
        diagnostics.profile_fallback_count = 1;
        diagnostics.replay_ticks_total = 2;
        diagnostics.replay_time_ms_total = 12;
        diagnostics.exec_world_time_ms_total = 120;
        diagnostics.exec_world_profiled_time_ms_total = 60;
        diagnostics.tick_total_time_ms_total = 180;
        diagnostics.geometry_update_skipped_count = 4;

        recordProfiledStepSuccess(diagnostics, {
            exec_geology_terrain_ms: 10.1114,
            exec_climate_ms: 11.5555,
            exec_hydrology_ms: 12.8888,
            river_network_rebuild_count: 2.8,
            river_fallback_count: 1.9,
            sink_rebuild_full_count: 1.2,
            sink_rebuild_partial_count: 2.4,
            sink_rebuild_skipped_count: 3.9,
            sink_rebuild_fallback_full_count: 0.9,
            sink_validation_fail_count: 1.2,
            sink_affected_ratio: 0.25,
        });
        recordProfiledStepSuccess(diagnostics, {
            exec_geology_terrain_ms: 5,
            exec_climate_ms: 6,
            exec_hydrology_ms: 7,
            river_network_rebuild_count: 1,
            sink_rebuild_partial_count: 1,
            sink_rebuild_skipped_count: 1,
            sink_rebuild_fallback_full_count: 1,
            sink_affected_ratio: 0.75,
        });

        const summary = buildDiagnosticsSummary(diagnostics, 10, 240);

        expect(summary.step_geology_terrain_time_ms_total).toBe(15.111);
        expect(summary.step_climate_time_ms_total).toBe(17.556);
        expect(summary.step_hydrology_time_ms_total).toBe(19.889);
        expect(summary.step_geology_river_time_ms_total).toBe(19.889);
        expect(summary.river_rebuild_rate).toBe(0.3);
        expect(summary.sink_affected_ratio_mean).toBe(0.5);

        expect(summary.modules).toEqual({
            geology: {
                exec_time_ms_total: 15.111,
                exec_time_share_of_exec_world: 0.251857,
            },
            climate: {
                exec_time_ms_total: 17.556,
                exec_time_share_of_exec_world: 0.292592,
            },
            hydrology: {
                exec_time_ms_total: 19.889,
                exec_time_share_of_exec_world: 0.33148,
                river_network_rebuild_count_total: 3,
                river_rebuild_rate: 0.3,
                river_fallback_count_total: 1,
                sink_rebuild_full_count_total: 1,
                sink_rebuild_partial_count_total: 3,
                sink_rebuild_skipped_count_total: 4,
                sink_rebuild_fallback_full_count_total: 1,
                sink_validation_fail_count_total: 1,
                sink_affected_ratio_mean: 0.5,
            },
        });

        expect(summary.normalized).toEqual({
            module_geology_exec_time_ms_total: 15.111,
            module_geology_exec_time_share_of_exec_world: 0.251857,
            module_climate_exec_time_ms_total: 17.556,
            module_climate_exec_time_share_of_exec_world: 0.292592,
            module_hydrology_exec_time_ms_total: 19.889,
            module_hydrology_exec_time_share_of_exec_world: 0.33148,
            module_hydrology_river_network_rebuild_count_total: 3,
            module_hydrology_river_rebuild_rate: 0.3,
            module_hydrology_river_fallback_count_total: 1,
            module_hydrology_sink_rebuild_full_count_total: 1,
            module_hydrology_sink_rebuild_partial_count_total: 3,
            module_hydrology_sink_rebuild_skipped_count_total: 4,
            module_hydrology_sink_rebuild_fallback_full_count_total: 1,
            module_hydrology_sink_validation_fail_count_total: 1,
            module_hydrology_sink_affected_ratio_mean: 0.5,
        });
    });
});
