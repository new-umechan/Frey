import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_geology,
    generate_mesh,
    type InitInput,
    type InitOutput,
} from "../../../generated/wasm/web/frey_wasm";

export type ExecModuleDocRecord = {
    phase: string;
    module: string;
    description: string;
    inbox: string;
    profile: string;
    display: string;
    execution: string;
    tick_boundary: boolean;
    reads: string[];
    writes: string[];
    feedback_targets: string[];
    depends_on: string[];
};

export type ExecModuleGraphEdgeRecord = {
    from_phase: string;
    from_module: string;
    to_phase: string;
    to_module: string;
};

export type ExecModuleGraphRecord = {
    modules: ExecModuleDocRecord[];
    edges: ExecModuleGraphEdgeRecord[];
};

export default initWasm as (input?: InitInput | Promise<InitInput>) => Promise<InitOutput>;

export function getExecModules(controller: WorldSimController): ExecModuleDocRecord[] {
    return (controller as WorldSimController & {
        exec_modules(): ExecModuleDocRecord[];
    }).exec_modules();
}

export function getExecModuleGraph(controller: WorldSimController): ExecModuleGraphRecord {
    return (controller as WorldSimController & {
        exec_module_graph(): ExecModuleGraphRecord;
    }).exec_module_graph();
}

export {
    WorldSimController,
    build_render_positions,
    generate_geology,
    generate_mesh,
};
