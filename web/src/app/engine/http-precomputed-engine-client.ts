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
import {
    fieldFromCachedFrame,
    TimelinePrefetchCache,
    type CachedTickFrame,
    viewDeltaFromCachedFrame,
} from "./timeline-prefetch-cache";
import type { DecodedPlaybackChunk } from "./playback-chunk-codec";
import { PlaybackChunkWorkerClient } from "./playback-chunk-worker-client";

interface ApiErrorBody {
    error?: unknown;
    message?: unknown;
    precompute_status?: unknown;
    request_id?: unknown;
}

interface PendingSeek {
    tick: number;
    frame: CachedTickFrame;
    refreshExact: boolean;
    request: Promise<unknown>;
}

interface StreamEnvelope {
    type?: string;
    request_id?: number;
    tick?: number;
    metrics?: MetricsResult;
    timeline?: TimelineStateResult;
    frame?: ViewDeltaResult;
    delta?: ViewDeltaResult;
}

const PREFETCH_RADIUS = 2;
const COARSE_INTERVAL = 256;
const PLAYBACK_BUFFER_TICKS = 8;
const PREVIEW_FIELD_KINDS = [
    "height",
    "lake_depth",
    "plate_id",
    "river_flux",
    "mantle_heat",
];

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
    private readonly prefetchCaches = new Map<string, TimelinePrefetchCache>();
    private readonly streamSockets = new Map<string, WebSocket>();
    private readonly pendingStreamCenters = new Map<string, number>();
    private readonly coarseRequests = new Set<string>();
    private readonly streamReconnectAttempts = new Map<string, number>();
    private readonly activeFrames = new Map<string, CachedTickFrame>();
    private readonly pendingSeeks = new Map<string, PendingSeek>();
    // 通常再生専用。JSON の timeline stream とは独立させ、再生を seek の通信量に巻き込まない。
    private readonly playbackSockets = new Map<string, WebSocket>();
    private readonly playbackBuffers = new Map<string, Map<number, ViewDeltaResult>>();
    private readonly playbackLocalTicks = new Map<string, number>();
    private readonly playbackServerTicks = new Map<string, number>();
    private readonly playbackInFlightTicks = new Map<string, number>();
    private readonly playbackEpochs = new Map<string, number>();
    private readonly playbackFields = new Map<string, string[]>();
    private readonly playbackPreviewFrames = new Map<string, Map<number, ViewDeltaResult>>();
    private readonly pendingPlaybackPreviews = new Map<string, number>();
    private readonly activePlaybackPreviews = new Map<string, ViewDeltaResult>();
    private readonly meshPositions = new Map<number, Float32Array>();
    private readonly worldMeshLevels = new Map<string, number>();
    private readonly previewProjections = new Map<string, Uint32Array>();
    private playbackDecoder: PlaybackChunkWorkerClient | null = null;
    private playbackDisabled = false;
    private nextStreamRequestId = 1;

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
        const mesh = await this.request<MeshGenerationResult>(`/api/mesh/${level}`);
        this.meshPositions.set(level, toFloat32Array(mesh.positions));
        return mesh;
    }

    async init_world(
        seed: string,
        meshLevel: number,
        config: unknown,
    ): Promise<InitWorldResult> {
        const result = await this.post<InitWorldResult>("/api/worlds", {
            seed,
            mesh_level: meshLevel,
            config,
        });
        this.prefetchCaches.set(result.world_id, new TimelinePrefetchCache());
        this.worldMeshLevels.set(result.world_id, meshLevel);
        const tick = result.tick ?? 0;
        this.playbackLocalTicks.set(result.world_id, tick);
        this.playbackServerTicks.set(result.world_id, tick);
        this.playbackBuffers.set(result.world_id, new Map());
        this.playbackPreviewFrames.set(result.world_id, new Map());
        return result;
    }

    async exec_world(worldId: string, tickCount: number): Promise<void> {
        await this.advance_timeline(worldId, tickCount);
    }

    async advance_timeline(
        worldId: string,
        tickCount: number,
    ): Promise<TimelineAdvanceResult> {
        await this.settlePendingSeek(worldId);
        await this.syncPlaybackServerCursor(worldId);
        this.activeFrames.delete(worldId);
        const result = await this.post<TimelineAdvanceResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/advance`,
            { tick_count: tickCount },
        );
        this.notePlaybackServerTick(worldId, result.tick);
        return result;
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
        await this.settlePendingSeek(worldId);
        this.activeFrames.delete(worldId);
        const currentTick = this.playbackLocalTicks.get(worldId);
        const expectedTick = currentTick === undefined ? null : currentTick + 1;
        const buffered = expectedTick === null
            ? undefined
            : this.playbackBuffers.get(worldId)?.get(expectedTick);
        if (buffered && expectedTick !== null) {
            this.playbackBuffers.get(worldId)?.delete(expectedTick);
            this.playbackLocalTicks.set(worldId, expectedTick);
            this.ensurePlaybackBuffer(worldId, includeFieldsFromOptions(options));
            return {
                slice: {
                    busy: false,
                    processed_ticks: 1,
                    phase: "precomputed",
                    head_tick: buffered.head_tick,
                    tick_boundary: "completed_tick",
                },
                delta: buffered,
            };
        }
        await this.syncPlaybackServerCursor(worldId);
        const result = await this.post<ExecWorldSliceAndDeltaResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/advance-slice-and-delta`,
            { work_budget: workBudget, options },
        );
        const nextTick = Number(result.delta?.tick ?? result.slice?.head_tick);
        if (Number.isFinite(nextTick)) {
            this.notePlaybackServerTick(worldId, nextTick);
            this.ensurePlaybackBuffer(worldId, includeFieldsFromOptions(options));
        }
        return result;
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
        const activePreview = this.activePlaybackPreviews.get(worldId);
        if (activePreview) {
            return filterPlaybackPreviewFields(activePreview, includeFieldsFromOptions(options));
        }
        const activeFrame = this.activeFrames.get(worldId);
        if (activeFrame) {
            return viewDeltaFromCachedFrame(activeFrame, includeFieldsFromOptions(options));
        }
        await this.syncPlaybackServerCursor(worldId);
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
        const activeFrame = this.activeFrames.get(worldId);
        if (activeFrame) {
            return activeFrame.timeline;
        }
        await this.syncPlaybackServerCursor(worldId);
        return await this.request<TimelineStateResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/timeline`,
        );
    }

    async get_metrics(worldId: string): Promise<MetricsResult | null> {
        if (this.activePlaybackPreviews.has(worldId)) {
            return null;
        }
        const activeFrame = this.activeFrames.get(worldId);
        if (activeFrame) {
            return activeFrame.metrics;
        }
        await this.syncPlaybackServerCursor(worldId);
        return await this.request<MetricsResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/metrics`,
        );
    }

    async get_field(
        worldId: string,
        fieldKind: string,
        window: number,
    ): Promise<FieldResult> {
        const activeFrame = this.activeFrames.get(worldId);
        const cachedField = activeFrame ? fieldFromCachedFrame(activeFrame, fieldKind) : null;
        if (cachedField && window === 1) {
            return cachedField;
        }
        await this.syncPlaybackServerCursor(worldId);
        return await this.request<FieldResult>(
            `/api/worlds/${encodeURIComponent(worldId)}/field/${encodeURIComponent(fieldKind)}?lod=${encodeURIComponent(String(window))}`,
        );
    }

    // 事前計算サーバーはライブの World を持たず、explain はライブ状態を要するため未対応。
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
        await this.settlePendingSeek(worldId);
        const cache = this.prefetchCaches.get(worldId);
        const playbackPreview = this.playbackPreviewFrames.get(worldId)?.get(tick);
        if (playbackPreview) {
            this.activePlaybackPreviews.set(worldId, playbackPreview);
            const pending: PendingSeek = {
                tick,
                frame: {
                    tick,
                    headTick: playbackPreview.head_tick ?? tick,
                    metrics: null as unknown as MetricsResult,
                    timeline: null as unknown as TimelineStateResult,
                    fields: new Map(),
                    preview: true,
                },
                refreshExact: true,
                request: this.post(`/api/worlds/${encodeURIComponent(worldId)}/seek`, { tick }),
            };
            this.pendingSeeks.set(worldId, pending);
            this.resetPlaybackPosition(worldId, tick);
            return;
        }
        const exact = cache?.getExact(tick) ?? null;
        const base = this.activeFrames.get(worldId) ?? cache?.getNearestExact(tick) ?? null;
        const frame = exact ?? (base ? cache?.composeCoarsePreview(tick, base) ?? null : null);
        if (!frame) {
            this.activeFrames.delete(worldId);
            await this.post(`/api/worlds/${encodeURIComponent(worldId)}/seek`, { tick });
            this.resetPlaybackPosition(worldId, tick);
            return;
        }

        this.activeFrames.set(worldId, frame);
        const pending: PendingSeek = {
            tick,
            frame,
            refreshExact: frame.preview,
            request: this.post(`/api/worlds/${encodeURIComponent(worldId)}/seek`, { tick }),
        };
        this.pendingSeeks.set(worldId, pending);
        this.resetPlaybackPosition(worldId, tick);
    }

    async restore_world_to_tick(worldId: string, tick: number): Promise<void> {
        await this.seek_world_to_tick(worldId, tick);
    }

    async rewind_world_by_ticks(worldId: string, tickCount: number): Promise<void> {
        await this.settlePendingSeek(worldId);
        this.activeFrames.delete(worldId);
        await this.post(`/api/worlds/${encodeURIComponent(worldId)}/rewind`, {
            tick_count: tickCount,
        });
        const previous = this.playbackLocalTicks.get(worldId) ?? 0;
        this.resetPlaybackPosition(worldId, Math.max(0, previous - tickCount));
    }

    async set_simulation_rate(worldId: string, rate: number): Promise<void> {
        await this.settlePendingSeek(worldId);
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

    prefetch_timeline(worldId: string, centerTick: number): void {
        const tick = Math.max(0, Math.floor(centerTick));
        this.pendingPlaybackPreviews.set(worldId, tick);
        this.playbackEpochs.set(worldId, (this.playbackEpochs.get(worldId) ?? 0) + 1);
        this.playbackBuffers.get(worldId)?.clear();
        this.playbackInFlightTicks.delete(worldId);
        this.ensurePlaybackBuffer(worldId, this.playbackFields.get(worldId));
    }

    async finish_prefetched_seek(worldId: string, tick: number): Promise<boolean> {
        const pending = this.pendingSeeks.get(worldId);
        if (!pending || pending.tick !== tick) {
            return false;
        }
        await pending.request;
        if (this.pendingSeeks.get(worldId) !== pending) {
            return false;
        }
        this.pendingSeeks.delete(worldId);
        if (this.activePlaybackPreviews.has(worldId)) {
            this.activePlaybackPreviews.delete(worldId);
            return true;
        }
        if (pending.refreshExact && this.activeFrames.get(worldId) === pending.frame) {
            this.activeFrames.delete(worldId);
            return true;
        }
        return false;
    }

    private async settlePendingSeek(worldId: string): Promise<void> {
        const pending = this.pendingSeeks.get(worldId);
        if (!pending) {
            return;
        }
        await pending.request;
        if (this.pendingSeeks.get(worldId) === pending) {
            this.pendingSeeks.delete(worldId);
            if (this.activePlaybackPreviews.has(worldId)) {
                this.activePlaybackPreviews.delete(worldId);
            }
            if (pending.refreshExact && this.activeFrames.get(worldId) === pending.frame) {
                this.activeFrames.delete(worldId);
            }
        }
    }

    private notePlaybackServerTick(worldId: string, tick: number) {
        const normalized = Math.max(0, Math.floor(tick));
        this.playbackLocalTicks.set(worldId, normalized);
        this.playbackServerTicks.set(worldId, normalized);
        this.playbackBuffers.get(worldId)?.clear();
        this.playbackInFlightTicks.delete(worldId);
        this.playbackEpochs.set(worldId, (this.playbackEpochs.get(worldId) ?? 0) + 1);
    }

    private resetPlaybackPosition(worldId: string, tick: number) {
        const normalized = Math.max(0, Math.floor(tick));
        this.playbackLocalTicks.set(worldId, normalized);
        this.playbackServerTicks.set(worldId, normalized);
        this.playbackBuffers.get(worldId)?.clear();
        this.playbackInFlightTicks.delete(worldId);
        this.playbackEpochs.set(worldId, (this.playbackEpochs.get(worldId) ?? 0) + 1);
    }

    private async syncPlaybackServerCursor(worldId: string) {
        const local = this.playbackLocalTicks.get(worldId);
        const server = this.playbackServerTicks.get(worldId);
        if (local === undefined || server === local) {
            return;
        }
        await this.post(`/api/worlds/${encodeURIComponent(worldId)}/seek`, { tick: local });
        this.playbackServerTicks.set(worldId, local);
    }

    private ensurePlaybackBuffer(worldId: string, fields?: string[]) {
        if (this.playbackDisabled || typeof WebSocket === "undefined" || typeof Worker === "undefined") {
            return;
        }
        const localTick = this.playbackLocalTicks.get(worldId);
        if (localTick === undefined || this.playbackInFlightTicks.has(worldId)) {
            return;
        }
        const normalizedFields = fields ?? [];
        this.playbackFields.set(worldId, normalizedFields);
        const previewTick = this.pendingPlaybackPreviews.get(worldId);
        if (previewTick !== undefined) {
            const socket = this.openPlaybackStream(worldId);
            if (socket?.readyState === WebSocket.OPEN) {
                this.requestPlaybackPreview(worldId, socket, previewTick);
            }
            return;
        }
        const buffer = this.playbackBuffers.get(worldId) ?? new Map<number, ViewDeltaResult>();
        this.playbackBuffers.set(worldId, buffer);
        let target = localTick + 1;
        while (buffer.has(target)) {
            target += 1;
        }
        if (target > localTick + PLAYBACK_BUFFER_TICKS) {
            return;
        }
        const socket = this.openPlaybackStream(worldId);
        if (!socket || socket.readyState !== WebSocket.OPEN) {
            return;
        }
        this.requestPlaybackTick(worldId, socket, target, normalizedFields);
    }

    private openPlaybackStream(worldId: string): WebSocket | null {
        if (typeof WebSocket === "undefined" || typeof Worker === "undefined" || this.playbackDisabled) {
            return null;
        }
        const existing = this.playbackSockets.get(worldId);
        if (existing && (existing.readyState === WebSocket.OPEN || existing.readyState === WebSocket.CONNECTING)) {
            return existing;
        }
        try {
            this.playbackDecoder ??= new PlaybackChunkWorkerClient();
            const httpUrl = new URL(
                this.url(`/api/worlds/${encodeURIComponent(worldId)}/playback`),
                window.location.href,
            );
            httpUrl.protocol = httpUrl.protocol === "https:" ? "wss:" : "ws:";
            const socket = new WebSocket(httpUrl);
            socket.binaryType = "arraybuffer";
            this.playbackSockets.set(worldId, socket);
            socket.addEventListener("open", () => this.ensurePlaybackBuffer(worldId, this.playbackFields.get(worldId)));
            socket.addEventListener("message", (event) => this.acceptPlaybackMessage(worldId, event.data));
            socket.addEventListener("close", () => {
                if (this.playbackSockets.get(worldId) === socket) {
                    this.playbackSockets.delete(worldId);
                }
                this.playbackInFlightTicks.delete(worldId);
            });
            socket.addEventListener("error", () => socket.close());
            return socket;
        } catch {
            this.disablePlaybackStreaming();
            return null;
        }
    }

    private requestPlaybackTick(worldId: string, socket: WebSocket, tick: number, fields: string[]) {
        const epoch = this.playbackEpochs.get(worldId) ?? 0;
        this.playbackInFlightTicks.set(worldId, tick);
        socket.send(JSON.stringify({
            type: "playback",
            epoch,
            start_tick: tick,
            tick_count: 1,
            include_fields: fields,
        }));
    }

    private requestPlaybackPreview(worldId: string, socket: WebSocket, tick: number) {
        const epoch = this.playbackEpochs.get(worldId) ?? 0;
        this.playbackInFlightTicks.set(worldId, tick);
        socket.send(JSON.stringify({
            type: "preview",
            epoch,
            tick,
            include_fields: PREVIEW_FIELD_KINDS,
        }));
    }

    private acceptPlaybackMessage(worldId: string, rawData: unknown) {
        if (rawData instanceof ArrayBuffer) {
            const decoder = this.playbackDecoder;
            if (!decoder) {
                return;
            }
            void decoder.decode(rawData).then((chunk) => {
                const epoch = this.playbackEpochs.get(worldId) ?? 0;
                if (chunk.epoch !== epoch) {
                    return;
                }
                const delta = this.expandSpatialPreview(worldId, chunk);
                if (!delta) {
                    return;
                }
                delta.world_id = worldId;
                const previewTick = this.pendingPlaybackPreviews.get(worldId);
                if (previewTick === chunk.tick) {
                    const previews = this.playbackPreviewFrames.get(worldId) ?? new Map<number, ViewDeltaResult>();
                    previews.set(chunk.tick, delta);
                    this.playbackPreviewFrames.set(worldId, previews);
                    this.pendingPlaybackPreviews.delete(worldId);
                    this.playbackInFlightTicks.delete(worldId);
                    return;
                }
                const buffer = this.playbackBuffers.get(worldId) ?? new Map<number, ViewDeltaResult>();
                this.playbackBuffers.set(worldId, buffer);
                const local = this.playbackLocalTicks.get(worldId) ?? 0;
                if (chunk.tick > local && chunk.tick <= local + PLAYBACK_BUFFER_TICKS) {
                    buffer.set(chunk.tick, delta);
                }
                if (this.playbackInFlightTicks.get(worldId) === chunk.tick) {
                    this.playbackInFlightTicks.delete(worldId);
                }
                this.ensurePlaybackBuffer(worldId, this.playbackFields.get(worldId));
            }).catch(() => this.disablePlaybackStreaming());
            return;
        }
        if (typeof rawData === "string") {
            // エラー応答は当該リクエストを解放し、HTTP フォールバックに任せる。
            this.playbackInFlightTicks.delete(worldId);
        }
    }

    private disablePlaybackStreaming() {
        this.playbackDisabled = true;
        for (const socket of this.playbackSockets.values()) {
            socket.close();
        }
        this.playbackSockets.clear();
        this.playbackInFlightTicks.clear();
        this.playbackBuffers.clear();
        this.playbackDecoder?.close();
        this.playbackDecoder = null;
    }

    private expandSpatialPreview(
        worldId: string,
        chunk: DecodedPlaybackChunk,
    ): ViewDeltaResult | null {
        if (chunk.spatialLod === null) {
            return chunk.delta;
        }
        const meshLevel = this.worldMeshLevels.get(worldId);
        const positions = meshLevel === undefined ? undefined : this.meshPositions.get(meshLevel);
        if (!positions) {
            return null;
        }
        const lowCount = 10 * (4 ** chunk.spatialLod) + 2;
        const cellCount = Math.floor(positions.length / 3);
        if (lowCount > cellCount) {
            return null;
        }
        const projectionKey = `${worldId}:${chunk.spatialLod}`;
        let projection = this.previewProjections.get(projectionKey);
        if (!projection) {
            projection = buildPreviewProjection(positions, lowCount);
            this.previewProjections.set(projectionKey, projection);
        }
        return {
            ...chunk.delta,
            deltas: chunk.delta.deltas.map((field) => ({
                ...field,
                f32_data: expandFloatPreviewValues(field.f32_data, projection, lowCount),
                u32_data: expandUintPreviewValues(field.u32_data, projection, lowCount),
                i32_data: expandIntPreviewValues(field.i32_data, projection, lowCount),
            })),
        };
    }

    private openTickStream(worldId: string) {
        if (typeof WebSocket === "undefined") {
            return;
        }
        const existing = this.streamSockets.get(worldId);
        if (existing && (existing.readyState === WebSocket.OPEN || existing.readyState === WebSocket.CONNECTING)) {
            return;
        }
        const httpUrl = new URL(
            this.url(`/api/worlds/${encodeURIComponent(worldId)}/stream`),
            window.location.href,
        );
        httpUrl.protocol = httpUrl.protocol === "https:" ? "wss:" : "ws:";
        const socket = new WebSocket(httpUrl);
        this.streamSockets.set(worldId, socket);
        socket.addEventListener("open", () => {
            const center = this.pendingStreamCenters.get(worldId) ?? 0;
            this.sendPrefetchSubscription(worldId, socket, center);
        });
        socket.addEventListener("message", (event) => {
            this.acceptStreamMessage(worldId, event.data);
        });
        socket.addEventListener("close", () => {
            if (this.streamSockets.get(worldId) === socket) {
                this.streamSockets.delete(worldId);
            }
            const cache = this.prefetchCaches.get(worldId);
            if (cache?.coarseTicks().length === 0) {
                this.coarseRequests.delete(worldId);
            }
            const attempt = (this.streamReconnectAttempts.get(worldId) ?? 0) + 1;
            this.streamReconnectAttempts.set(worldId, attempt);
            if (attempt <= 3 && this.pendingStreamCenters.has(worldId)) {
                setTimeout(() => this.openTickStream(worldId), attempt * 1000);
            }
        });
        socket.addEventListener("error", () => {
            socket.close();
        });
    }

    private sendPrefetchSubscription(worldId: string, socket: WebSocket, centerTick: number) {
        const cache = this.prefetchCaches.get(worldId);
        if (!cache) {
            return;
        }
        const includeCoarse = cache.coarseTicks().length === 0 && !this.coarseRequests.has(worldId);
        if (includeCoarse) {
            this.coarseRequests.add(worldId);
        }
        socket.send(JSON.stringify({
            type: "subscribe",
            request_id: this.nextStreamRequestId,
            center_tick: centerTick,
            radius: PREFETCH_RADIUS,
            known_exact_ticks: cache.exactTicks(),
            known_coarse_ticks: cache.coarseTicks(),
            coarse_interval: COARSE_INTERVAL,
            include_coarse: includeCoarse,
        }));
        this.nextStreamRequestId += 1;
    }

    private acceptStreamMessage(worldId: string, rawData: unknown) {
        if (typeof rawData !== "string") {
            return;
        }
        let message: StreamEnvelope;
        try {
            message = JSON.parse(rawData) as StreamEnvelope;
        } catch {
            return;
        }
        this.streamReconnectAttempts.set(worldId, 0);
        const cache = this.prefetchCaches.get(worldId);
        if (!cache) {
            return;
        }
        const tick = Math.max(0, Math.floor(Number(message.tick ?? 0)));
        if (message.type === "exact_anchor" && message.frame && message.metrics && message.timeline) {
            cache.acceptExactAnchor({
                tick,
                metrics: message.metrics,
                timeline: message.timeline,
                frame: message.frame,
            });
        } else if (message.type === "coarse_frame" && message.frame && message.metrics && message.timeline) {
            cache.acceptCoarseFrame({
                tick,
                metrics: message.metrics,
                timeline: message.timeline,
                frame: message.frame,
            });
        } else if (message.type === "exact_delta" && message.delta) {
            cache.acceptExactDelta(tick, message.delta);
        }
    }
}

