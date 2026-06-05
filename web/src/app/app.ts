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
import {
    closeEngineClient,
    createDefaultEngineClient,
    prepareDefaultEngineRuntime,
} from "./engine/default-engine-client";

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

export async function createApp() {
    const isPerfEnabled = isPerfFeatureEnabled();
    const elements: AppElements = collectAppElements({ perfEnabled: isPerfEnabled });
    const {
        appShell,
        seedInput,
        sidebarToggle,
        statusMessage,
        statusEraLabel,
        eraScaleTickLabel,
        perfPanel,
    } = elements;
    const { setSidebarOpen } = createSidebarController({ appShell, sidebarToggle });
    elements.setSidebarOpen = setSidebarOpen;
    const statusRows = [statusEraLabel, eraScaleTickLabel];
    const { setStatus } = createStatusController(statusMessage, statusRows);

    setPerfPanelVisibility(perfPanel, isPerfEnabled);
    if (sidebarToggle) {
        setSidebarOpen(true);
    }
    seedInput.value = DEFAULT_TERRAIN_SEED;
    await prepareDefaultEngineRuntime();
    setStatus("Preparing mesh...");
    const bootstrapEngine = await createDefaultEngineClient();
    const mesh = await bootstrapEngine.generate_mesh(LEVEL);
    closeEngineClient(bootstrapEngine);
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
