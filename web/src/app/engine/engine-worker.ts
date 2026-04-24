import {
    WorldSimController,
    generate_mesh,
    initializeFreyWasm,
} from "../../transport/wasm/frey-wasm-module";
import type { EngineWorkerRequest, EngineWorkerResponse } from "./worker-protocol";

type WorkerScope = {
    postMessage: (message: EngineWorkerResponse, transfer?: Transferable[]) => void;
    onmessage: ((event: MessageEvent<EngineWorkerRequest>) => void) | null;
};

const workerScope = self as unknown as WorkerScope;

let initialized = false;
let controller: WorldSimController | null = null;

async function ensureController(): Promise<WorldSimController> {
    if (!initialized) {
        await initializeFreyWasm();
        controller = new WorldSimController();
        initialized = true;
    }
    return controller as WorldSimController;
}

function post(response: EngineWorkerResponse, transferables?: Transferable[]) {
    if (transferables && transferables.length > 0) {
        workerScope.postMessage(response, transferables);
    } else {
        workerScope.postMessage(response);
    }
}

function extractTransferables(payload: unknown, seen: Set<ArrayBuffer> = new Set()): Transferable[] {
    const transferables: Transferable[] = [];

    function walk(value: unknown): void {
        if (value === null || value === undefined) {
            return;
        }
        if (value instanceof ArrayBuffer) {
            if (!seen.has(value)) {
                seen.add(value);
                transferables.push(value);
            }
            return;
        }
        if (ArrayBuffer.isView(value)) {
            const buffer = (value as ArrayBufferView).buffer as ArrayBuffer;
            if (!seen.has(buffer)) {
                seen.add(buffer);
                transferables.push(buffer);
            }
            return;
        }
        if (Array.isArray(value)) {
            for (const item of value) {
                walk(item);
            }
            return;
        }
        if (typeof value === "object") {
            for (const item of Object.values(value as Record<string, unknown>)) {
                walk(item);
            }
        }
    }

    walk(payload);
    return transferables;
}

workerScope.onmessage = async (event: MessageEvent<EngineWorkerRequest>) => {
    const request = event.data;
    try {
        const runtime = await ensureController();
        switch (request.kind) {
            case "generate_mesh": {
                const result = generate_mesh(request.payload.level);
                const transferables = extractTransferables(result);
                post({ id: request.id, ok: true, kind: request.kind, payload: result }, transferables);
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
            case "exec_world_slice_and_delta": {
                const slice = runtime.exec_world_slice(
                    request.payload.worldId,
                    request.payload.workBudget,
                );
                let delta: unknown = null;
                if ((slice?.processed_ticks ?? 0) > 0) {
                    delta = runtime.get_world_delta(
                        request.payload.worldId,
                        request.payload.options ?? null,
                    );
                }
                const payload = { slice, delta };
                const transferables = extractTransferables(payload);
                post({ id: request.id, ok: true, kind: request.kind, payload }, transferables);
                return;
            }
            case "get_world_delta": {
                const result = runtime.get_world_delta(
                    request.payload.worldId,
                    request.payload.options ?? null,
                );
                const transferables = extractTransferables(result);
                post({ id: request.id, ok: true, kind: request.kind, payload: result }, transferables);
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
                const transferables = extractTransferables(result);
                post({ id: request.id, ok: true, kind: request.kind, payload: result }, transferables);
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
