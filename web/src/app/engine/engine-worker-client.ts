import type {
  EngineClient,
  ExecModuleDocRecord,
  ExecModuleGraphRecord,
  InitWorldResult,
  MeshGenerationResult,
  ExecWorldSliceResult,
  ExecWorldSliceAndDeltaResult,
  ProfiledExecResult,
  WorldDeltaResult,
  MetricsResult,
  FieldResult,
  HistoryTicksResult,
  TimelineAdvanceResult,
  TimelineStateResult,
} from "./engine-client";
import type {
  EngineWorkerRequest,
  EngineWorkerResponse,
} from "./worker-protocol";

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
      // Transferableとして受け取ったArrayBuffer/TypedArrayを保持
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

  async init_world(
    seed: string,
    meshLevel: number,
    config: unknown,
    options?: { devSnapshotStage?: string },
  ): Promise<InitWorldResult> {
    return (await this.request("init_world", {
      seed,
      meshLevel,
      config,
      devSnapshotStage: options?.devSnapshotStage,
    })) as InitWorldResult;
  }

  async generate_mesh(level: number): Promise<MeshGenerationResult> {
    return (await this.request("generate_mesh", {
      level,
    })) as MeshGenerationResult;
  }

  async exec_world(worldId: string, tickCount: number): Promise<void> {
    await this.request("advance_timeline", { worldId, tickCount });
  }

  async advance_timeline(
    worldId: string,
    tickCount: number,
  ): Promise<TimelineAdvanceResult> {
    return (await this.request("advance_timeline", {
      worldId,
      tickCount,
    })) as TimelineAdvanceResult;
  }

  async advance_timeline_slice(
    worldId: string,
    workBudget: number,
  ): Promise<ExecWorldSliceResult> {
    return (await this.request("advance_timeline_slice", {
      worldId,
      workBudget,
    })) as ExecWorldSliceResult;
  }

  async advance_timeline_slice_and_delta(
    worldId: string,
    workBudget: number,
    options?: unknown,
  ): Promise<ExecWorldSliceAndDeltaResult> {
    return (await this.request("advance_timeline_slice_and_delta", {
      worldId,
      workBudget,
      options,
    })) as ExecWorldSliceAndDeltaResult;
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
    return await this.advance_timeline_slice_and_delta(
      worldId,
      workBudget,
      options,
    );
  }

  async exec_world_profiled(
    worldId: string,
    tickCount: number,
  ): Promise<ProfiledExecResult> {
    return (await this.request("exec_world_profiled", {
      worldId,
      tickCount,
    })) as ProfiledExecResult;
  }

  async get_view_delta(
    worldId: string,
    options?: unknown,
  ): Promise<WorldDeltaResult> {
    return (await this.request("get_view_delta", {
      worldId,
      options,
    })) as WorldDeltaResult;
  }

  async get_world_delta(
    worldId: string,
    options?: unknown,
  ): Promise<WorldDeltaResult> {
    return await this.get_view_delta(worldId, options);
  }

  async get_timeline_state(worldId: string): Promise<TimelineStateResult> {
    return (await this.request("get_timeline_state", {
      worldId,
    })) as TimelineStateResult;
  }

  async get_metrics(worldId: string): Promise<MetricsResult | null> {
    const result = await this.request("get_metrics", { worldId });
    return (result ?? null) as MetricsResult | null;
  }

  async get_field(
    worldId: string,
    fieldKind: string,
    window: number,
  ): Promise<FieldResult> {
    return (await this.request("get_field", {
      worldId,
      fieldKind,
      window,
    })) as FieldResult;
  }

  async list_checkpoint_ticks(worldId: string): Promise<HistoryTicksResult> {
    return (await this.request("list_checkpoint_ticks", {
      worldId,
    })) as HistoryTicksResult;
  }

  async list_history_ticks(worldId: string): Promise<HistoryTicksResult> {
    return await this.list_checkpoint_ticks(worldId);
  }

  async seek_world_to_tick(worldId: string, tick: number): Promise<void> {
    await this.request("seek_world_to_tick", { worldId, tick });
  }

  async restore_world_to_tick(worldId: string, tick: number): Promise<void> {
    await this.seek_world_to_tick(worldId, tick);
  }

  async rewind_world_by_ticks(
    worldId: string,
    tickCount: number,
  ): Promise<void> {
    await this.request("rewind_world_by_ticks", { worldId, tickCount });
  }

  async set_simulation_rate(worldId: string, rate: number): Promise<void> {
    await this.request("set_simulation_rate", { worldId, rate });
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