function includeFieldsFromOptions(options: unknown): string[] | undefined {
    if (!options || typeof options !== "object") {
        return undefined;
    }
    const includeFields = (options as { include_fields?: unknown }).include_fields;
    return Array.isArray(includeFields)
        ? includeFields.filter((field): field is string => typeof field === "string")
        : undefined;
}

function filterPlaybackPreviewFields(
    preview: ViewDeltaResult,
    includeFields: string[] | undefined,
): ViewDeltaResult {
    if (!includeFields) {
        return preview;
    }
    const include = new Set(includeFields);
    return {
        ...preview,
        deltas: preview.deltas.filter((delta) => include.has(delta.field_kind)),
    };
}

function toFloat32Array(values: number[] | Float32Array): Float32Array {
    return values instanceof Float32Array ? values : new Float32Array(values);
}

function buildPreviewProjection(positions: Float32Array, lowCount: number): Uint32Array {
    const cellCount = Math.floor(positions.length / 3);
    const projection = new Uint32Array(cellCount);
    for (let cell = 0; cell < cellCount; cell += 1) {
        const offset = cell * 3;
        const x = positions[offset];
        const y = positions[offset + 1];
        const z = positions[offset + 2];
        let nearest = 0;
        let nearestDot = -Infinity;
        for (let candidate = 0; candidate < lowCount; candidate += 1) {
            const candidateOffset = candidate * 3;
            const dot = x * positions[candidateOffset]
                + y * positions[candidateOffset + 1]
                + z * positions[candidateOffset + 2];
            if (dot > nearestDot) {
                nearestDot = dot;
                nearest = candidate;
            }
        }
        projection[cell] = nearest;
    }
    return projection;
}

