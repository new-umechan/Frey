import initWasm, {
    generate_mesh,
} from "../interface/wasm";
import { collectAppElements } from "../ui/dom.js";
import {
    createStatusController,
    isPerfFeatureEnabled,
    setPerfPanelVisibility,
} from "./bootstrap/status-ui.js";
import {
    DEFAULT_TERRAIN_SEED,
    LEVEL,
} from "../core/constants.js";
import { advanceWorldLoop } from "./sim/world-loop.js";
import { createMeshBuffers } from "./core/app-state.js";
import { bootstrapAppRuntime } from "./bootstrap/app-bootstrap.js";

function createSidebarController(options = {}) {
    const { appShell, sidebarToggle } = options;
    function setSidebarOpen(isOpen) {
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
    const elements = collectAppElements({ perfEnabled: isPerfEnabled });
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
    setStatus("Loading WASM...");
    await initWasm();
    setStatus("Preparing mesh...");
    const mesh = generate_mesh(LEVEL);
    const { basePositions, indices } = createMeshBuffers(mesh);
    const runtime = bootstrapAppRuntime({
        elements,
        isPerfEnabled,
        setStatus,
        basePositions,
        indices,
    });
    await runtime.runInitialSync();

    return {
        tick(nowMs) {
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
