import type {
    EngineClient,
    ExecWorldSliceAndDeltaResult,
    ExecWorldSliceResult,
    FieldResult,
    ExplainCellResult,
    HistoryTicksResult,
    InitWorldResult,
    MeshGenerationResult,
    MetricsResult,
    ProfiledExecResult,
    TimelineAdvanceResult,
    TimelineStateResult,
    ViewDeltaResult,
    WorldDeltaResult,
    ExecModuleDocRecord,
    ExecModuleGraphRecord,
} from "./engine-client";

interface ApiErrorBody {
    error?: unknown;
    message?: unknown;
    precompute_status?: unknown;
    request_id?: unknown;
}

export class PrecomputePendingError extends Error {
    readonly requestId: string | null;

    constructor(message: string, requestId: string | null) {
        super(message);
        this.name = "PrecomputePendingError";
        this.requestId = requestId;
    }
}

export class HttpPrecomputedEngineClient implements EngineClient {
    private readonly baseUrl: string;

    constructor(baseUrl: string) {
        this.baseUrl = baseUrl.replace(/\/+$/, "");
    }

    private url(path: string) {
        return `${this.baseUrl}${path.startsWith("/") ? path : `/${path}`}`;
    }

    private async request<T>(path: string, init?: RequestInit): Promise<T> {
        const response = await fetch(this.url(path), {
            ...init,
            headers: {
                "content-type": "application/json",
                ...(init?.headers ?? {}),
            },
        });
        const body = await response.json().catch(() => null) as ApiErrorBody | null;
        if (response.status === 202) {
            const requestId = typeof body?.request_id === "string" ? body.request_id : null;
            const message = typeof body?.message === "string"
                ? body.message
                : "Precompute request was queued";
            throw new PrecomputePendingError(message, requestId);
        }
        if (!response.ok) {
            const message = typeof body?.error === "string"
                ? body.error
                : `HTTP ${response.status}`;
            throw new Error(message);
        }
        return body as T;
    }

    private async post<T>(path: string, body: unknown): Promise<T> {
        return await this.request<T>(path, {
            method: "POST",
            body: JSON.stringify(body ?? {}),
        });
    }

    async generate_mesh(level: number): Promise<MeshGenerationResult> {
        return await this.request<MeshGenerationResult>(`/api/mesh/${level}`);
    }

    async init_world(
        seed: string,
        meshLevel: number,
        config: unknown,
    ): Promise<InitWorldResult> {
        return await this.post<InitWorldResult>("/api/worlds", {
            seed,
            mesh_level: meshLevel,
            config,
        });
    }

    async exec_world(worldId: string, tickCount: number): Promise<void> {
        await this.advance_timeline(worldId, tickCount);
    }

    async advance_timeline(
        worldId: string,
        tickCount: number,
    ): Promise<TimelineAdvanceResult> {
        return await this.post<TimelineAdvanceResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/advance`,
            { tick_count: tickCount },
        );
    }

    async advance_timeline_slice(
        worldId: string,
        workBudget: number,
    ): Promise<ExecWorldSliceResult> {
        const result = await this.advance_timeline_slice_and_delta(worldId, workBudget);
        return result.slice;
    }

    async advance_timeline_slice_and_delta(
        worldId: string,
        workBudget: number,
        options?: unknown,
    ): Promise<ExecWorldSliceAndDeltaResult> {
        return await this.post<ExecWorldSliceAndDeltaResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/advance-slice-and-delta`,
            { work_budget: workBudget, options },
        );
    }

    async exec_world_slice(
        worldId: string,
        workBudget: number,
    ): Promise<ExecWorldSliceResult> {
        return await this.advance_timeline_slice(worldId, workBudget);
    }

    async exec_world_slice_and_delta(
        worldId: string,
        workBudget: number,
        options?: unknown,
    ): Promise<ExecWorldSliceAndDeltaResult> {
        return await this.advance_timeline_slice_and_delta(worldId, workBudget, options);
    }

    async exec_world_profiled(
        worldId: string,
        tickCount: number,
    ): Promise<ProfiledExecResult> {
        return await this.post<ProfiledExecResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/profiled`,
            { tick_count: tickCount },
        );
    }

    async get_view_delta(
        worldId: string,
        options?: unknown,
    ): Promise<ViewDeltaResult> {
        return await this.post<ViewDeltaResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/view-delta`,
            { options },
        );
    }

    async get_world_delta(
        worldId: string,
        options?: unknown,
    ): Promise<WorldDeltaResult> {
        return await this.get_view_delta(worldId, options);
    }

    async get_timeline_state(worldId: string): Promise<TimelineStateResult> {
        return await this.request<TimelineStateResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/timeline`,
        );
    }

    async get_metrics(worldId: string): Promise<MetricsResult | null> {
        return await this.request<MetricsResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/metrics`,
        );
    }

    async get_field(
        worldId: string,
        fieldKind: string,
        window: number,
    ): Promise<FieldResult> {
        return await this.request<FieldResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/field/${encodeURIComponent(fieldKind)}?lod=${encodeURIComponent(String(window))}`,
        );
    }

    // 事前計算サーバーはライブの World を保持せず、フィールド配列だけを持つ。
    // explain_cell はライブ状態を要するため、この経路は未対応。
    // サーバー側 explain 実装が入るまでは呼び出し側で握りつぶす想定。
    async explain_cell(
        _worldId: string,
        _cellIndex: number,
        _target: string,
    ): Promise<ExplainCellResult> {
        throw new Error(
            "explain_cell is not yet supported on the precomputed (HTTP) engine",
        );
    }

    async list_checkpoint_ticks(worldId: string): Promise<HistoryTicksResult> {
        return await this.request<HistoryTicksResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/checkpoints`,
        );
    }

    async list_history_ticks(worldId: string): Promise<HistoryTicksResult> {
        return await this.list_checkpoint_ticks(worldId);
    }

    async seek_world_to_tick(worldId: string, tick: number): Promise<void> {
        await this.post(`/api/worlds/${encodeURIComponent(worldId)}/seek`, { tick });
    }

    async restore_world_to_tick(worldId: string, tick: number): Promise<void> {
        await this.seek_world_to_tick(worldId, tick);
    }

    async rewind_world_by_ticks(worldId: string, tickCount: number): Promise<void> {
        await this.post(`/api/worlds/${encodeURIComponent(worldId)}/rewind`, {
            tick_count: tickCount,
        });
    }

    async set_simulation_rate(worldId: string, rate: number): Promise<void> {
        await this.post(`/api/worlds/${encodeURIComponent(worldId)}/simulation-rate`, {
            rate,
        });
    }

    async get_exec_modules(): Promise<ExecModuleDocRecord[]> {
        const modules = await this.request<unknown>("/api/exec-modules");
        return Array.isArray(modules) ? modules as ExecModuleDocRecord[] : [];
    }

    async get_exec_module_graph(): Promise<ExecModuleGraphRecord> {
        const graph = await this.request<unknown>("/api/exec-module-graph");
        return (graph ?? { modules: [], edges: [] }) as ExecModuleGraphRecord;
    }
}
