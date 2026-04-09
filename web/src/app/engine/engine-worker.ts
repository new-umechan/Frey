import initWasm, { WorldSimController } from "../../interface/wasm";
import type { EngineWorkerRequest, EngineWorkerResponse } from "./worker-protocol";

const workerScope = self as unknown as DedicatedWorkerGlobalScope;

let initialized = false;
let controller: WorldSimController | null = null;

async function ensureController(): Promise<WorldSimController> {
    if (!initialized) {
        await initWasm();
        controller = new WorldSimController();
        initialized = true;
    }
    return controller as WorldSimController;
}

function post(response: EngineWorkerResponse) {
    workerScope.postMessage(response);
}

workerScope.onmessage = async (event: MessageEvent<EngineWorkerRequest>) => {
    const request = event.data;
    try {
        const runtime = await ensureController();
        switch (request.kind) {
            case "init_world": {
                const result = runtime.init_world(
                    request.payload.seed,
                    request.payload.meshLevel,
                    request.payload.config ?? null,
                );
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "exec_world": {
                runtime.exec_world(request.payload.worldId, request.payload.tickCount);
                post({ id: request.id, ok: true, kind: request.kind, payload: null });
                return;
            }
            case "exec_world_slice": {
                const result = runtime.exec_world_slice(
                    request.payload.worldId,
                    request.payload.workBudget,
                );
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "get_world_delta": {
                const result = runtime.get_world_delta(
                    request.payload.worldId,
                    request.payload.options ?? null,
                );
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "get_metrics": {
                const result = runtime.get_metrics(request.payload.worldId);
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            default: {
                const neverKind: never = request;
                throw new Error(`unsupported worker request: ${String(neverKind)}`);
            }
        }
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        post({ id: request.id, ok: false, kind: request.kind, error: message });
    }
};