function expandFloatPreviewValues(
    values: number[] | Float32Array | null | undefined,
    projection: Uint32Array,
    lowCount: number,
): number[] | Float32Array | null | undefined {
    if (!values || !(values instanceof Float32Array) || values.length !== lowCount) {
        return values;
    }
    const expanded = new Float32Array(projection.length);
    for (let cell = 0; cell < projection.length; cell += 1) {
        expanded[cell] = values[projection[cell]];
    }
    return expanded;
}

function expandUintPreviewValues(
    values: number[] | Uint32Array | null | undefined,
    projection: Uint32Array,
    lowCount: number,
): number[] | Uint32Array | null | undefined {
    if (!values || !(values instanceof Uint32Array) || values.length !== lowCount) {
        return values;
    }
    const expanded = new Uint32Array(projection.length);
    for (let cell = 0; cell < projection.length; cell += 1) {
        expanded[cell] = values[projection[cell]];
    }
    return expanded;
}

function expandIntPreviewValues(
    values: number[] | Int32Array | null | undefined,
    projection: Uint32Array,
    lowCount: number,
): number[] | Int32Array | null | undefined {
    if (!values || !(values instanceof Int32Array) || values.length !== lowCount) {
        return values;
    }
    const expanded = new Int32Array(projection.length);
    for (let cell = 0; cell < projection.length; cell += 1) {
        expanded[cell] = values[projection[cell]];
    }
    return expanded;
}
