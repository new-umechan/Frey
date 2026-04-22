import { describe, expect, it, vi } from "vitest";

import { createPerfRunner } from "../../../src/app/perf/runner";

class MockWorldSimController {
    private tick = 0;
    private readonly marker = "bound-instance";

    init_world(): { world_id: string } {
        return { world_id: "world-000001" };
    }

    get_field(_worldId: string, fieldKind: string): { f32_data?: number[]; i32_data?: number[]; u32_data?: number[] } {
        if (fieldKind === "plate_id") {
            return { u32_data: [0, 0, 0] };
        }
        if (fieldKind === "river_next") {
            return { i32_data: [-1, -1, -1] };
        }
        return { f32_data: [0, 0, 0] };
    }

    get_metrics(): { tick: number } {
        return { tick: this.tick };
    }

    exec_world(_worldId: string, tickCount: number): void {
        this.tick += tickCount;
    }

    get_world_delta(): { deltas: [] } {
        return { deltas: [] };
    }

    exec_world_profiled_detail(_worldId: string, tickCount: number): Record<string, unknown> {
        if (this.marker !== "bound-instance") {
            throw new Error("exec_world_profiled_detail lost this binding");
        }
        this.tick += tickCount;
        return {
            steps: tickCount,
            exec_feedback_ms: 0,
            exec_geology_terrain_ms: 0,
            exec_climate_ms: 0,
            exec_hydrology_ms: 0,
            exec_ecology_ms: 0,
            exec_society_ms: 0,
            exec_transition_ms: 0,
            step_sync_erosion_ms: 0,
            step_observe_world_change_ms: 0,
            step_history_snapshot_ms: 0,
            river_network_rebuild_count: 0,
            river_fallback_count: 0,
            sink_rebuild_full_count: 0,
            sink_rebuild_partial_count: 0,
            sink_rebuild_skipped_count: 0,
            sink_rebuild_fallback_full_count: 0,
        };
    }
}

describe("perf/runner", () => {
    it("calls exec_world_profiled_detail with controller binding", async () => {
        const runner = createPerfRunner({
            WorldSimController: MockWorldSimController as unknown as new () => any,
            build_render_positions: vi.fn(),
            generate_mesh: vi.fn().mockReturnValue({ positions: [0, 0, 0] }),
        });

        const result = await runner.runBenchmark({
            profile: { tickCount: 1, seed: "alpha", surfaceMode: "globe", viewMode: "normal" },
            sampleInterval: 1,
            skipGeometry: true,
            terrainParams: {},
        });

        expect(result.diagnostics.profile_attempt_count).toBe(1);
        expect(result.diagnostics.profile_success_count).toBe(1);
        expect(result.diagnostics.profile_fallback_count).toBe(0);
        expect(result.diagnostics.modules.geology.exec_time_ms_total).toBe(0);
        expect(result.diagnostics.modules.climate.exec_time_ms_total).toBe(0);
        expect(result.diagnostics.modules.hydrology.exec_time_ms_total).toBe(0);
        expect(result.diagnostics.normalized.module_hydrology_exec_time_ms_total).toBe(0);
    });
});
