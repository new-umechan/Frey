import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_mesh,
} from "../interface/wasm.js";
import { createPerfRunner } from "../app/perf/runner.js";

const runner = createPerfRunner({
    WorldSimController,
    build_render_positions,
    generate_mesh,
});

let wasmReadyPromise = null;

function ensureWasmReady() {
    if (!wasmReadyPromise) {
        wasmReadyPromise = initWasm();
    }
    return wasmReadyPromise;
}

async function runBenchmark(message) {
    await ensureWasmReady();
    return await runner.runBenchmark({
        runId: message.runId,
        profile: message?.profile ?? {},
        level: Number.isFinite(message?.level) ? message.level : 3,
        terrainParams: message?.terrainParams ?? {},
        sampleInterval: Math.max(1, Math.floor(message?.sampleInterval ?? 4)),
        meta: {
            user_agent: message?.meta?.user_agent ?? self.navigator?.userAgent ?? "worker",
            timezone: message?.meta?.timezone ?? "unknown",
        },
        onProgress(payload) {
            self.postMessage({
                type: "progress",
                runId: message.runId,
                done: payload.done,
                total: payload.total,
                percent: payload.percent,
                status: payload.status,
            });
        },
        onWarning(warningMessage) {
            self.postMessage({
                type: "progress",
                runId: message.runId,
                done: 0,
                total: 1,
                percent: 0,
                status: `Worker warning: ${warningMessage}`,
            });
        },
    });
}

self.addEventListener("message", async (event) => {
    const message = event.data ?? {};
    if (message.type !== "run") {
        return;
    }
    try {
        const result = await runBenchmark(message);
        self.postMessage({
            type: "done",
            runId: message.runId,
            result,
        });
    } catch (error) {
        self.postMessage({
            type: "error",
            runId: message.runId,
            message: error instanceof Error ? error.message : String(error),
        });
    }
});
