import * as THREE from "three";
import initWasm, {
    CrustTerrainAutomaton,
    WorldTimeController,
    generate_mesh,
    init_erosion_automaton,
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
    LEVEL,
    SUBSYSTEM_ACTIVITY_QUEUE_PRESSURE_GAIN,
    WORLD_SUBSYSTEM_KEYS,
} from "../core/constants.js";
import {
    createEmptyCore,
    createEmptyLayers,
    createInitialBudgets,
    createInitialRuntimeState,
} from "../sim/runtime/state.js";
import { createPlateMotionState } from "../sim/terrain/plate-motion.js";
import {
    buildObservedActivityForTick,
    clamp01,
} from "../sim/runtime/activity.js";
import {
    ensureRequiredLayers,
    stepLayersWithBudgets,
} from "../sim/layers/updates.js";
import { saveDebugSnapshotIfNeeded } from "../sim/debug/snapshot.js";
import { runCrustGeneration } from "../sim/terrain/generation/run-crust-generation.js";
import { buildTerrainBundle } from "../sim/terrain/generation/build-terrain-bundle.js";
import { applyTerrainBundle } from "../sim/terrain/generation/apply-terrain-bundle.js";
import { runTerrainCoreStep } from "../sim/terrain/core-step.js";
import { drainRiverQueue, enqueueRiverStep } from "../sim/terrain/river-step.js";

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
    let terrainDeltaAccum = 0;
    let plateReassignAccum = 0;
    let terrainSkipNoNeighbors = 0;
    let terrainSkipNoPlateMotion = 0;
    const worldTimeController = new WorldTimeController();
    const world = {
        tick: 0,
        era: DEFAULT_ERA_SCALE,
        mesh: {
            positions: basePositions,
            indices,
            nbrOffsets: mesh.nbr_offsets ? new Uint32Array(mesh.nbr_offsets) : null,
            nbrs: mesh.nbrs ? new Uint32Array(mesh.nbrs) : null,
        },
        core: createEmptyCore(),
        layers: createEmptyLayers(),
        budgets: createInitialBudgets(),
        runtime: createInitialRuntimeState(getEraScalePresetRuntimeTickMs(DEFAULT_ERA_SCALE)),
    };
    let currentTerrainData = world.core;
    const worldState = world.runtime;
    let plateMotionState = null;
    const debugSnapshotTickSet = new Set(DEBUG_SNAPSHOT_TICKS);
    const debugSnapshotSavedTicks = new Set();

    const vertexCount = basePositions.length / 3;
    const terrainUv = buildTerrainUvFromPositions(basePositions);
    geometry.setAttribute("terrainUv", new THREE.BufferAttribute(terrainUv, 2));
    geometry.setAttribute("terrainHeight", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
    geometry.setAttribute("terrainRiverFlux", new THREE.BufferAttribute(new Float32Array(vertexCount), 1));
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

    function getEraScalePresetRuntimeTickMs(key) {
        const preset = Object.hasOwn(ERA_SCALE_PRESETS, key)
            ? ERA_SCALE_PRESETS[key]
            : ERA_SCALE_PRESETS[DEFAULT_ERA_SCALE];
        return Number.isFinite(preset.runtimeTickMs) ? preset.runtimeTickMs : 120;
    }

    function getEraScalePreset(key) {
        if (Object.hasOwn(ERA_SCALE_PRESETS, key)) {
            return ERA_SCALE_PRESETS[key];
        }
        return ERA_SCALE_PRESETS[DEFAULT_ERA_SCALE];
    }

    function syncTimeStateFromRust() {
        const nextEraScale = worldTimeController.eraKey();
        if (nextEraScale !== currentEraScale) {
            setEraScale(nextEraScale);
        }
        world.tick = Math.max(0, Math.floor(worldTimeController.tick()));
        world.era = currentEraScale;
    }

    function renderEraScaleControls() {
        const preset = getEraScalePreset(currentEraScale);
        eraScaleSelect.value = currentEraScale;
        eraScaleTickLabel.textContent = `1 Tick: ${preset.tickLabel}`;
        eraScaleWeightFields.terrain.textContent = preset.weights.terrain.toFixed(2);
        eraScaleWeightFields.river.textContent = preset.weights.river.toFixed(2);
        eraScaleWeightFields.climate.textContent = preset.weights.climate.toFixed(2);
        eraScaleWeightFields.ecology.textContent = preset.weights.ecology.toFixed(2);
        eraScaleWeightFields.civilization.textContent = preset.weights.civilization.toFixed(2);
    }

    function setEraScale(nextEraScale) {
        currentEraScale = Object.hasOwn(ERA_SCALE_PRESETS, nextEraScale)
            ? nextEraScale
            : DEFAULT_ERA_SCALE;
        worldState.runtimeTickMs = getEraScalePresetRuntimeTickMs(currentEraScale);
        renderEraScaleControls();
        const preset = getEraScalePreset(currentEraScale);
        setStatus(`Ready (${currentSeed}) | ${preset.label} / 1Tick=${preset.tickLabel}`);
    }

    function resetWorldProgress() {
        worldTimeController.reset();
        syncTimeStateFromRust();
        debugSnapshotSavedTicks.clear();
        world.budgets = createInitialBudgets();
        worldState.accumulatorMs = 0;
        worldState.lastFrameTimeMs = null;
        worldState.pendingRiverSteps = 0;
        worldState.terrainErosionDirty = false;
        worldState.terrainCoreDirty = false;
        worldState.latestActivity.terrain = 0;
        worldState.latestActivity.river = 1;
        worldState.latestActivity.climate = 1;
        worldState.latestActivity.ecology = 1;
        worldState.latestActivity.civilization = 1;
        for (const key of WORLD_SUBSYSTEM_KEYS) {
            worldState.carry[key] = 0;
            worldState.executedSteps[key] = 0;
        }
        worldState.pendingRiverSteps = 0;
        worldState.terrainErosionDirty = false;
        worldState.terrainCoreDirty = false;
        worldState.terrainDynamics = null;
    }

    function computeBudgetsForCurrentTick(nextWorld, preset) {
        const budgets = createInitialBudgets();
        for (const subsystemKey of WORLD_SUBSYSTEM_KEYS) {
            const weight = preset?.weights?.[subsystemKey] ?? 0;
            if (!Number.isFinite(weight) || weight <= 0) {
                continue;
            }
            nextWorld.runtime.carry[subsystemKey] += weight;
            const steps = Math.floor(nextWorld.runtime.carry[subsystemKey]);
            if (steps <= 0) {
                continue;
            }
            nextWorld.runtime.carry[subsystemKey] -= steps;
            budgets[subsystemKey] = steps;
        }
        nextWorld.budgets = budgets;
    }

    function runTerrainStep(steps) {
        if (!Number.isFinite(steps) || steps <= 0) {
            return;
        }
        for (let i = 0; i < steps; i += 1) {
            worldState.executedSteps.terrain += 1;
            updateTerrainCoreStep();
        }
    }

    function updateTerrainCoreStep() {
        const coreStep = runTerrainCoreStep({
            currentTerrainData,
            world,
            worldState,
            basePositions,
            currentEraScale,
            currentSeed,
            plateMotionState,
        });
        plateMotionState = coreStep.plateMotionState;
        terrainDeltaAccum += coreStep.terrainDeltaDelta;
        plateReassignAccum += coreStep.plateReassignDelta;
        terrainSkipNoNeighbors += coreStep.skipNoNeighborsDelta;
        terrainSkipNoPlateMotion += coreStep.skipNoPlateMotionDelta;
    }

    function stepWorldTick() {
        if (!currentTerrainData) {
            return;
        }
        syncTimeStateFromRust();
        const preset = getEraScalePreset(currentEraScale);
        world.era = currentEraScale;
        computeBudgetsForCurrentTick(world, preset);
        ensureRequiredLayers(world);

        runTerrainStep(world.budgets.terrain);
        enqueueRiverStep(worldState, world.budgets.river);
        stepLayersWithBudgets({
            world,
            worldState,
            currentTerrainData,
            basePositions,
            budgets: world.budgets,
        });

        if (worldState.terrainCoreDirty) {
            terrainVisualController.updateTerrainAttributes(currentTerrainData);
            terrainVisualController.updateGeometryPositions(currentTerrainData, currentSurfaceMode);
            worldState.terrainCoreDirty = false;
        }

        const terrainActivity = buildObservedActivityForTick(worldState, world.budgets, "terrain", preset);
        const riverActivity = buildObservedActivityForTick(worldState, world.budgets, "river", preset);
        const climateActivity = buildObservedActivityForTick(worldState, world.budgets, "climate", preset);
        const ecologyActivity = buildObservedActivityForTick(worldState, world.budgets, "ecology", preset);
        const civilizationActivity = buildObservedActivityForTick(
            worldState,
            world.budgets,
            "civilization",
            preset,
        );
        const riverQueuePressure = clamp01(
            worldState.pendingRiverSteps * SUBSYSTEM_ACTIVITY_QUEUE_PRESSURE_GAIN,
        );
        worldTimeController.observeActivity(
            terrainActivity,
            Math.max(riverActivity, riverQueuePressure),
            climateActivity,
            ecologyActivity,
            civilizationActivity,
        );
        worldState.latestActivity.terrain = 0;
        worldState.latestActivity.river = 0;
        worldState.latestActivity.climate = 0;
        worldState.latestActivity.ecology = 0;
        worldState.latestActivity.civilization = 0;
        syncTimeStateFromRust();

        const nextTick = world.tick + 1;
        const prevHeightForSnapshot = debugSnapshotTickSet.has(nextTick) && currentTerrainData?.heightData
            ? currentTerrainData.heightData.slice()
            : null;

        worldTimeController.step(1);
        world.tick = Math.max(0, Math.floor(worldTimeController.tick()));
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
                `Running (${currentSeed}) | ${preset.label} / 1Tick=${preset.tickLabel} | tick=${world.tick} | terrainΔ=${terrainDeltaAccum.toExponential(2)} | plateReassign=${plateReassignAccum} | skipNbr=${terrainSkipNoNeighbors} | skipMotion=${terrainSkipNoPlateMotion}`,
            );
            terrainDeltaAccum = 0;
            plateReassignAccum = 0;
            terrainSkipNoNeighbors = 0;
            terrainSkipNoPlateMotion = 0;
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

        if (!worldState.isRunning || !currentTerrainData) {
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

        const preset = getEraScalePreset(currentEraScale);
        drainRiverQueue({
            worldState,
            currentTerrainData,
            currentEraScale,
            preset,
            applyTerrainVisualUpdates: () => {
                terrainVisualController.updateTerrainAttributes(currentTerrainData);
                terrainVisualController.updateRiverMaskTexture(currentTerrainData);
                terrainVisualController.updateGeometryPositions(currentTerrainData, currentSurfaceMode);
            },
        });
    }

    function setViewMode(nextMode) {
        const normalizedMode = nextMode === "plates" ? "plates" : "normal";
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
        const waitNextFrame = () =>
            new Promise((resolve) => requestAnimationFrame(resolve));

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        await waitNextFrame();
        if (token !== generationToken) {
            return;
        }

        const terrain = await runCrustGeneration({
            seed: nextSeed,
            token,
            getActiveToken: () => generationToken,
            terrainParams: TERRAIN_PARAMS,
            CrustTerrainAutomaton,
            setStatus,
            waitNextFrame,
        });
        if (!terrain) {
            return;
        }

        const terrainBundle = buildTerrainBundle({
            terrain,
            seed: nextSeed,
            terrainParams: TERRAIN_PARAMS,
            initErosionAutomaton: init_erosion_automaton,
        });
        if (token !== generationToken) {
            return;
        }

        currentTerrainData = applyTerrainBundle({
            world,
            worldState,
            createEmptyLayers,
            terrainBundle,
        });
        plateMotionState = createPlateMotionState({
            terrainData: currentTerrainData,
            basePositions,
            seed: nextSeed,
        });
        terrainVisualController.updateTerrainAttributes(currentTerrainData);
        terrainVisualController.updateRiverMaskTexture(currentTerrainData);
        terrainVisualController.updateGeometryPositions(currentTerrainData, currentSurfaceMode);
        terrainVisualController.applyTerrainMaterialState(currentViewMode, debugEnabled);
        hoverController.hidePopup();

        const { plateCount, landRatio } = terrainBundle;

        currentSeed = nextSeed;
        resetWorldProgress();
        statFields.vertices.textContent = `${basePositions.length / 3}`;
        statFields.level.textContent = `${LEVEL}`;
        statFields.seed.textContent = currentSeed;
        statFields.plates.textContent = `${plateCount}`;
        statFields.land.textContent = `${(landRatio * 100).toFixed(1)}%`;

        const eraPreset = getEraScalePreset(currentEraScale);
        setStatus(`Ready (${currentSeed}) | ${eraPreset.label} / 1Tick=${eraPreset.tickLabel}`);
        seedInput.value = currentSeed;
        seedInput.removeAttribute("disabled");
        seedForm.querySelector("button")?.removeAttribute("disabled");
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
    setEraScale(DEFAULT_ERA_SCALE);
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
