import { buildCoreBuffers, type CoreBuffers } from "./world-core";
import { type WorldSimController } from "../../transport/wasm/frey-wasm-module";
import { type PerfProfile } from "./recorder";

export interface ControllerState {
    controller: WorldSimController;
    worldId: string;
    core: CoreBuffers;
}

export type VerificationMode = "interactive" | "headless_metrics" | "scientific_benchmark";

function initWorld(
    controller: WorldSimController,
    seed: string,
    level: number,
    terrainParams: Record<string, unknown>,
    verificationMode: VerificationMode,
): Promise<{ world_id?: string }> {
    const config = {
        geology_params: terrainParams,
        verification_mode: verificationMode,
    };
    return controller.init_world(seed, level, config);
}

export async function createControllerState(
    WorldSimControllerConstructor: new () => WorldSimController,
    profile: PerfProfile,
    level: number,
    terrainParams: Record<string, unknown>,
    verificationMode: VerificationMode,
): Promise<ControllerState> {
    const controller = new WorldSimControllerConstructor();
    const initResult = await initWorld(
        controller,
        profile.seed ?? "alpha",
        level,
        terrainParams,
        verificationMode,
    );
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

export async function rebuildControllerState(
    WorldSimControllerConstructor: new () => WorldSimController,
    profile: PerfProfile,
    level: number,
    terrainParams: Record<string, unknown>,
    verificationMode: VerificationMode,
    completedTicks: number,
    deltaFieldKinds: string[],
): Promise<ControllerState> {
    const state = await createControllerState(
        WorldSimControllerConstructor,
        profile,
        level,
        terrainParams,
        verificationMode,
    );
    if (completedTicks > 0) {
        state.controller.exec_world(state.worldId, completedTicks);
    }
    state.controller.get_world_delta(state.worldId, {
        include_fields: deltaFieldKinds,
    });
    state.core = buildCoreBuffers(state.controller, state.worldId);
    return state;
}
