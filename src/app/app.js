import * as THREE from "three";
import initWasm, {
    WorldSimController,
    generate_mesh,
} from "../interface/wasm.js";
import { collectAppElements } from "../ui/dom.js";
import { setupUiControls } from "../ui/controls.js";
import { createPlateHoverController } from "./plate-hover-controller.js";
import { createTerrainVisualController } from "./terrain-visual-controller.js";
import { createGlobeScene, resizeViewport } from "../gfx/scene.js";
import { createCameraController } from "../gfx/views/camera-controller.js";
import { TERRAIN_PARAMS } from "../interface/params/terrain.js";
import { buildRenderPositions } from "../gfx/views/terrain-visuals.js";
import { buildRiverMaskTexture, buildTerrainUvFromPositions } from "../gfx/materials/river-mask.js";
import {
    DEBUG_SNAPSHOT_TICKS,
    DEFAULT_ERA_SCALE,
    DEFAULT_SURFACE_MODE,
    DEFAULT_TERRAIN_SEED,
    DEFAULT_VIEW_MODE,
    ERA_SCALE_PRESETS,
    formatRealYearsPerTick,
    LEVEL,
} from "../core/constants.js";
import {
    createEmptyCore,
    createEmptyLayers,
    createInitialBudgets,
    createInitialRuntimeState,
} from "../sim/runtime/state.js";
import { saveDebugSnapshotIfNeeded } from "../sim/debug/snapshot.js";

function getFieldData(controller, worldId, fieldKind) {
    const response = controller.get_field(worldId, fieldKind, 1);
    if (fieldKind === "height" || fieldKind === "river_flux" || fieldKind === "mantle_heat") {
        return new Float32Array(response?.f32_data ?? []);
    }
    if (fieldKind === "plate_id") {
        return new Uint32Array(response?.u32_data ?? []);
    }
    return new Int32Array(response?.i32_data ?? []);
}

function buildPlateInfoFromStats(plateStats) {
    const plateCount = Math.max(0, Number(plateStats?.plate_count) || 0);
    const stats = Array.isArray(plateStats?.stats) ? plateStats.stats : [];
    const isOcean = new Uint8Array(plateCount);
    const baseHeight = new Float32Array(plateCount);
    const baseWeight = new Float32Array(plateCount);

    let maxCellCount = 1;
    for (const stat of stats) {
        const cellCount = Math.max(0, Number(stat?.cell_count) || 0);
        if (cellCount > maxCellCount) {
            maxCellCount = cellCount;
        }
    }

    for (const stat of stats) {
        const plateId = Number(stat?.plate_id);
        if (!Number.isInteger(plateId) || plateId < 0 || plateId >= plateCount) {
            continue;
        }
        const meanHeight = Number(stat?.mean_height);
        const cellCount = Math.max(0, Number(stat?.cell_count) || 0);
        isOcean[plateId] = Number.isFinite(meanHeight) && meanHeight <= 0 ? 1 : 0;
        baseHeight[plateId] = Number.isFinite(meanHeight) ? meanHeight : 0;
        baseWeight[plateId] = Math.max(0.05, Math.min(1.0, cellCount / maxCellCount));
    }

    return {
        isOcean,
        baseHeight,
        baseWeight,
    };
}

function buildCoreFromController({
    heightData,
    plateId,
    riverFlux,
    riverNext,
    mantleHeat,
    plateInfo,
    targetLandRatio,
}) {
    const cellCount = heightData.length;
    const vertexWeight = new Float32Array(cellCount);
    for (let i = 0; i < cellCount; i += 1) {
        const pid = plateId[i];
        const weight = pid >= 0 && pid < plateInfo.baseWeight.length
            ? plateInfo.baseWeight[pid]
            : 0.5;
        vertexWeight[i] = weight;
    }

    return {
        heightData,
        plateId,
        riverFlux,
        riverNext,
        mantleHeat,
        lakeDepth: new Float32Array(cellCount),
        plateInfo,
        vertexWeight,
        tectonicDebug: {
            trench: new Float32Array(cellCount),
            arc: new Float32Array(cellCount),
            backarc: new Float32Array(cellCount),
            oceanOceanArc: new Float32Array(cellCount),
        },
        targetLandRatio: Number.isFinite(targetLandRatio) ? targetLandRatio : 0,
    };
}

function createEraMetrics(key = DEFAULT_ERA_SCALE) {
    const preset = Object.hasOwn(ERA_SCALE_PRESETS, key)
        ? ERA_SCALE_PRESETS[key]
        : ERA_SCALE_PRESETS[DEFAULT_ERA_SCALE];
    return {
        tickLabel: preset.tickLabel,
        runtimeTickMs: Number.isFinite(preset.runtimeTickMs) ? preset.runtimeTickMs : 120,
        budgets: {
            geology: Number(preset.weights.geology ?? 0),
            climate: Number(preset.weights.climate ?? 0),
            ecology: Number(preset.weights.ecology ?? 0),
            civilization: Number(preset.weights.civilization ?? 0),
        },
    };
}

