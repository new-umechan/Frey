import { buildCoreBuffers, type CoreBuffers } from "./world-core.js";

export interface ControllerState {
    controller: any;
    worldId: string;
    core: CoreBuffers;
}

export function createControllerState(
    WorldSimController: any,
    profile: any,
    level: number,
    terrainParams: any
): ControllerState {
    const controller = new WorldSimController();
    const initResult = controller.init_world(profile.seed ?? "alpha", level, {
        geology_params: terrainParams,
    });
    const worldId = initResult?.world_id;
    if (!worldId) {
        throw new Error("performance run failed: missing world id");
    }
    return {
        controller,
        worldId,
        core: buildCoreBuffers(controller, worldId),
    };
}

export function rebuildControllerState(
    WorldSimController: any,
    profile: any,
    level: number,
    terrainParams: any,
    completedTicks: number,
    deltaFieldKinds: string[]
): ControllerState {
    const state = createControllerState(WorldSimController, profile, level, terrainParams);
    if (completedTicks > 0) {
        state.controller.exec_world(state.worldId, completedTicks);
    }
    state.controller.get_world_delta(state.worldId, {
        include_fields: deltaFieldKinds,
    });
    state.core = buildCoreBuffers(state.controller, state.worldId);
    return state;
}
