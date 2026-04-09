export type EngineWorkerRequest =
    | {
        id: number;
        kind: "init_world";
        payload: { seed: string; meshLevel: number; config: unknown };
    }
    | {
        id: number;
        kind: "exec_world";
        payload: { worldId: string; tickCount: number };
    }
    | {
        id: number;
        kind: "exec_world_slice";
        payload: { worldId: string; workBudget: number };
    }
    | {
        id: number;
        kind: "get_world_delta";
        payload: { worldId: string; options?: unknown };
    }
    | {
        id: number;
        kind: "get_metrics";
        payload: { worldId: string };
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
