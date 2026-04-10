import initWasm, { WorldSimController, generate_mesh } from "../../interface/wasm";
import type { EngineWorkerRequest, EngineWorkerResponse } from "./worker-protocol";

type WorkerScope = {
    postMessage: (message: EngineWorkerResponse) => void;
    onmessage: ((event: MessageEvent<EngineWorkerRequest>) => void) | null;
};

const workerScope = self as unknown as WorkerScope;

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
            case "generate_mesh": {
                const result = generate_mesh(request.payload.level);
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
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
            case "get_field": {
                const result = runtime.get_field(
                    request.payload.worldId,
                    request.payload.fieldKind,
                    request.payload.window,
                );
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "list_history_ticks": {
                const result = runtime.list_history_ticks(request.payload.worldId);
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "restore_world_to_tick": {
                runtime.restore_world_to_tick(request.payload.worldId, request.payload.tick);
                post({ id: request.id, ok: true, kind: request.kind, payload: null });
                return;
            }
            case "set_simulation_rate": {
                (runtime as WorldSimController & {
                    set_simulation_rate(worldId: string, rate: number): void;
                }).set_simulation_rate(request.payload.worldId, request.payload.rate);
                post({ id: request.id, ok: true, kind: request.kind, payload: null });
                return;
            }
            case "set_target_sea_ratio": {
                (runtime as WorldSimController & {
                    set_target_sea_ratio(worldId: string, targetSeaRatio: number): void;
                }).set_target_sea_ratio(
                    request.payload.worldId,
                    request.payload.targetSeaRatio,
                );
                post({ id: request.id, ok: true, kind: request.kind, payload: null });
                return;
            }
            case "fork_world": {
                const result = (runtime as WorldSimController & {
                    fork_world(worldId: string, tick: number): unknown;
                }).fork_world(request.payload.worldId, request.payload.tick);
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "exec_world_profiled": {
                const result = runtime.exec_world_profiled(
                    request.payload.worldId,
                    request.payload.tickCount,
                );
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "get_exec_modules": {
                const result = (runtime as WorldSimController & {
                    exec_modules(): unknown[];
                }).exec_modules();
                post({ id: request.id, ok: true, kind: request.kind, payload: result });
                return;
            }
            case "get_exec_module_graph": {
                const result = (runtime as WorldSimController & {
                    exec_module_graph(): unknown;
                }).exec_module_graph();
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
