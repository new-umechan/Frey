import { collectAppElements, type AppElements } from "../components/dom";
import {
    createStatusController,
    isPerfFeatureEnabled,
    setPerfPanelVisibility,
} from "./bootstrap/status-ui";
import {
    DEFAULT_TERRAIN_SEED,
    LEVEL,
} from "../shared/constants";
import { advanceWorldLoop } from "./sim/world-loop";
import { createMeshBuffers } from "./state/app-state";
import { bootstrapAppRuntime } from "./bootstrap/app-bootstrap";
import { createEngineWorkerClient } from "./engine/engine-worker-client";
import { initializeFreyWasm } from "../transport/wasm/frey-wasm-module";

interface SidebarControllerOptions {
    appShell: HTMLElement;
    sidebarToggle: HTMLButtonElement | null;
}

function createSidebarController(options: SidebarControllerOptions) {
    const { appShell, sidebarToggle } = options;
    function setSidebarOpen(isOpen: boolean) {
        if (!sidebarToggle) {
            return;
        }
        appShell.classList.toggle("is-sidebar-collapsed", !isOpen);
        sidebarToggle.setAttribute("aria-expanded", String(isOpen));
    }
    return { setSidebarOpen };
}

function isLocalhostRuntime() {
    const hostname = globalThis.location?.hostname ?? "";
    return hostname === "localhost" || hostname === "127.0.0.1";
}

export async function createApp() {
    const isPerfEnabled = isPerfFeatureEnabled();
    const isDevCheckpointEnabled = isLocalhostRuntime();
    const elements: AppElements = collectAppElements({ perfEnabled: isPerfEnabled });
    const {
        appShell,
        seedInput,
        sidebarToggle,
        statusMessage,
        statusEraLabel,
        eraScaleTickLabel,
        perfPanel,
        devSnapshotPanel,
        devSnapshotStageSelect,
        devSnapshotJumpButton,
    } = elements;
    const { setSidebarOpen } = createSidebarController({ appShell, sidebarToggle });
    elements.setSidebarOpen = setSidebarOpen;
    const statusRows = [statusEraLabel, eraScaleTickLabel];
    const { setStatus } = createStatusController(statusMessage, statusRows);

    setPerfPanelVisibility(perfPanel, isPerfEnabled);
    if (devSnapshotPanel) {
        devSnapshotPanel.hidden = !isDevCheckpointEnabled;
        devSnapshotPanel.setAttribute("aria-hidden", String(!isDevCheckpointEnabled));
    }
    if (sidebarToggle) {
        setSidebarOpen(true);
    }
    seedInput.value = DEFAULT_TERRAIN_SEED;
    await initializeFreyWasm();
    setStatus("Preparing mesh...");
    const bootstrapEngine = createEngineWorkerClient();
    const mesh = await bootstrapEngine.generate_mesh(LEVEL);
    bootstrapEngine.close();
    const { basePositions, indices, metricCellOverlayMesh } = createMeshBuffers(mesh);
    const runtime = await bootstrapAppRuntime({
        elements,
        isPerfEnabled,
        setStatus,
        basePositions,
        indices,
        metricCellOverlayMesh,
    });
    await runtime.runInitialSync();
    if (isDevCheckpointEnabled && devSnapshotStageSelect && devSnapshotJumpButton) {
        let pending = false;
        devSnapshotJumpButton.addEventListener("click", () => {
            if (pending) {
                return;
            }
            const stage = devSnapshotStageSelect.value;
            if (!["environment", "life", "civilization", "history"].includes(stage)) {
                setStatus(`Invalid checkpoint stage: ${stage}`);
                return;
            }
            pending = true;
            devSnapshotJumpButton.disabled = true;
            seedInput.value = "alpha";
            void runtime.updateTerrain("alpha", { devSnapshotStage: stage })
                .catch((error) => {
                    setStatus(`Dev checkpoint jump failed: ${String(error)}`);
                    console.error(error);
                })
                .finally(() => {
                    pending = false;
                    devSnapshotJumpButton.disabled = false;
                });
        });
    }

    return {
        tick(nowMs: number) {
            advanceWorldLoop(
                nowMs,
                runtime.worldState,
                runtime.shouldAdvanceWorld,
                runtime.stepWorldPlayback,
            );
            runtime.renderFrame();
        },
        getLastPerfResult: runtime.getLastPerfResult,
    };
}
