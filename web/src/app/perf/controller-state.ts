import { buildCoreBuffers, type CoreBuffers } from "./world-core";
import { type WorldSimController } from "../../interface/wasm";
import { type PerfProfile } from "./recorder";

export interface ControllerState {
    controller: WorldSimController;
    worldId: string;
    core: CoreBuffers;
}

export function createControllerState(
    WorldSimControllerConstructor: new () => WorldSimController,
    profile: PerfProfile,
    level: number,
    terrainParams: Record<string, unknown>
): ControllerState {
    const controller = new WorldSimControllerConstructor();
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
    WorldSimControllerConstructor: new () => WorldSimController,
    profile: PerfProfile,
    level: number,
    terrainParams: Record<string, unknown>,
    completedTicks: number,
    deltaFieldKinds: string[]
): ControllerState {
    const state = createControllerState(WorldSimControllerConstructor, profile, level, terrainParams);
    if (completedTicks > 0) {
        state.controller.exec_world(state.worldId, completedTicks);
    }
    state.controller.get_world_delta(state.worldId, {
        include_fields: deltaFieldKinds,
    });
    state.core = buildCoreBuffers(state.controller, state.worldId);
    return state;
}
