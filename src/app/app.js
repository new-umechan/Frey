import * as THREE from "three";
import initWasm, {
    WorldSimController,
    generate_mesh,
} from "../interface/wasm.js";
import { collectAppElements } from "../ui/dom.js";
import { setupUiControls } from "../ui/controls.js";
import { createPlateHover } from "./plate-hover.js";
import {
    getClimateMetricMeta,
    normalizeClimateMetric,
} from "./climate-metric.js";
import { createTerrainRenderer } from "./terrain-renderer.js";
import { createGlobeScene, resizeViewport } from "../gfx/scene.js";
import { createCameraController } from "../gfx/views/camera-controller.js";
import { TERRAIN_PARAMS } from "../interface/params/terrain.js";
import { buildRenderPositions } from "../gfx/views/terrain-visuals.js";
import { buildRiverMaskTexture, buildTerrainUvFromPositions } from "../gfx/materials/river-mask.js";
import {
    buildEraMetricsFromRuntime,
    createEraMetrics,
    getEraScalePreset,
    renderEraScaleControls,
} from "./era-presets.js";
import {
    DEBUG_SNAPSHOT_TICKS,
    DEFAULT_CLIMATE_METRIC,
    DEFAULT_ERA_SCALE,
    DEFAULT_SURFACE_MODE,
    DEFAULT_TERRAIN_SEED,
    DEFAULT_VIEW_MODE,
    LEVEL,
} from "../core/constants.js";
import {
    createEmptyCore,
    createEmptyLayers,
    createInitialBudgets,
    createInitialRuntimeState,
} from "../sim/runtime/state.js";
import { saveDebugSnapshotIfNeeded } from "../sim/debug/snapshot.js";
import {
    refreshWorldStatsFromController,
    syncWorldDeltaFromController,
    syncWorldFromController,
} from "./world-sync.js";
import { advanceWorldLoop, resetWorldProgress } from "./world-loop.js";

