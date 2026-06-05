export type EngineWorkerRequest =
  | {
      id: number;
      kind: "generate_mesh";
      payload: { level: number };
    }
  | {
      id: number;
      kind: "init_world";
      payload: {
        seed: string;
        meshLevel: number;
        config: unknown;
      };
    }
  | {
      id: number;
      kind: "advance_timeline";
      payload: { worldId: string; tickCount: number };
    }
  | {
      id: number;
      kind: "advance_timeline_slice";
      payload: { worldId: string; workBudget: number };
    }
  | {
      id: number;
      kind: "advance_timeline_slice_and_delta";
      payload: { worldId: string; workBudget: number; options?: unknown };
    }
  | {
      id: number;
      kind: "get_view_delta";
      payload: { worldId: string; options?: unknown };
    }
  | {
      id: number;
      kind: "get_timeline_state";
      payload: { worldId: string };
    }
  | {
      id: number;
      kind: "get_metrics";
      payload: { worldId: string };
    }
  | {
      id: number;
      kind: "get_field";
      payload: { worldId: string; fieldKind: string; window: number };
    }
  | {
      id: number;
      kind: "list_checkpoint_ticks";
      payload: { worldId: string };
    }
  | {
      id: number;
      kind: "seek_world_to_tick";
      payload: { worldId: string; tick: number };
    }
  | {
      id: number;
      kind: "rewind_world_by_ticks";
      payload: { worldId: string; tickCount: number };
    }
  | {
      id: number;
      kind: "set_simulation_rate";
      payload: { worldId: string; rate: number };
    }
  | {
      id: number;
      kind: "exec_world_profiled";
      payload: { worldId: string; tickCount: number };
    }
  | {
      id: number;
      kind: "get_exec_modules";
      payload: Record<string, never>;
    }
  | {
      id: number;
      kind: "get_exec_module_graph";
      payload: Record<string, never>;
    };

export type EngineWorkerResponse =
  | {
      id: number;
      ok: true;
      kind: EngineWorkerRequest["kind"];
      payload: unknown;
    }
  | {
      id: number;
      ok: false;
      kind: EngineWorkerRequest["kind"];
      error: string;
    };
