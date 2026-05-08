import { buildCoreBuffers, type CoreBuffers } from "./world-core";
import { type WorldSimController } from "../../transport/wasm/frey-wasm-module";
import { type PerfProfile } from "./recorder";

export interface ControllerState {
    controller: WorldSimController;
    worldId: string;
    core: CoreBuffers;
}

export type VerificationMode = "interactive" | "headless_metrics" | "scientific_benchmark";

const ALPHA_STAGES = new Set(["environment", "life", "civilization", "history"]);

function resolveDevSnapshotStage(explicitStage?: string): string | undefined {
    const envStage = typeof process !== "undefined" ? process.env?.FREY_DEV_SNAPSHOT_STAGE : undefined;
    const raw = explicitStage ?? envStage?.trim();
    if (!raw || !ALPHA_STAGES.has(raw)) {
        return undefined;
    }
    return raw;
}

async function initWorldWithOptionalSnapshot(
    controller: WorldSimController,
    seed: string,
    level: number,
    terrainParams: Record<string, unknown>,
    verificationMode: VerificationMode,
    devSnapshotStage?: string,
): Promise<{ world_id?: string }> {
    const config = {
        geology_params: terrainParams,
        verification_mode: verificationMode,
    };
    const stage = resolveDevSnapshotStage(devSnapshotStage);
    if (seed !== "alpha" || !stage) {
        return controller.init_world(seed, level, config);
    }
    try {
        const manifestResponse = await fetch("/.dev-precomputed/alpha/manifest.json", {
            cache: "no-store",
        });
        if (!manifestResponse.ok) {
            throw new Error(`manifest fetch failed: HTTP ${manifestResponse.status}`);
        }
        const manifest = (await manifestResponse.json()) as {
            entries?: Array<{ stage?: string; filename?: string }>;
        };
        const entry = Array.isArray(manifest.entries)
            ? manifest.entries.find((candidate) => candidate?.stage === stage)
            : null;
        const filename = entry?.filename;
        if (!filename) {
            throw new Error(`manifest entry missing for stage=${stage}`);
        }
        const snapshotResponse = await fetch(`/.dev-precomputed/alpha/${filename}`, {
            cache: "no-store",
        });
        if (!snapshotResponse.ok) {
            throw new Error(`snapshot fetch failed: HTTP ${snapshotResponse.status}`);
        }
        const snapshotBytes = new Uint8Array(await snapshotResponse.arrayBuffer());
        const runtime = controller as WorldSimController & {
            init_world_from_snapshot: (
                seedInput: string,
                levelInput: number,
                configInput: unknown,
                bytes: Uint8Array,
            ) => { world_id?: string };
        };
        return runtime.init_world_from_snapshot(seed, level, config, snapshotBytes);
    } catch {
        return controller.init_world(seed, level, config);
    }
}

export async function createControllerState(
    WorldSimControllerConstructor: new () => WorldSimController,
    profile: PerfProfile,
    level: number,
    terrainParams: Record<string, unknown>,
    verificationMode: VerificationMode,
    devSnapshotStage?: string,
): Promise<ControllerState> {
    const controller = new WorldSimControllerConstructor();
    const initResult = await initWorldWithOptionalSnapshot(
        controller,
        profile.seed ?? "alpha",
        level,
        terrainParams,
        verificationMode,
        devSnapshotStage,
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
    devSnapshotStage?: string,
): Promise<ControllerState> {
    const state = await createControllerState(
        WorldSimControllerConstructor,
        profile,
        level,
        terrainParams,
        verificationMode,
        devSnapshotStage,
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
