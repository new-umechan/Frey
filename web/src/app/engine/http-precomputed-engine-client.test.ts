import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MetricsResult, TimelineStateResult, ViewDeltaResult } from "./engine-client";
import { HttpPrecomputedEngineClient } from "./http-precomputed-engine-client";

class FakeWebSocket {
    static readonly CONNECTING = 0;
    static readonly OPEN = 1;
    static readonly CLOSED = 3;
    static instances: FakeWebSocket[] = [];

    readyState = FakeWebSocket.CONNECTING;
    sent: string[] = [];
    private readonly listeners = new Map<string, Array<(event: { data?: string }) => void>>();

    constructor(readonly url: string | URL) {
        FakeWebSocket.instances.push(this);
    }

    addEventListener(type: string, listener: (event: { data?: string }) => void) {
        const listeners = this.listeners.get(type) ?? [];
        listeners.push(listener);
        this.listeners.set(type, listeners);
    }

    send(payload: string) {
        this.sent.push(payload);
    }

    close() {
        this.readyState = FakeWebSocket.CLOSED;
        this.emit("close", {});
    }

    open() {
        this.readyState = FakeWebSocket.OPEN;
        this.emit("open", {});
    }

    message(payload: unknown) {
        this.emit("message", { data: JSON.stringify(payload) });
    }

    private emit(type: string, event: { data?: string }) {
        for (const listener of this.listeners.get(type) ?? []) {
            listener(event);
        }
    }
}

function metrics(tick: number): MetricsResult {
    return {
        world_id: "world-1",
        tick,
        era: "geologic",
        simulation_rate: 1,
        real_years_per_tick: 1,
        runtime_tick_ms: 1,
        budgets: { geology: 1, climate: 1, ecology: 1, civilization: 1 },
        cell_count: 2,
        land_cells: 1,
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
        largest_continent_cells: 1,
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

function frame(tick: number, heights: number[]): ViewDeltaResult {
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
            f32_data: heights,
        }],
    };
}

describe("HttpPrecomputedEngineClient timeline prefetch", () => {
    beforeEach(() => {
        FakeWebSocket.instances = [];
        vi.stubGlobal("WebSocket", FakeWebSocket);
        vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
            const url = String(input);
            if (url.endsWith("/api/worlds")) {
                return jsonResponse({ world_id: "world-1", tick: 10, head_tick: 100 });
            }
            if (url.endsWith("/seek")) {
                return jsonResponse({ world_id: "world-1", tick: 10, head_tick: 100 });
            }
            return jsonResponse({ field_kind: "height", f32_data: [99, 99] });
        }));
    });

    afterEach(() => {
        vi.unstubAllGlobals();
    });

    it("exact cache を即時利用し、coarse preview 後だけ exact 再同期を要求する", async () => {
        const client = new HttpPrecomputedEngineClient("");
        await client.init_world("alpha", 6, {});
        const socket = FakeWebSocket.instances[0];
        socket.open();
        expect(JSON.parse(socket.sent[0])).toMatchObject({
            type: "subscribe",
            center_tick: 10,
            radius: 2,
        });

        socket.message({
            type: "exact_anchor",
            request_id: 1,
            tick: 10,
            metrics: metrics(10),
            timeline: timeline(10),
            frame: frame(10, [1, 2]),
        });
        socket.message({
            type: "coarse_frame",
            request_id: 1,
            tick: 64,
            metrics: metrics(64),
            timeline: timeline(64),
            frame: frame(64, [7, 8]),
        });

        await client.seek_world_to_tick("world-1", 10);
        expect((await client.get_field("world-1", "height", 1)).f32_data).toEqual(
            new Float32Array([1, 2]),
        );
        expect(await client.finish_prefetched_seek("world-1", 10)).toBe(false);

        await client.seek_world_to_tick("world-1", 70);
        expect((await client.get_field("world-1", "height", 1)).f32_data).toEqual(
            new Float32Array([7, 8]),
        );
        expect(await client.finish_prefetched_seek("world-1", 70)).toBe(true);
    });
});

function jsonResponse(body: unknown): Response {
    return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
    });
}
