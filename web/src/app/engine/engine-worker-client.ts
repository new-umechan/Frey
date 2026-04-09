import type { ExecModuleDocRecord, ExecModuleGraphRecord } from "../../interface/wasm";
import type { EngineClient } from "./engine-client";
import type { EngineWorkerRequest, EngineWorkerResponse } from "./worker-protocol";

type PendingRequest = {
    resolve: (value: unknown) => void;
    reject: (error: Error) => void;
};

export class EngineWorkerClient implements EngineClient {
    private readonly worker: Worker;
    private readonly pending = new Map<number, PendingRequest>();
    private nextId = 1;

    constructor(worker: Worker) {
        this.worker = worker;
        this.worker.addEventListener("message", this.handleMessage);
        this.worker.addEventListener("error", this.handleError);
    }

    private handleMessage = (event: MessageEvent<EngineWorkerResponse>) => {
        const response = event.data;
        const request = this.pending.get(response.id);
        if (!request) {
            return;
        }
        this.pending.delete(response.id);
        if (response.ok) {
            request.resolve(response.payload);
            return;
        }
        request.reject(new Error(response.error));
    };

    private handleError = (event: ErrorEvent) => {
        const message = event.message || "Engine worker failed";
        for (const request of this.pending.values()) {
            request.reject(new Error(message));
        }
        this.pending.clear();
    };

    private request<K extends EngineWorkerRequest["kind"]>(
        kind: K,
        payload: Extract<EngineWorkerRequest, { kind: K }>["payload"],
    ): Promise<unknown> {
        const id = this.nextId++;
        const message = { id, kind, payload } as EngineWorkerRequest;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.worker.postMessage(message);
        });
    }

    init_world(seed: string, meshLevel: number, config: unknown): Promise<any> {
        return this.request("init_world", { seed, meshLevel, config });
    }

    async exec_world(worldId: string, tickCount: number): Promise<void> {
        await this.request("exec_world", { worldId, tickCount });
    }

    exec_world_slice(worldId: string, workBudget: number): Promise<any> {
        return this.request("exec_world_slice", { worldId, workBudget });
    }

    exec_world_profiled(worldId: string, tickCount: number): Promise<any> {
        return this.request("exec_world_profiled", { worldId, tickCount });
    }

    get_world_delta(worldId: string, options?: unknown): Promise<any> {
        return this.request("get_world_delta", { worldId, options });
    }

    get_metrics(worldId: string): Promise<any> {
        return this.request("get_metrics", { worldId });
    }

    get_field(worldId: string, fieldKind: string, window: number): Promise<any> {
        return this.request("get_field", { worldId, fieldKind, window });
    }

    list_history_ticks(worldId: string): Promise<any> {
        return this.request("list_history_ticks", { worldId });
    }

    async restore_world_to_tick(worldId: string, tick: number): Promise<void> {
        await this.request("restore_world_to_tick", { worldId, tick });
    }

    async get_exec_modules(): Promise<ExecModuleDocRecord[]> {
        const modules = await this.request("get_exec_modules", {});
        return Array.isArray(modules) ? (modules as ExecModuleDocRecord[]) : [];
    }

    async get_exec_module_graph(): Promise<ExecModuleGraphRecord> {
        const graph = await this.request("get_exec_module_graph", {});
        const fallback: ExecModuleGraphRecord = { modules: [], edges: [] };
        return (graph ?? fallback) as ExecModuleGraphRecord;
    }
}

export function createEngineWorkerClient(): EngineWorkerClient {
    const worker = new Worker(new URL("./engine-worker.ts", import.meta.url), {
        type: "module",
    });
    return new EngineWorkerClient(worker);
}