export async function createApp() {
    const {
        appShell,
        canvas,
        viewportPanel,
        seedForm,
        seedInput,
        sidebarToggle,
        statusMessage,
        plateHoverPopup,
        debugToggleInput,
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        viewModeInputs,
        climateMetricGroup,
        climateMetricInputs,
        climateLegend,
        climateControlHint,
        statFields,
    } = collectAppElements();

    function setStatus(message) {
        statusMessage.textContent = message;
    }

    function setSidebarOpen(isOpen) {
        appShell.classList.toggle("is-sidebar-collapsed", !isOpen);
        sidebarToggle.setAttribute("aria-expanded", String(isOpen));
    }

    setSidebarOpen(true);
    seedInput.value = DEFAULT_TERRAIN_SEED;
    setStatus("Loading WASM...");

    await initWasm();
    setStatus("Preparing mesh...");

    const mesh = generate_mesh(LEVEL);
    const basePositions = new Float32Array(mesh.positions);
    const indices = new Uint32Array(mesh.indices);

    const {
        scene,
        globeCamera,
        mapCamera,
        renderer,
        globeControls,
        mapControls,
        geometry,
        sphere,
        wireframe,
        halo,
        terrainMaterial,
    } = createGlobeScene(canvas, indices);
    let debugEnabled = debugToggleInput.checked;
    const cameraController = createCameraController({
        globeCamera,
        mapCamera,
        globeControls,
        mapControls,
        sphere,
        wireframe,
        halo,
        resizeViewport,
        viewportPanel,
        renderer,
        isDebugEnabled: () => debugEnabled,
    });

    let generationToken = 0;
    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentClimateMetric = DEFAULT_CLIMATE_METRIC;
    let currentSurfaceMode = DEFAULT_SURFACE_MODE;
    let currentEraScale = DEFAULT_ERA_SCALE;
    let currentEraMetrics = createEraMetrics(DEFAULT_ERA_SCALE);
    const worldSimController = new WorldSimController();
    let activeWorldId = null;
    const world = {
        tick: 0,
        era: DEFAULT_ERA_SCALE,
        mesh: {
            positions: basePositions,
            indices,
            nbrOffsets: null,
            nbrs: null,
        },
        core: createEmptyCore(),
        layers: createEmptyLayers(),
        budgets: createInitialBudgets(),
        runtime: createInitialRuntimeState(currentEraMetrics.runtimeTickMs),
    };
    let currentTerrainData = world.core;
    const worldState = world.runtime;
    const debugSnapshotTickSet = new Set(DEBUG_SNAPSHOT_TICKS);
    const debugSnapshotSavedTicks = new Set();

    const vertexCount = basePositions.length / 3;
    const terrainUv = buildTerrainUvFromPositions(basePositions);
    geometry.setAttribute("terrainUv", new THREE.BufferAttribute(terrainUv, 2));
    geometry.setAttribute("terrainHeight", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainRiverFlux", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainMantleHeat", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainTemperature", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute(
        "terrainPrecipitation",
        new THREE.BufferAttribute(new Float32Array(vertexCount), 1),
    );
    geometry.setAttribute("terrainPlateId", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainLakeDepth", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugTrench", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugArc", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainDebugBackarc", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute(
        "terrainDebugOceanOceanArc",
        new THREE.BufferAttribute(new Float32Array(vertexCount), 1),
    );
    terrainMaterial.setViewMode(currentViewMode);
    terrainMaterial.setClimateMetric(currentClimateMetric);
    terrainMaterial.setDebugEnabled(debugEnabled);

    const terrainRenderer = createTerrainRenderer({
        geometry,
        terrainMaterial,
        basePositions,
        buildRenderPositions,
        buildRiverMaskTexture,
    });
    const plateHover = createPlateHover({
        canvas,
        sphere,
        geometry,
        viewportPanel,
        plateHoverPopup,
        getState: () => ({
            currentTerrainData,
            currentViewMode,
            currentClimateMetric,
            currentSurfaceMode,
            camera: cameraController.getCamera(),
            debugEnabled,
        }),
        onClimateHover: updateClimateHoverReadout,
    });

    function computeClimateLegendStats(metricKey) {
        const values = currentTerrainData?.[metricKey];
        if (!values || values.length === 0) {
            return null;
        }
        let min = Number.POSITIVE_INFINITY;
        let max = Number.NEGATIVE_INFINITY;
        for (let i = 0; i < values.length; i += 1) {
            const value = values[i];
            if (!Number.isFinite(value)) {
                continue;
            }
            min = Math.min(min, value);
            max = Math.max(max, value);
        }
        if (!Number.isFinite(min) || !Number.isFinite(max)) {
            return null;
        }
        return {
            min,
            mid: (min + max) * 0.5,
            max,
        };
    }

    function updateClimateHoverReadout(payload) {
        climateLegend.hover.textContent = payload
            ? `Hover: ${payload.label} ${payload.value}`
            : "Hover: -";
    }

    function syncClimateUi() {
        const isClimateMode = currentViewMode === "climate";
        climateMetricGroup.hidden = !isClimateMode;
        climateLegend.panel.hidden = !isClimateMode;
        climateControlHint.hidden = !isClimateMode;
        for (const input of climateMetricInputs) {
            input.checked = input.value === currentClimateMetric;
        }
        if (!isClimateMode) {
            updateClimateHoverReadout(null);
            return;
        }
        const meta = getClimateMetricMeta(currentClimateMetric);
        const stats = computeClimateLegendStats(meta.key);
        climateLegend.panel.dataset.metric = currentClimateMetric;
        climateLegend.title.textContent = `${meta.label} (${meta.unit})`;
        climateLegend.min.textContent = stats ? meta.formatter(stats.min) : "-";
        climateLegend.mid.textContent = stats ? meta.formatter(stats.mid) : "-";
        climateLegend.max.textContent = stats ? meta.formatter(stats.max) : "-";
    }

    function setSurfaceMode(nextMode) {
        const normalizedMode = nextMode === "map" ? "map" : "globe";
        if (currentSurfaceMode === normalizedMode && currentTerrainData) {
            return;
        }
        currentSurfaceMode = normalizedMode;
        terrainRenderer.updateGeometryPositions(currentTerrainData, currentSurfaceMode, {
            force: true,
            heightChanged: true,
            tick: world.tick,
        });
        cameraController.setSurfaceMode(normalizedMode);
        plateHover.hidePopup();
    }

    function setDebugModeEnabled(nextEnabled) {
        debugEnabled = Boolean(nextEnabled);
        debugToggleInput.checked = debugEnabled;
        wireframe.visible = debugEnabled && cameraController.getSurfaceMode() === "globe";
        terrainRenderer.applyTerrainMaterialState(currentViewMode, debugEnabled, currentClimateMetric);
        plateHover.syncDebugMode();
    }

    function setEraScale(nextEraScale, metrics = null) {
        currentEraScale = getEraScalePreset(nextEraScale).key ?? DEFAULT_ERA_SCALE;
        currentEraMetrics = metrics ?? createEraMetrics(currentEraScale);
        worldState.runtimeTickMs = currentEraMetrics.runtimeTickMs;
        renderEraScaleControls(
            eraScaleSelect,
            eraScaleTickLabel,
            eraScaleWeightFields,
            currentEraScale,
            currentEraMetrics,
        );
        const preset = getEraScalePreset(currentEraScale);
        setStatus(`Ready (${currentSeed}) | ${preset.label} / 1Tick=${currentEraMetrics.tickLabel}`);
    }

    function shouldRefreshStatsAtTick(tick) {
        return (tick % 8) === 0;
    }

    function refreshActiveWorldStats() {
        return refreshWorldStatsFromController({
            worldSimController,
            worldId: activeWorldId,
            world,
            currentSeed,
            statFields,
            basePositions,
            level: LEVEL,
        });
    }

    function stepWorldTick() {
        if (!activeWorldId || !currentTerrainData) {
            return;
        }

        const nextTick = world.tick + 1;
        const prevHeightForSnapshot = debugSnapshotTickSet.has(nextTick) && currentTerrainData?.heightData
            ? currentTerrainData.heightData.slice()
            : null;

        worldSimController.step_world(activeWorldId, 1);
        const shouldRefreshStats = shouldRefreshStatsAtTick(nextTick);
        const { changes, statsRefreshed } = syncWorldDeltaFromController({
            worldSimController,
            worldId: activeWorldId,
            world,
            currentSurfaceMode,
            terrainRenderer,
            createEraMetrics,
            buildEraMetricsFromRuntime,
            setEraScale,
            refreshStats: shouldRefreshStats,
            refreshWorldStats: refreshActiveWorldStats,
        });
        if (changes?.climate || statsRefreshed) {
            syncClimateUi();
        }

        void saveDebugSnapshotIfNeeded({
            isDev: import.meta.env.DEV,
            tick: world.tick,
            debugSnapshotTickSet,
            debugSnapshotSavedTicks,
            currentTerrainData,
            currentSeed,
            currentEraScale,
            world,
            worldState,
            prevHeightForSnapshot,
            setStatus,
        });

        if (world.tick > 0 && shouldRefreshStats) {
            const preset = getEraScalePreset(currentEraScale);
            setStatus(
                `Running (${currentSeed}) | ${preset.label} / 1Tick=${currentEraMetrics.tickLabel} | tick=${world.tick}`,
            );
        }
    }

    function setViewMode(nextMode) {
        const normalizedMode = (
            nextMode === "plates"
            || nextMode === "mantle"
            || nextMode === "climate"
        )
            ? nextMode
            : "normal";
        currentViewMode = normalizedMode;
        for (const input of viewModeInputs) {
            input.checked = input.value === normalizedMode;
        }
        terrainRenderer.applyTerrainMaterialState(currentViewMode, debugEnabled, currentClimateMetric);
        syncClimateUi();
        if (normalizedMode !== "plates") {
            plateHover.hidePopup();
        }
    }

    function setClimateMetric(nextMetric) {
        currentClimateMetric = normalizeClimateMetric(nextMetric);
        terrainRenderer.applyTerrainMaterialState(currentViewMode, debugEnabled, currentClimateMetric);
        syncClimateUi();
        plateHover.hidePopup();
    }

    function onResize() {
        cameraController.onResize();
    }

    async function updateTerrain(seed) {
        const token = ++generationToken;
        const nextSeed = seed.trim() || DEFAULT_TERRAIN_SEED;

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        try {
            const initResult = worldSimController.init_world(nextSeed, LEVEL, {
                terrain_params: TERRAIN_PARAMS,
            });
            if (token !== generationToken) {
                return;
            }

            currentSeed = nextSeed;
            activeWorldId = initResult.world_id;
            currentEraMetrics = resetWorldProgress(
                world,
                worldState,
                debugSnapshotSavedTicks,
                createEmptyLayers,
                createInitialBudgets,
                createEraMetrics,
            );
            syncWorldFromController({
                worldSimController,
                worldId: activeWorldId,
                world,
                basePositions,
                currentSeed,
                currentSurfaceMode,
                terrainRenderer,
                createEraMetrics,
                buildEraMetricsFromRuntime,
                setEraScale,
                setCurrentTerrainData: (core) => {
                    currentTerrainData = core;
                },
                statFields,
                level: LEVEL,
            });
            syncClimateUi();
            plateHover.hidePopup();

            const eraPreset = getEraScalePreset(currentEraScale);
            setStatus(`Ready (${currentSeed}) | ${eraPreset.label} / 1Tick=${currentEraMetrics.tickLabel}`);
            seedInput.value = currentSeed;
        } finally {
            seedInput.removeAttribute("disabled");
            seedForm.querySelector("button")?.removeAttribute("disabled");
        }
    }

    setupUiControls({
        canvas,
        viewportPanel,
        sidebarToggle,
        debugToggleInput,
        eraScaleSelect,
        viewModeInputs,
        climateMetricInputs,
        seedForm,
        seedInput,
        onResize,
        onSidebarToggle: () => {
            const isOpen = sidebarToggle.getAttribute("aria-expanded") === "true";
            setSidebarOpen(!isOpen);
            requestAnimationFrame(onResize);
        },
        onPointerMove: plateHover.updateFromPointer,
        onPointerLeave: plateHover.hidePopup,
        onDebugToggle: setDebugModeEnabled,
        onEraScaleChange: (value, isDisabled) => {
            if (isDisabled) {
                renderEraScaleControls();
                return;
            }
            setEraScale(value);
        },
        onViewModeChange: setViewMode,
        onClimateMetricChange: setClimateMetric,
        onToggleSurface: setSurfaceMode,
        onToggleDebug: setDebugModeEnabled,
        getDebugEnabled: () => debugEnabled,
        getCurrentSurfaceMode: () => currentSurfaceMode,
        getCurrentViewMode: () => currentViewMode,
        onSubmitSeed: updateTerrain,
        onSubmitSeedError: (error) => {
            setStatus(`Generation failed: ${String(error)}`);
            seedInput.removeAttribute("disabled");
            seedForm.querySelector("button")?.removeAttribute("disabled");
            console.error(error);
        },
    });

    await updateTerrain(DEFAULT_TERRAIN_SEED);
    eraScaleSelect.setAttribute("disabled", "disabled");
    eraScaleSelect.setAttribute("aria-disabled", "true");
    eraScaleSelect.title = "時代プリセットは進行状況に応じて自動切り替えされます。";
    renderEraScaleControls(
        eraScaleSelect,
        eraScaleTickLabel,
        eraScaleWeightFields,
        currentEraScale,
        currentEraMetrics,
    );
    setEraScale(DEFAULT_ERA_SCALE, currentEraMetrics);
    syncClimateUi();
    onResize();
    plateHover.hidePopup();

    return {
        tick(nowMs) {
            advanceWorldLoop(
                nowMs,
                worldState,
                () => worldState.isRunning && Boolean(currentTerrainData) && Boolean(activeWorldId),
                stepWorldTick,
            );
            cameraController.getActiveControls().update();
            renderer.render(scene, cameraController.getCamera());
        },
    };
}
