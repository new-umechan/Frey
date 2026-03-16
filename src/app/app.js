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
    getDeltaFieldKindsForView,
    refreshWorldStatsFromController,
    syncVisibleCoreFieldsFromController,
    syncWorldDeltaFromController,
    syncWorldFromController,
} from "./world-sync.js";
import { advanceWorldLoop, resetWorldProgress } from "./world-loop.js";
import { createPlaybackController } from "./playback-controller.js";
import {
    createBenchmarkConsoleTable,
    createBenchmarkProfile,
    createTickPerfRecorder,
    formatBenchmarkSummaryLine,
} from "./perf-benchmark.js";

function isPerfFeatureEnabled() {
    const params = new URLSearchParams(window.location.search);
    return params.get("perf") === "1" || params.get("bench") === "1";
}

const STEP_BREAKDOWN_SAMPLE_INTERVAL = 4;
const STEP_BREAKDOWN_METRIC_NAMES = [
    "step_feedback",
    "step_geology_terrain",
    "step_climate",
    "step_geology_river",
    "step_ecology",
    "step_civilization",
    "step_transition",
    "step_sync_erosion",
    "step_observe_world_change",
    "step_history_snapshot",
];

export async function createApp() {
    const isPerfEnabled = isPerfFeatureEnabled();
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
        controlHelpModal,
        controlHelpCloseButton,
        playbackControls,
        eventLogList,
        perfPanel,
        perfControls,
        perfStatFields,
        statFields,
    } = collectAppElements({ perfEnabled: isPerfEnabled });

    function setStatus(message) {
        statusMessage.textContent = message;
    }

    function setSidebarOpen(isOpen) {
        appShell.classList.toggle("is-sidebar-collapsed", !isOpen);
        sidebarToggle.setAttribute("aria-expanded", String(isOpen));
    }

    if (isPerfEnabled) {
        perfPanel.hidden = false;
        perfPanel.setAttribute("aria-hidden", "false");
    } else {
        perfPanel.hidden = true;
        perfPanel.setAttribute("aria-hidden", "true");
    }

    let lastPerfBenchmarkResult = null;
    let isPerfBenchmarkRunning = false;

    function setPerfStatus(message) {
        if (!isPerfEnabled || !perfControls) {
            return;
        }
        perfControls.status.textContent = message;
    }

    function setPerfProgress(value, max = 1) {
        if (!isPerfEnabled || !perfControls) {
            return;
        }
        const normalizedMax = Math.max(1, Math.floor(max));
        const normalizedValue = Math.max(0, Math.min(normalizedMax, Math.floor(value)));
        perfControls.progress.max = normalizedMax;
        perfControls.progress.value = normalizedValue;
    }

    function pushStepBreakdownSamples(
        perfRecorder,
        profiledResult,
        options = {},
    ) {
        if (!perfRecorder || !profiledResult) {
            return;
        }
        const stepCountKey = options.stepCountKey ?? "steps";
        const steps = Math.max(1, Math.floor(profiledResult[stepCountKey] ?? 1));
        for (const metricName of STEP_BREAKDOWN_METRIC_NAMES) {
            const rawValue = profiledResult[`${metricName}_ms`];
            if (!Number.isFinite(rawValue)) {
                continue;
            }
            perfRecorder.pushSample(metricName, rawValue / steps);
        }
    }

    function formatMs(value) {
        if (!Number.isFinite(value)) {
            return "-";
        }
        return `${value.toFixed(3)} ms`;
    }

    function renderPerfStats(result) {
        if (!isPerfEnabled || !perfStatFields) {
            return;
        }
        const metrics = result?.metrics ?? {};
        perfStatFields.tickP50.textContent = formatMs(metrics.tick_total?.p50);
        perfStatFields.tickP95.textContent = formatMs(metrics.tick_total?.p95);
        perfStatFields.stepMean.textContent = formatMs(metrics.step_world?.mean);
        perfStatFields.deltaMean.textContent = formatMs(metrics.delta_sync?.mean);
        perfStatFields.geomMean.textContent = formatMs(metrics.geometry_update?.mean);
        perfStatFields.riverMean.textContent = formatMs(metrics.river_mask_update?.mean);
    }

    function setPerfControlsDisabled(isDisabled) {
        if (!isPerfEnabled || !perfControls) {
            return;
        }
        perfControls.runButton.disabled = isDisabled;
        perfControls.copyButton.disabled = isDisabled || !lastPerfBenchmarkResult;
    }

    async function copyPerfBenchmarkResult() {
        if (!isPerfEnabled) {
            return;
        }
        if (!lastPerfBenchmarkResult) {
            setPerfStatus("No result to copy.");
            return;
        }
        const payload = JSON.stringify(lastPerfBenchmarkResult, null, 2);
        try {
            if (navigator.clipboard?.writeText) {
                await navigator.clipboard.writeText(payload);
                setPerfStatus("Copied benchmark JSON.");
                return;
            }
        } catch (error) {
            console.warn("clipboard write failed", error);
        }
        console.log("[perf-bench][json]", payload);
        setPerfStatus("Clipboard unavailable. JSON logged to console.");
    }

    setSidebarOpen(true);
    if (isPerfEnabled && perfControls) {
        setPerfStatus("Idle");
        setPerfProgress(0, 1);
        renderPerfStats(null);
        perfControls.copyButton.disabled = true;
    }
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
    const playbackState = worldState.playback;
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
        climateMetricGroup.setAttribute("aria-hidden", String(!isClimateMode));
        climateLegend.panel.setAttribute("aria-hidden", String(!isClimateMode));
        climateControlHint.setAttribute("aria-hidden", String(!isClimateMode));
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

    let playbackController = null;

    function syncWorldFromActiveController() {
        if (!activeWorldId) {
            return null;
        }
        const result = syncWorldFromController({
            worldSimController,
            worldId: activeWorldId,
            world,
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
        playbackController.syncAfterWorldSync();
        return result;
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
        const previousEra = currentEraScale;
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
        if (activeWorldId && previousEra !== currentEraScale) {
            const previousLabel = getEraScalePreset(previousEra).label;
            playbackController.appendPlaybackEvent(
                "era-changed",
                "時代遷移",
                `${previousLabel} -> ${preset.label}`,
            );
        }
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
            level: LEVEL,
        });
    }

    function getCurrentDeltaFieldKinds() {
        return getDeltaFieldKindsForView({
            viewMode: currentViewMode,
            climateMetric: currentClimateMetric,
        });
    }

    function syncVisibleFieldsForCurrentView() {
        if (!activeWorldId || !currentTerrainData) {
            return;
        }
        const changes = syncVisibleCoreFieldsFromController({
            worldSimController,
            worldId: activeWorldId,
            core: currentTerrainData,
            fieldKinds: getCurrentDeltaFieldKinds(),
        });
        terrainRenderer.applyCoreChanges(currentTerrainData, changes, currentSurfaceMode, world.tick);
    }

    function stepWorldTick(perfRecorder = null, options = {}) {
        if (!activeWorldId || !currentTerrainData) {
            return false;
        }
        const runTick = () => {
            const benchmarkMode = options?.benchmarkMode === true;
            const sampleStepBreakdown = options?.sampleStepBreakdown === true;
            const nextTick = world.tick + 1;
            const prevHeightForSnapshot = debugSnapshotTickSet.has(nextTick) && currentTerrainData?.heightData
                ? currentTerrainData.heightData.slice()
                : null;

            if (perfRecorder) {
                perfRecorder.measure("step_world", () => {
                    if (sampleStepBreakdown) {
                        const profiled = worldSimController.step_world_profiled(activeWorldId, 1);
                        pushStepBreakdownSamples(perfRecorder, profiled);
                        return;
                    }
                    worldSimController.step_world(activeWorldId, 1);
                });
            } else {
                worldSimController.step_world(activeWorldId, 1);
            }
            const shouldRefreshStats = benchmarkMode ? false : shouldRefreshStatsAtTick(nextTick);
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
                deltaFieldKinds: getCurrentDeltaFieldKinds(),
                perfRecorder,
            });
            if (!benchmarkMode && (changes?.climate || statsRefreshed)) {
                syncClimateUi();
            }

            if (!benchmarkMode) {
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
            }

            if (!benchmarkMode && world.tick > 0 && shouldRefreshStats) {
                const preset = getEraScalePreset(currentEraScale);
                setStatus(
                    `Running (${currentSeed}) | ${preset.label} / 1Tick=${currentEraMetrics.tickLabel} | tick=${world.tick}`,
                );
            }
            if (!benchmarkMode) {
                playbackController.syncAfterWorldStep();
            }
            return true;
        };
        if (perfRecorder) {
            return perfRecorder.measure("tick_total", runTick);
        }
        return runTick();
    }

    async function runPerfBenchmark() {
        if (!isPerfEnabled || isPerfBenchmarkRunning || !activeWorldId || !currentTerrainData) {
            return;
        }
        const profile = createBenchmarkProfile();
        isPerfBenchmarkRunning = true;
        setPerfControlsDisabled(true);
        setPerfStatus("Preparing benchmark profile...");
        const wasPlaying = playbackState.isPlaying;
        playbackController.setPlaybackRunning(false);

        try {
            if (currentSeed !== profile.seed) {
                await updateTerrain(profile.seed);
            }
            if (currentSurfaceMode !== profile.surfaceMode) {
                setSurfaceMode(profile.surfaceMode);
            }
            if (currentViewMode !== profile.viewMode) {
                setViewMode(profile.viewMode);
            }

            const startedAt = performance.now();
            const recorder = createTickPerfRecorder();
            setPerfStatus(`Running 0/${profile.tickCount} ticks... (0%)`);
            setPerfProgress(0, profile.tickCount + 1);
            const tickStart = world.tick;
            for (let i = 0; i < profile.tickCount; i += 1) {
                stepWorldTick(recorder, { benchmarkMode: true, sampleStepBreakdown: false });
                setPerfProgress(i + 1, profile.tickCount + 1);
                const pct = Math.floor(((i + 1) / profile.tickCount) * 100);
                setPerfStatus(`Running ${i + 1}/${profile.tickCount} ticks... (${pct}%)`);
                await new Promise((resolve) => {
                    requestAnimationFrame(() => resolve());
                });
            }
            const wallTimeMainMs = performance.now() - startedAt;

            setPerfStatus("Collecting step breakdown...");
            const breakdownStartedAt = performance.now();
            const breakdownWorld = worldSimController.init_world(profile.seed, LEVEL, {
                terrain_params: TERRAIN_PARAMS,
            });
            const breakdownResult = worldSimController.step_world_profiled_batch(
                breakdownWorld.world_id,
                profile.tickCount,
                STEP_BREAKDOWN_SAMPLE_INTERVAL,
            );
            pushStepBreakdownSamples(recorder, breakdownResult, { stepCountKey: "sampled_steps" });
            setPerfProgress(profile.tickCount + 1, profile.tickCount + 1);
            await new Promise((resolve) => {
                requestAnimationFrame(() => resolve());
            });
            const breakdownCollectionMs = performance.now() - breakdownStartedAt;

            const result = {
                meta: {
                    generated_at: new Date().toISOString(),
                    user_agent: navigator.userAgent,
                    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
                },
                profile: {
                    ...profile,
                    tickStart,
                    tickEnd: world.tick,
                },
                totals: {
                    wall_time_ms: Math.round(wallTimeMainMs * 1000) / 1000,
                    breakdown_collection_ms: Math.round(breakdownCollectionMs * 1000) / 1000,
                    processed_ticks: profile.tickCount,
                },
                metrics: recorder.buildSummary(),
            };
            lastPerfBenchmarkResult = result;
            renderPerfStats(result);
            const summaryLine = formatBenchmarkSummaryLine(result);
            setPerfStatus(`Done: ${summaryLine}`);
            setPerfProgress(profile.tickCount + 1, profile.tickCount + 1);
            console.group(`[perf-bench] ${profile.tickCount} tick benchmark`);
            console.log("result", result);
            console.table(createBenchmarkConsoleTable(result));
            console.groupEnd();
        } finally {
            playbackController.syncAfterWorldSync();
            playbackController.setPlaybackRunning(wasPlaying);
            isPerfBenchmarkRunning = false;
            setPerfControlsDisabled(false);
        }
    }

    function getLastPerfBenchmarkResult() {
        return isPerfEnabled ? lastPerfBenchmarkResult : null;
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
        syncVisibleFieldsForCurrentView();
        terrainRenderer.applyTerrainMaterialState(currentViewMode, debugEnabled, currentClimateMetric);
        syncClimateUi();
        if (normalizedMode !== "plates") {
            plateHover.hidePopup();
        }
    }

    function setClimateMetric(nextMetric) {
        currentClimateMetric = normalizeClimateMetric(nextMetric);
        if (currentViewMode === "climate") {
            syncVisibleFieldsForCurrentView();
        }
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
            playbackController.setPlaybackRunning(true);
            syncWorldFromActiveController();
            playbackController.appendPlaybackEvent("world-generated", "地形生成", `seed=${currentSeed}`);

            const eraPreset = getEraScalePreset(currentEraScale);
            setStatus(`Ready (${currentSeed}) | ${eraPreset.label} / 1Tick=${currentEraMetrics.tickLabel}`);
            seedInput.value = currentSeed;
            const activeElement = document.activeElement;
            if (activeElement instanceof HTMLElement && seedForm.contains(activeElement)) {
                activeElement.blur();
            }
        } finally {
            seedInput.removeAttribute("disabled");
            seedForm.querySelector("button")?.removeAttribute("disabled");
        }
    }

    playbackController = createPlaybackController({
        playbackControls,
        eventLogList,
        playbackState,
        worldState,
        worldSimController,
        getActiveWorldId: () => activeWorldId,
        getCurrentTerrainData: () => currentTerrainData,
        getWorldTick: () => world.tick,
        syncWorldFromActiveController,
        stepWorldTick,
        setStatus,
    });

    setupUiControls({
        canvas,
        viewportPanel,
        sidebarToggle,
        debugToggleInput,
        eraScaleSelect,
        viewModeInputs,
        climateMetricInputs,
        controlHelpModal,
        controlHelpCloseButton,
        playbackControls,
        eventLogList,
        perfEnabled: isPerfEnabled,
        perfControls,
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
        onTogglePlay: playbackController.handleTogglePlay,
        onStepForward: playbackController.handleStepForward,
        onRewind: playbackController.handleRewind,
        onHistorySeek: playbackController.handleHistorySeek,
        onHistoryStepDirection: playbackController.handleHistoryStepDirection,
        onEventLogJump: playbackController.handleHistoryJump,
        onRunPerfBenchmark: runPerfBenchmark,
        onCopyPerfBenchmark: copyPerfBenchmarkResult,
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
    playbackController.refreshHistoryTicks();
    playbackController.syncPlaybackUi();
    playbackController.notePlaybackOverlayActivity();
    playbackController.bindOverlayActivityEvents(viewportPanel);
    onResize();
    plateHover.hidePopup();

    return {
        tick(nowMs) {
            advanceWorldLoop(
                nowMs,
                worldState,
                () => playbackState.isPlaying && Boolean(currentTerrainData) && Boolean(activeWorldId),
                stepWorldTick,
            );
            cameraController.getActiveControls().update();
            renderer.render(scene, cameraController.getCamera());
        },
        getLastPerfBenchmarkResult,
    };
}