function buildEraMetricsFromRuntime(era, metrics) {
    const fallback = createEraMetrics(era);
    return {
        tickLabel: formatRealYearsPerTick(Number(metrics?.real_years_per_tick) || 0),
        runtimeTickMs: Number(metrics?.runtime_tick_ms) || fallback.runtimeTickMs,
        budgets: {
            geology: Number(metrics?.budgets?.geology) || 0,
            climate: Number(metrics?.budgets?.climate) || 0,
            ecology: Number(metrics?.budgets?.ecology) || 0,
            civilization: Number(metrics?.budgets?.civilization) || 0,
        },
    };
}

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
    terrainMaterial.setDebugEnabled(debugEnabled);

    const terrainVisualController = createTerrainVisualController({
        geometry,
        terrainMaterial,
        basePositions,
        buildRenderPositions,
        buildRiverMaskTexture,
    });
    const hoverController = createPlateHoverController({
        canvas,
        sphere,
        geometry,
        viewportPanel,
        plateHoverPopup,
        getState: () => ({
            currentTerrainData,
            currentViewMode,
            currentSurfaceMode,
            camera: cameraController.getCamera(),
            debugEnabled,
        }),
    });

    function setSurfaceMode(nextMode) {
        const normalizedMode = nextMode === "map" ? "map" : "globe";
        if (currentSurfaceMode === normalizedMode && currentTerrainData) {
            return;
        }
        currentSurfaceMode = normalizedMode;
        terrainVisualController.updateGeometryPositions(currentTerrainData, currentSurfaceMode);
        cameraController.setSurfaceMode(normalizedMode);
        hoverController.hidePopup();
    }

    function setDebugModeEnabled(nextEnabled) {
        debugEnabled = Boolean(nextEnabled);
        debugToggleInput.checked = debugEnabled;
        wireframe.visible = debugEnabled && cameraController.getSurfaceMode() === "globe";
        terrainVisualController.applyTerrainMaterialState(currentViewMode, debugEnabled);
        hoverController.syncDebugMode();
    }

    function getEraScalePreset(key) {
        if (Object.hasOwn(ERA_SCALE_PRESETS, key)) {
            return ERA_SCALE_PRESETS[key];
        }
        return ERA_SCALE_PRESETS[DEFAULT_ERA_SCALE];
    }

    function renderEraScaleControls() {
        eraScaleSelect.value = currentEraScale;
        eraScaleTickLabel.textContent = `1 Tick: ${currentEraMetrics.tickLabel}`;
        eraScaleWeightFields.geology.textContent = currentEraMetrics.budgets.geology.toFixed(2);
        eraScaleWeightFields.climate.textContent = currentEraMetrics.budgets.climate.toFixed(2);
        eraScaleWeightFields.ecology.textContent = currentEraMetrics.budgets.ecology.toFixed(2);
        eraScaleWeightFields.civilization.textContent = currentEraMetrics.budgets.civilization.toFixed(2);
    }

    function setEraScale(nextEraScale, metrics = null) {
        currentEraScale = Object.hasOwn(ERA_SCALE_PRESETS, nextEraScale)
            ? nextEraScale
            : DEFAULT_ERA_SCALE;
        currentEraMetrics = metrics ?? createEraMetrics(currentEraScale);
        worldState.runtimeTickMs = currentEraMetrics.runtimeTickMs;
        renderEraScaleControls();
        const preset = getEraScalePreset(currentEraScale);
        setStatus(`Ready (${currentSeed}) | ${preset.label} / 1Tick=${currentEraMetrics.tickLabel}`);
    }

    function resetWorldProgress() {
        world.tick = 0;
        world.era = DEFAULT_ERA_SCALE;
        world.layers = createEmptyLayers();
        world.budgets = createInitialBudgets();
        currentEraMetrics = createEraMetrics(DEFAULT_ERA_SCALE);
        debugSnapshotSavedTicks.clear();
        worldState.accumulatorMs = 0;
        worldState.lastFrameTimeMs = null;
        worldState.pendingRiverSteps = 0;
        worldState.terrainErosionDirty = false;
        worldState.terrainCoreDirty = false;
        worldState.latestActivity.geology = 0;
        worldState.latestActivity.climate = 0;
        worldState.latestActivity.ecology = 0;
        worldState.latestActivity.civilization = 0;
        for (const key of Object.keys(worldState.carry)) {
            worldState.carry[key] = 0;
        }
        for (const key of Object.keys(worldState.executedSteps)) {
            worldState.executedSteps[key] = 0;
        }
    }

    function syncWorldFromController(worldId) {
        const metrics = worldSimController.get_metrics(worldId);
        const plateStats = worldSimController.get_plate_stats(worldId);
        const heightData = getFieldData(worldSimController, worldId, "height");
        const riverFlux = getFieldData(worldSimController, worldId, "river_flux");
        const plateId = getFieldData(worldSimController, worldId, "plate_id");
        const riverNext = getFieldData(worldSimController, worldId, "river_next");
        const mantleHeat = getFieldData(worldSimController, worldId, "mantle_heat");

        const plateInfo = buildPlateInfoFromStats(plateStats);
        const core = buildCoreFromController({
            heightData,
            plateId,
            riverFlux,
            riverNext,
            mantleHeat,
            plateInfo,
            targetLandRatio: metrics.land_ratio,
        });

        world.tick = Math.max(0, Math.floor(metrics.tick ?? 0));
        world.era = typeof metrics.era === "string" ? metrics.era : DEFAULT_ERA_SCALE;
        const nextEraMetrics = buildEraMetricsFromRuntime(world.era, metrics);
        world.budgets = { ...nextEraMetrics.budgets };
        world.core = core;
        currentTerrainData = core;

        setEraScale(world.era, nextEraMetrics);

        terrainVisualController.updateTerrainAttributes(currentTerrainData);
        terrainVisualController.updateRiverMaskTexture(currentTerrainData);
        terrainVisualController.updateGeometryPositions(currentTerrainData, currentSurfaceMode);

        statFields.vertices.textContent = `${basePositions.length / 3}`;
        statFields.level.textContent = `${LEVEL}`;
        statFields.seed.textContent = currentSeed;
        statFields.plates.textContent = `${Number(plateStats?.plate_count) || 0}`;
        statFields.land.textContent = `${((Number(metrics.land_ratio) || 0) * 100).toFixed(1)}%`;
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
        syncWorldFromController(activeWorldId);

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

        if (world.tick > 0 && world.tick % 8 === 0) {
            const preset = getEraScalePreset(currentEraScale);
            setStatus(
                `Running (${currentSeed}) | ${preset.label} / 1Tick=${currentEraMetrics.tickLabel} | tick=${world.tick}`,
            );
        }
    }

    function advanceWorldLoop(nowMs) {
        if (!Number.isFinite(nowMs)) {
            return;
        }
        if (worldState.lastFrameTimeMs === null) {
            worldState.lastFrameTimeMs = nowMs;
            return;
        }

        const frameDeltaMs = Math.min(nowMs - worldState.lastFrameTimeMs, 250);
        worldState.lastFrameTimeMs = nowMs;

        if (!worldState.isRunning || !currentTerrainData || !activeWorldId) {
            return;
        }

        worldState.accumulatorMs += frameDeltaMs;
        let ticksProcessed = 0;
        while (
            worldState.accumulatorMs >= worldState.runtimeTickMs &&
            ticksProcessed < worldState.maxTicksPerFrame
        ) {
            stepWorldTick();
            worldState.accumulatorMs -= worldState.runtimeTickMs;
            ticksProcessed += 1;
        }

        if (ticksProcessed >= worldState.maxTicksPerFrame) {
            worldState.accumulatorMs = Math.min(worldState.accumulatorMs, worldState.runtimeTickMs);
        }
    }

    function setViewMode(nextMode) {
        const normalizedMode = nextMode === "plates" || nextMode === "mantle" ? nextMode : "normal";
        currentViewMode = normalizedMode;
        for (const input of viewModeInputs) {
            input.checked = input.value === normalizedMode;
        }
        terrainVisualController.applyTerrainMaterialState(currentViewMode, debugEnabled);
        if (normalizedMode !== "plates") {
            hoverController.hidePopup();
        }
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
            resetWorldProgress();
            syncWorldFromController(activeWorldId);
            hoverController.hidePopup();

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
        seedForm,
        seedInput,
        onResize,
        onSidebarToggle: () => {
            const isOpen = sidebarToggle.getAttribute("aria-expanded") === "true";
            setSidebarOpen(!isOpen);
            requestAnimationFrame(onResize);
        },
        onPointerMove: hoverController.updateFromPointer,
        onPointerLeave: hoverController.hidePopup,
        onDebugToggle: setDebugModeEnabled,
        onEraScaleChange: (value, isDisabled) => {
            if (isDisabled) {
                renderEraScaleControls();
                return;
            }
            setEraScale(value);
        },
        onViewModeChange: setViewMode,
        onToggleSurface: setSurfaceMode,
        onToggleDebug: setDebugModeEnabled,
        getDebugEnabled: () => debugEnabled,
        getCurrentSurfaceMode: () => currentSurfaceMode,
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
    renderEraScaleControls();
    setEraScale(DEFAULT_ERA_SCALE, currentEraMetrics);
    onResize();
    hoverController.hidePopup();

    return {
        tick(nowMs) {
            advanceWorldLoop(nowMs);
            cameraController.getActiveControls().update();
            renderer.render(scene, cameraController.getCamera());
        },
    };
}
