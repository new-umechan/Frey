import { afterEach, describe, expect, it, vi } from "vitest";
import { HttpPrecomputedEngineClient } from "./http-precomputed-engine-client";

describe("HttpPrecomputedEngineClient timeline prefetch", () => {
    afterEach(() => {
        vi.unstubAllGlobals();
    });

    it("Worker 非対応時は巨大な旧JSON streamを開始しない", async () => {
        const webSocket = vi.fn();
        vi.stubGlobal("WebSocket", webSocket);
        vi.stubGlobal("fetch", vi.fn(async () => jsonResponse({
            world_id: "world-1",
            tick: 10,
            head_tick: 100,
        })));

        const client = new HttpPrecomputedEngineClient("");
        await client.init_world("alpha", 6, {});
        client.prefetch_timeline("world-1", 70);

        expect(webSocket).not.toHaveBeenCalled();
    });
});

function jsonResponse(body: unknown): Response {
    return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
    });
}
