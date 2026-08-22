import { describe, expect, it } from "vitest";
import type { MetricsResult, TimelineStateResult, ViewDeltaResult } from "./engine-client";
import { TimelinePrefetchCache } from "./timeline-prefetch-cache";

function metrics(tick: number): MetricsResult {
    return {
        world_id: "world-1",
        tick,
        era: "geologic",
        simulation_rate: 1,
        real_years_per_tick: 1,
        runtime_tick_ms: 1,
        budgets: { geology: 1, climate: 1, ecology: 1, civilization: 1 },
        cell_count: 4,
        land_cells: 2,
        land_ratio: 0.5,
        sea_level_offset: 0,
        mean_height: 0,
        height_std_dev: 0,
        mean_river_flux: 0,
        max_height: 0,
        min_height: 0,
        max_river_flux: 0,
        top10_river_flux_sum: 0,
        river_active_cells: 0,
        river_fragmentation_ratio: 0,
        river_ocean_reach_ratio: 0,
        river_mainstem_persistence: 0,
        river_flux_concentration: 0,
        continent_count: 1,
        largest_continent_cells: 2,
    };
}

function timeline(tick: number): TimelineStateResult {
    return {
        world_id: "world-1",
        current_tick: tick,
        head_tick: 100,
        checkpoint_interval: 1,
        checkpoint_count: 101,
        undo_log_count: 0,
    };
}

function fullFrame(tick: number, values: number[]): ViewDeltaResult {
    return {
        world_id: "world-1",
        tick,
        head_tick: 100,
        era: "geologic",
        real_years_per_tick: 1,
        runtime_tick_ms: 1,
        budgets: { geology: 1, climate: 1, ecology: 1, civilization: 1 },
        deltas: [{
            field_kind: "height",
            mode: "full",
            ranges: [],
            f32_data: values,
        }],
    };
}

describe("TimelinePrefetchCache", () => {
    it("full anchor に range と bitmap delta を順に適用する", () => {
        const cache = new TimelinePrefetchCache();
        cache.acceptExactAnchor({
            tick: 10,
            metrics: metrics(10),
            timeline: timeline(10),
            frame: fullFrame(10, [1, 2, 3, 4]),
        });
        cache.acceptExactDelta(11, {
            ...fullFrame(11, []),
            deltas: [{
                field_kind: "height",
                mode: "delta",
                ranges: [{ start: 1, end: 3 }],
                f32_data: [20, 30],
            }],
        });
        cache.acceptExactDelta(12, {
            ...fullFrame(12, []),
            deltas: [{
                field_kind: "height",
                mode: "bitmap",
                ranges: [],
                dirty_bitmap: [0b1001],
                f32_data: [10, 40],
            }],
        });

        expect(cache.getExact(10)?.fields.get("height")?.f32_data).toEqual(
            new Float32Array([1, 2, 3, 4]),
        );
        expect(cache.getExact(11)?.fields.get("height")?.f32_data).toEqual(
            new Float32Array([1, 20, 30, 4]),
        );
        expect(cache.getExact(12)?.fields.get("height")?.f32_data).toEqual(
            new Float32Array([10, 20, 30, 40]),
        );
    });

    it("exact cache を上限内に保つ", () => {
        const cache = new TimelinePrefetchCache(2, 2);
        for (let tick = 0; tick < 3; tick += 1) {
            cache.acceptExactAnchor({
                tick,
                metrics: metrics(tick),
                timeline: timeline(tick),
                frame: fullFrame(tick, [tick]),
            });
        }

        expect(cache.exactTicks()).toEqual([1, 2]);
    });

    it("coarse keyframe の field だけを完全 frame に重ねる", () => {
        const cache = new TimelinePrefetchCache();
        const base = cache.acceptExactAnchor({
            tick: 10,
            metrics: metrics(10),
            timeline: timeline(10),
            frame: {
                ...fullFrame(10, [1, 2]),
                deltas: [
                    ...fullFrame(10, [1, 2]).deltas,
                    {
                        field_kind: "temperature",
                        mode: "full",
                        ranges: [],
                        f32_data: [7, 8],
                    },
                ],
            },
        });
        cache.acceptCoarseFrame({
            tick: 64,
            metrics: metrics(64),
            timeline: timeline(64),
            frame: fullFrame(64, [9, 10]),
        });

        const preview = cache.composeCoarsePreview(70, base);
        expect(preview?.tick).toBe(70);
        expect(preview?.preview).toBe(true);
        expect(preview?.fields.get("height")?.f32_data).toEqual(new Float32Array([9, 10]));
        expect(preview?.fields.get("temperature")?.f32_data).toEqual(new Float32Array([7, 8]));
    });
});
