import type { ExecModuleDocRecord, ExecModuleGraphRecord } from "../../interface/wasm";
import type {
    EngineClient,
    InitWorldResult,
    ForkWorldResult,
    MeshGenerationResult,
    ExecWorldSliceResult,
    ProfiledExecResult,
    WorldDeltaResult,
    MetricsResult,
    FieldResult,
    HistoryTicksResult,
} from "./engine-client";
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

    close() {
        this.worker.removeEventListener("message", this.handleMessage);
        this.worker.removeEventListener("error", this.handleError);
        this.worker.terminate();
        this.pending.clear();
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

    async init_world(seed: string, meshLevel: number, config: unknown): Promise<InitWorldResult> {
        return await this.request("init_world", { seed, meshLevel, config }) as InitWorldResult;
    }

    async generate_mesh(level: number): Promise<MeshGenerationResult> {
        return await this.request("generate_mesh", { level }) as MeshGenerationResult;
    }

    async exec_world(worldId: string, tickCount: number): Promise<void> {
        await this.request("exec_world", { worldId, tickCount });
    }

    async exec_world_slice(worldId: string, workBudget: number): Promise<ExecWorldSliceResult> {
        return await this.request("exec_world_slice", { worldId, workBudget }) as ExecWorldSliceResult;
    }

    async exec_world_profiled(worldId: string, tickCount: number): Promise<ProfiledExecResult> {
        return await this.request("exec_world_profiled", { worldId, tickCount }) as ProfiledExecResult;
    }

    async get_world_delta(worldId: string, options?: unknown): Promise<WorldDeltaResult> {
        return await this.request("get_world_delta", { worldId, options });
    }

    async get_metrics(worldId: string): Promise<MetricsResult | null> {
        const result = await this.request("get_metrics", { worldId });
        return (result ?? null) as MetricsResult | null;
    }

    async get_field(worldId: string, fieldKind: string, window: number): Promise<FieldResult> {
        return await this.request("get_field", { worldId, fieldKind, window });
    }

    async list_history_ticks(worldId: string): Promise<HistoryTicksResult> {
        return await this.request("list_history_ticks", { worldId }) as HistoryTicksResult;
    }

    async restore_world_to_tick(worldId: string, tick: number): Promise<void> {
        await this.request("restore_world_to_tick", { worldId, tick });
    }

    async set_simulation_rate(worldId: string, rate: number): Promise<void> {
        await this.request("set_simulation_rate", { worldId, rate });
    }

    async set_target_sea_ratio(worldId: string, targetSeaRatio: number): Promise<void> {
        await this.request("set_target_sea_ratio", { worldId, targetSeaRatio });
    }

    async fork_world(worldId: string, tick: number): Promise<ForkWorldResult> {
        return await this.request("fork_world", { worldId, tick }) as ForkWorldResult;
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
