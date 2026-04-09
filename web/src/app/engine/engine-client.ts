import type { ExecModuleDocRecord, ExecModuleGraphRecord } from "../../interface/wasm";

export interface EngineClient {
    generate_mesh: (level: number) => Promise<any>;
    init_world: (seed: string, meshLevel: number, config: unknown) => Promise<any>;
    exec_world: (worldId: string, tickCount: number) => Promise<void>;
    exec_world_slice: (worldId: string, workBudget: number) => Promise<any>;
    exec_world_profiled: (worldId: string, tickCount: number) => Promise<any>;
    get_world_delta: (worldId: string, options?: unknown) => Promise<any>;
    get_metrics: (worldId: string) => Promise<any>;
    get_field: (worldId: string, fieldKind: string, window: number) => Promise<any>;
    list_history_ticks: (worldId: string) => Promise<any>;
    restore_world_to_tick: (worldId: string, tick: number) => Promise<void>;
    get_exec_modules: () => Promise<ExecModuleDocRecord[]>;
    get_exec_module_graph: () => Promise<ExecModuleGraphRecord>;
}
