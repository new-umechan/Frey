import * as THREE from "three";
import initWasm, {
    generate_mesh,
    generate_terrain,
    init_erosion_automaton,
    step_erosion_automaton,
} from "./wasm/frey_wasm.js";
import { collectAppElements } from "./app/dom.js";
import { createGlobeScene, resizeViewport } from "./app/scene.js";
import { TERRAIN_LEVEL, TERRAIN_PARAMS } from "./app/terrain-params.js";
import { buildRenderPositions } from "./app/terrain-visuals.js";
import { buildRiverMaskTexture, buildTerrainUvFromPositions } from "./app/river-mask.js";

const LEVEL = TERRAIN_LEVEL;
const DEFAULT_TERRAIN_SEED = "alpha";
const DEFAULT_VIEW_MODE = "normal";
const DEFAULT_SURFACE_MODE = "globe";
const DEFAULT_ERA_SCALE = "crust";
const PLATE_HOVER_POPUP_DELAY_MS = 450;
const WORLD_SUBSYSTEM_KEYS = Object.freeze([
    "terrain",
    "river",
    "climate",
    "ecology",
    "civilization",
]);
const ERA_SCALE_PRESETS = Object.freeze({
    crust: {
        label: "地殻形成期",
        tickLabel: "100万年",
        runtimeTickMs: 220,
        weights: { terrain: 1.0, river: 0.05, climate: 0.0, ecology: 0.0, civilization: 0.0 },
    },
    environment: {
        label: "環境形成期",
        tickLabel: "1万年",
        runtimeTickMs: 150,
        weights: { terrain: 0.3, river: 1.0, climate: 0.9, ecology: 0.15, civilization: 0.0 },
    },
    life: {
        label: "生命誕生期",
        tickLabel: "1000年",
        runtimeTickMs: 110,
        weights: { terrain: 0.15, river: 0.5, climate: 0.6, ecology: 1.0, civilization: 0.05 },
    },
    civilization: {
        label: "文明成立期",
        tickLabel: "100年",
        runtimeTickMs: 90,
        weights: { terrain: 0.08, river: 0.3, climate: 0.45, ecology: 0.5, civilization: 1.0 },
    },
    history: {
        label: "歴史展開期",
        tickLabel: "1年",
        runtimeTickMs: 70,
        weights: { terrain: 0.02, river: 0.08, climate: 0.12, ecology: 0.1, civilization: 1.0 },
    },
});

async function bootstrap() {
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
    let camera = globeCamera;
    let activeControls = globeControls;
    globeControls.enabled = true;
    mapControls.enabled = false;

    let generationToken = 0;
    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentSurfaceMode = DEFAULT_SURFACE_MODE;
    let currentTerrainData = null;
    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();
    const hoverLocalPoint = new THREE.Vector3();
    const hoverTriA = new THREE.Vector3();
    const hoverTriB = new THREE.Vector3();
    const hoverTriC = new THREE.Vector3();
    const hoverBarycoord = new THREE.Vector3();
    let plateHoverTimerId = null;
    let pendingPlateHover = null;
    let visiblePlateHoverId = null;
    let debugEnabled = debugToggleInput.checked;
    let currentRiverMaskTexture = null;
    let currentEraScale = DEFAULT_ERA_SCALE;
    const worldState = {
        tick: 0,
        isRunning: true,
        accumulatorMs: 0,
        lastFrameTimeMs: null,
        runtimeTickMs: getEraScalePresetRuntimeTickMs(DEFAULT_ERA_SCALE),
        maxTicksPerFrame: 6,
        erosionAutomatonState: null,
        carry: {
            terrain: 0,
            river: 0,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        executedSteps: {
            terrain: 0,
            river: 0,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
    };

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

    function updateGeometryPositions() {
        if (!currentTerrainData) {
            return;
        }
        const positions = buildRenderPositions(
            basePositions,
            currentTerrainData.heightData,
            currentSurfaceMode,
        );
        geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
        geometry.computeVertexNormals();
        geometry.computeBoundingSphere();
    }

    function fitCameraToCurrentSurface() {
        if (currentSurfaceMode === "map") {
            camera = mapCamera;
            mapCamera.position.set(0, 0, 5);
            mapCamera.up.set(0, 1, 0);
            mapCamera.lookAt(0, 0, 0);
            mapControls.target.set(0, 0, 0);
            mapControls.update();
            activeControls = mapControls;
            globeControls.enabled = false;
            mapControls.enabled = true;
            mapControls.enablePan = true;
            sphere.visible = true;
            wireframe.visible = false;
            halo.visible = false;
            return;
        }

        camera = globeCamera;
        globeCamera.position.set(0, 0, 2.7);
        globeCamera.up.set(0, 1, 0);
        globeControls.target.set(0, 0, 0);
        activeControls = globeControls;
        globeControls.enabled = true;
        mapControls.enabled = false;
        sphere.visible = true;
        wireframe.visible = debugEnabled;
        halo.visible = true;
        globeControls.update();
    }

    function setSurfaceMode(nextMode) {
        const normalizedMode = nextMode === "map" ? "map" : "globe";
        if (currentSurfaceMode === normalizedMode && currentTerrainData) {
            return;
        }
        currentSurfaceMode = normalizedMode;
        updateGeometryPositions();
        fitCameraToCurrentSurface();
        hidePlateHoverPopup();
    }

    function clearPlateHoverTimer() {
        if (plateHoverTimerId !== null) {
            window.clearTimeout(plateHoverTimerId);
            plateHoverTimerId = null;
        }
    }

    function hidePlateHoverPopup() {
        clearPlateHoverTimer();
        pendingPlateHover = null;
        visiblePlateHoverId = null;
        plateHoverPopup.hidden = true;
        plateHoverPopup.textContent = "";
    }

    function showPlateHoverPopup(clientX, clientY, plateIdValue, hoverDiagnostics) {
        if (!currentTerrainData || currentViewMode !== "plates") {
            hidePlateHoverPopup();
            return;
        }

        const plateIndex = Number(plateIdValue);
        const { plateInfo } = currentTerrainData;
        if (
            !Number.isInteger(plateIndex) ||
            plateIndex < 0 ||
            plateIndex >= plateInfo.isOcean.length
        ) {
            hidePlateHoverPopup();
            return;
        }

        const plateKind = plateInfo.isOcean[plateIndex] ? "海洋プレート" : "大陸プレート";
        const weight = Number.isFinite(hoverDiagnostics?.weight)
            ? hoverDiagnostics.weight
            : plateInfo.baseWeight[plateIndex];
        const height = plateInfo.baseHeight[plateIndex];
        const debugLines = debugEnabled ? (hoverDiagnostics?.debugLines ?? []) : [];
        plateHoverPopup.textContent = [
            `Plate #${plateIndex}`,
            plateKind,
            `weight: ${weight.toFixed(3)}`,
            `height: ${height.toFixed(3)}`,
            ...debugLines,
        ].join("\n");
        plateHoverPopup.hidden = false;

        const viewportRect = viewportPanel.getBoundingClientRect();
        const margin = 10;
        const offset = 14;
        const maxLeft = Math.max(
            margin,
            viewportRect.width - plateHoverPopup.offsetWidth - margin,
        );
        const maxTop = Math.max(
            margin,
            viewportRect.height - plateHoverPopup.offsetHeight - margin,
        );
        const left = Math.min(Math.max(clientX - viewportRect.left + offset, margin), maxLeft);
        const top = Math.min(Math.max(clientY - viewportRect.top + offset, margin), maxTop);
        plateHoverPopup.style.left = `${left}px`;
        plateHoverPopup.style.top = `${top}px`;
        visiblePlateHoverId = plateIndex;
    }

    function setDebugModeEnabled(nextEnabled) {
        debugEnabled = Boolean(nextEnabled);
        debugToggleInput.checked = debugEnabled;
        wireframe.visible = debugEnabled && currentSurfaceMode === "globe";
        applyTerrainMaterialState();

        if (!plateHoverPopup.hidden && pendingPlateHover) {
            showPlateHoverPopup(
                pendingPlateHover.clientX,
                pendingPlateHover.clientY,
                pendingPlateHover.plateId,
                pendingPlateHover.hoverDiagnostics,
            );
            return;
        }

        if (!plateHoverPopup.hidden) {
            hidePlateHoverPopup();
        }
    }

    function schedulePlateHoverPopup(clientX, clientY, plateIdValue, hoverDiagnostics) {
        const plateIndex = Number(plateIdValue);
        if (!Number.isInteger(plateIndex)) {
            hidePlateHoverPopup();
            return;
        }

        if (visiblePlateHoverId === plateIndex && !plateHoverPopup.hidden) {
            showPlateHoverPopup(clientX, clientY, plateIndex, hoverDiagnostics);
            return;
        }

        pendingPlateHover = {
            clientX,
            clientY,
            plateId: plateIndex,
            hoverDiagnostics,
        };

        if (plateHoverTimerId !== null) {
            return;
        }

        plateHoverTimerId = window.setTimeout(() => {
            plateHoverTimerId = null;
            if (!pendingPlateHover) {
                return;
            }
            const {
                clientX: nextX,
                clientY: nextY,
                plateId: nextPlateId,
                hoverDiagnostics: nextHoverDiagnostics,
            } = pendingPlateHover;
            pendingPlateHover = null;
            showPlateHoverPopup(nextX, nextY, nextPlateId, nextHoverDiagnostics);
        }, PLATE_HOVER_POPUP_DELAY_MS);
    }

    function sampleHoverWeight(hit, plateIndexFallback) {
        const face = hit?.face;
        const positionAttr = geometry.getAttribute("position");
        if (
            !face ||
            !positionAttr ||
            !currentTerrainData
        ) {
            return {
                weight: null,
                source: "invalid-hit",
                debugLines: ["debug: source=invalid-hit"],
            };
        }

        hoverTriA.fromBufferAttribute(positionAttr, face.a);
        hoverTriB.fromBufferAttribute(positionAttr, face.b);
        hoverTriC.fromBufferAttribute(positionAttr, face.c);
        hoverLocalPoint.copy(hit.point);
        sphere.worldToLocal(hoverLocalPoint);
        const bary = THREE.Triangle.getBarycoord(
            hoverLocalPoint,
            hoverTriA,
            hoverTriB,
            hoverTriC,
            hoverBarycoord,
        );

        const weightA = currentTerrainData.vertexWeight[face.a];
        const weightB = currentTerrainData.vertexWeight[face.b];
        const weightC = currentTerrainData.vertexWeight[face.c];
        const plateA = currentTerrainData.plateId[face.a];
        const plateB = currentTerrainData.plateId[face.b];
        const plateC = currentTerrainData.plateId[face.c];
        const fallbackVertexWeight = weightA;

        const baseDebugLines = [
            `debug: face=(${face.a},${face.b},${face.c})`,
            `debug: vw=(${Number(weightA).toFixed(3)},${Number(weightB).toFixed(3)},${Number(weightC).toFixed(3)})`,
            `debug: pid=(${Number(plateA)},${Number(plateB)},${Number(plateC)})`,
        ];

        if (
            !bary ||
            !Number.isFinite(hoverBarycoord.x) ||
            !Number.isFinite(hoverBarycoord.y) ||
            !Number.isFinite(hoverBarycoord.z)
        ) {
            if (Number.isFinite(weightA)) {
                return {
                    weight: weightA,
                    source: "vertex-fallback-a",
                    debugLines: [
                        ...baseDebugLines,
                        "debug: source=vertex-fallback-a",
                        "debug: bary=invalid",
                    ],
                };
            }
            if (Number.isFinite(weightB)) {
                return {
                    weight: weightB,
                    source: "vertex-fallback-b",
                    debugLines: [
                        ...baseDebugLines,
                        "debug: source=vertex-fallback-b",
                        "debug: bary=invalid",
                    ],
                };
            }
            if (Number.isFinite(weightC)) {
                return {
                    weight: weightC,
                    source: "vertex-fallback-c",
                    debugLines: [
                        ...baseDebugLines,
                        "debug: source=vertex-fallback-c",
                        "debug: bary=invalid",
                    ],
                };
            }
            return {
                weight: null,
                source: "weight-invalid",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=weight-invalid",
                    "debug: bary=invalid",
                ],
            };
        }

        const targetPlate = Number.isInteger(plateIndexFallback)
            ? plateIndexFallback
            : Number(plateA);
        const samePlateA = Number(plateA) === targetPlate;
        const samePlateB = Number(plateB) === targetPlate;
        const samePlateC = Number(plateC) === targetPlate;

        if (samePlateA && samePlateB && samePlateC) {
            return {
                weight:
                    hoverBarycoord.x * weightA +
                    hoverBarycoord.y * weightB +
                    hoverBarycoord.z * weightC,
                source: "interp-all",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=interp-all",
                    `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                ],
            };
        }

        let sum = 0;
        let wsum = 0;
        if (samePlateA && Number.isFinite(weightA)) {
            sum += hoverBarycoord.x * weightA;
            wsum += hoverBarycoord.x;
        }
        if (samePlateB && Number.isFinite(weightB)) {
            sum += hoverBarycoord.y * weightB;
            wsum += hoverBarycoord.y;
        }
        if (samePlateC && Number.isFinite(weightC)) {
            sum += hoverBarycoord.z * weightC;
            wsum += hoverBarycoord.z;
        }
        if (wsum > 1e-6) {
            return {
                weight: sum / wsum,
                source: "interp-same-plate-only",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=interp-same-plate-only",
                    `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                    `debug: wsum=${wsum.toFixed(3)}`,
                ],
            };
        }

        if (Number.isFinite(fallbackVertexWeight)) {
            return {
                weight: fallbackVertexWeight,
                source: "vertex-fallback-final",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=vertex-fallback-final",
                    `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                    `debug: wsum=${wsum.toFixed(3)}`,
                ],
            };
        }

        return {
            weight: null,
            source: "plate-fallback",
            debugLines: [
                ...baseDebugLines,
                "debug: source=plate-fallback",
                `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                `debug: wsum=${wsum.toFixed(3)}`,
            ],
        };
    }

    function updatePlateHoverFromPointer(event) {
        if (!currentTerrainData || currentViewMode !== "plates" || currentSurfaceMode !== "globe") {
            hidePlateHoverPopup();
            return;
        }

        const rect = canvas.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) {
            hidePlateHoverPopup();
            return;
        }

        pointerNdc.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
        pointerNdc.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
        raycaster.setFromCamera(pointerNdc, camera);

        const [hit] = raycaster.intersectObject(sphere, false);
        const face = hit?.face;
        if (!face) {
            hidePlateHoverPopup();
            return;
        }

        const hoveredVertexIndex = face.a;
        const hoveredPlateId = currentTerrainData.plateId[hoveredVertexIndex];
        const hoveredPlateIndex = Number(hoveredPlateId);
        if (!Number.isInteger(hoveredPlateIndex)) {
            hidePlateHoverPopup();
            return;
        }

        if (pendingPlateHover && pendingPlateHover.plateId !== hoveredPlateIndex) {
            clearPlateHoverTimer();
            pendingPlateHover = null;
            plateHoverPopup.hidden = true;
            plateHoverPopup.textContent = "";
            visiblePlateHoverId = null;
        }

        const sampledWeightResult = sampleHoverWeight(hit, hoveredPlateIndex);
        const sampledWeight = Number.isFinite(sampledWeightResult?.weight)
            ? sampledWeightResult.weight
            : currentTerrainData.vertexWeight[hoveredVertexIndex];
        const hoverDiagnostics = {
            weight: sampledWeight,
            debugLines: [
                ...(sampledWeightResult?.debugLines ?? ["debug: source=unknown"]),
                `debug: faceAWeight=${currentTerrainData.vertexWeight[hoveredVertexIndex].toFixed(3)}`,
            ],
        };

        pendingPlateHover = {
            clientX: event.clientX,
            clientY: event.clientY,
            plateId: hoveredPlateIndex,
            hoverDiagnostics,
        };
        schedulePlateHoverPopup(
            event.clientX,
            event.clientY,
            hoveredPlateIndex,
            hoverDiagnostics,
        );
    }

    function updateTerrainAttributes() {
        if (!currentTerrainData) {
            return;
        }
        geometry.setAttribute("terrainHeight", new THREE.BufferAttribute(currentTerrainData.heightData, 1));
        geometry.setAttribute(
            "terrainRiverFlux",
            new THREE.BufferAttribute(currentTerrainData.riverFlux, 1),
        );
        geometry.setAttribute(
            "terrainPlateId",
            new THREE.BufferAttribute(Float32Array.from(currentTerrainData.plateId), 1),
        );
        geometry.setAttribute("terrainLakeDepth", new THREE.BufferAttribute(currentTerrainData.lakeDepth, 1));
        geometry.setAttribute(
            "terrainDebugTrench",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.trench, 1),
        );
        geometry.setAttribute(
            "terrainDebugArc",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.arc, 1),
        );
        geometry.setAttribute(
            "terrainDebugBackarc",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.backarc, 1),
        );
        geometry.setAttribute(
            "terrainDebugOceanOceanArc",
            new THREE.BufferAttribute(currentTerrainData.tectonicDebug.oceanOceanArc, 1),
        );
    }

    function updateRiverMaskTexture() {
        if (!currentTerrainData) {
            return;
        }
        const nextTexture = buildRiverMaskTexture(
            basePositions,
            currentTerrainData.riverNext,
            currentTerrainData.riverFlux,
        );
        if (currentRiverMaskTexture) {
            currentRiverMaskTexture.dispose();
        }
        currentRiverMaskTexture = nextTexture;
        terrainMaterial.setRiverMaskTexture(nextTexture);
    }

    function applyTerrainMaterialState() {
        terrainMaterial.setViewMode(currentViewMode);
        terrainMaterial.setDebugEnabled(debugEnabled);
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
        worldState.tick = 0;
        worldState.accumulatorMs = 0;
        worldState.lastFrameTimeMs = null;
        for (const key of WORLD_SUBSYSTEM_KEYS) {
            worldState.carry[key] = 0;
            worldState.executedSteps[key] = 0;
        }
    }

    function computeRiverAsyncBudgetCells(riverWeight) {
        if (!currentTerrainData || !Number.isFinite(riverWeight) || riverWeight <= 0) {
            return 0;
        }
        const vertexBudgetBase = Math.max(64, Math.floor(currentTerrainData.heightData.length * 0.01));
        return Math.max(1, Math.floor(vertexBudgetBase * riverWeight));
    }

    function applyErosionAutomatonStateToTerrain(erosionState) {
        if (!currentTerrainData || !erosionState) {
            return;
        }

        const nextHeight = new Float32Array(erosionState.height);
        const nextRiverFlux = new Float32Array(erosionState.river_flux);
        const nextRiverNext = new Int32Array(erosionState.river_next);
        if (
            nextHeight.length !== currentTerrainData.heightData.length ||
            nextRiverFlux.length !== currentTerrainData.riverFlux.length ||
            nextRiverNext.length !== currentTerrainData.riverNext.length
        ) {
            return;
        }

        currentTerrainData.heightData = nextHeight;
        currentTerrainData.riverFlux = nextRiverFlux;
        currentTerrainData.riverNext = nextRiverNext;

        updateTerrainAttributes();
        updateRiverMaskTexture();
        updateGeometryPositions();
    }

    function stepRiverAsyncForCurrentTick(preset) {
        if (!worldState.erosionAutomatonState || !preset) {
            return;
        }
        const budgetCells = computeRiverAsyncBudgetCells(preset.weights.river ?? 0);
        if (budgetCells <= 0) {
            return;
        }

        worldState.erosionAutomatonState = step_erosion_automaton(
            worldState.erosionAutomatonState,
            budgetCells,
        );
        worldState.executedSteps.river += 1;
        applyErosionAutomatonStateToTerrain(worldState.erosionAutomatonState);
    }

    function runSubsystemStep(subsystemKey) {
        if (!currentTerrainData) {
            return;
        }
        worldState.executedSteps[subsystemKey] += 1;
        // TODO: 実サブシステム更新をここに接続する。
    }

    function stepWorldTick() {
        if (!currentTerrainData) {
            return;
        }
        const preset = getEraScalePreset(currentEraScale);
        worldState.tick += 1;
        stepRiverAsyncForCurrentTick(preset);

        for (const subsystemKey of WORLD_SUBSYSTEM_KEYS) {
            if (subsystemKey === "river") {
                continue;
            }
            const weight = preset.weights[subsystemKey] ?? 0;
            if (!Number.isFinite(weight) || weight <= 0) {
                continue;
            }
            worldState.carry[subsystemKey] += weight;
            const steps = Math.floor(worldState.carry[subsystemKey]);
            if (steps <= 0) {
                continue;
            }
            worldState.carry[subsystemKey] -= steps;
            for (let i = 0; i < steps; i += 1) {
                runSubsystemStep(subsystemKey);
            }
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
    }

    function setViewMode(nextMode) {
        const normalizedMode = nextMode === "plates" ? "plates" : "normal";
        currentViewMode = normalizedMode;
        for (const input of viewModeInputs) {
            input.checked = input.value === normalizedMode;
        }
        applyTerrainMaterialState();
        if (normalizedMode !== "plates") {
            hidePlateHoverPopup();
        }
    }

    function onResize() {
        resizeViewport(viewportPanel, globeCamera, mapCamera, renderer);
        if (typeof globeControls.handleResize === "function") {
            globeControls.handleResize();
        }
        if (currentSurfaceMode === "map") {
            fitCameraToCurrentSurface();
        }
    }

    async function updateTerrain(seed) {
        const token = ++generationToken;
        const nextSeed = seed.trim() || DEFAULT_TERRAIN_SEED;

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        await new Promise((resolve) => requestAnimationFrame(resolve));
        if (token !== generationToken) {
            return;
        }

        const terrain = generate_terrain(nextSeed, TERRAIN_PARAMS);
        const erosionAutomatonState = init_erosion_automaton(nextSeed, TERRAIN_PARAMS);
        const heightData = new Float32Array(terrain.height);
        const plateId = new Uint32Array(terrain.plate_id);
        const riverFlux = new Float32Array(terrain.river_flux);
        const riverNext = new Int32Array(terrain.river_next);
        const lakeDepth = new Float32Array(terrain.lake_depth ?? heightData.length);
        const plateInfo = {
            isOcean: new Uint8Array(terrain.plate_is_ocean),
            baseHeight: new Float32Array(terrain.plate_base_height),
            baseWeight: new Float32Array(terrain.plate_base_weight),
        };
        const vertexWeight = new Float32Array(terrain.vertex_weight);
        const tectonicDebug = {
            trench: new Float32Array(terrain.debug_trench_strength ?? heightData.length),
            arc: new Float32Array(terrain.debug_arc_strength ?? heightData.length),
            backarc: new Float32Array(terrain.debug_backarc_strength ?? heightData.length),
            oceanOceanArc: new Float32Array(
                terrain.debug_ocean_ocean_arc_strength ?? heightData.length,
            ),
        };
        if (token !== generationToken) {
            return;
        }

        currentTerrainData = {
            heightData,
            plateId,
            riverFlux,
            riverNext,
            lakeDepth,
            plateInfo,
            vertexWeight,
            tectonicDebug,
        };
        worldState.erosionAutomatonState = erosionAutomatonState;
        updateTerrainAttributes();
        updateRiverMaskTexture();
        updateGeometryPositions();
        applyTerrainMaterialState();
        hidePlateHoverPopup();

        const plateCount = Number.isFinite(terrain.plate_count) ? terrain.plate_count : 0;
        const landRatio = Number.isFinite(terrain.land_ratio) ? terrain.land_ratio : 0;

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

    window.addEventListener("resize", onResize);
    if (typeof ResizeObserver !== "undefined") {
        const resizeObserver = new ResizeObserver(() => onResize());
        resizeObserver.observe(viewportPanel);
    }

    sidebarToggle.addEventListener("click", () => {
        const isOpen = sidebarToggle.getAttribute("aria-expanded") === "true";
        setSidebarOpen(!isOpen);
        requestAnimationFrame(onResize);
    });

    canvas.addEventListener("pointermove", (event) => {
        updatePlateHoverFromPointer(event);
    });
    canvas.addEventListener("pointerleave", hidePlateHoverPopup);
    canvas.addEventListener("pointercancel", hidePlateHoverPopup);
    debugToggleInput.addEventListener("change", () => {
        setDebugModeEnabled(debugToggleInput.checked);
    });
    eraScaleSelect.addEventListener("change", () => {
        setEraScale(eraScaleSelect.value);
    });

    for (const input of viewModeInputs) {
        input.addEventListener("change", () => {
            if (!input.checked) {
                return;
            }
            setViewMode(input.value);
        });
    }

    document.addEventListener("keydown", (event) => {
        if (
            event.defaultPrevented ||
            event.metaKey ||
            event.ctrlKey ||
            event.altKey
        ) {
            return;
        }

        const target = event.target;
        if (
            target instanceof HTMLElement &&
            (target.isContentEditable ||
                target instanceof HTMLInputElement ||
                target instanceof HTMLTextAreaElement ||
                target instanceof HTMLSelectElement)
        ) {
            return;
        }

        if (event.key === "1") {
            event.preventDefault();
            setViewMode("normal");
            return;
        }

        if (event.key === "2") {
            event.preventDefault();
            setViewMode("plates");
            return;
        }

        if (event.key.toLowerCase() === "t") {
            event.preventDefault();
            seedInput.focus();
            seedInput.select();
            return;
        }

        if (event.key.toLowerCase() === "d") {
            event.preventDefault();
            setDebugModeEnabled(!debugEnabled);
            return;
        }

        if (event.key.toLowerCase() === "v") {
            event.preventDefault();
            setSurfaceMode(currentSurfaceMode === "globe" ? "map" : "globe");
        }
    });

    seedForm.addEventListener("submit", async (event) => {
        event.preventDefault();
        try {
            await updateTerrain(seedInput.value);
        } catch (error) {
            setStatus(`Generation failed: ${String(error)}`);
            seedInput.removeAttribute("disabled");
            seedForm.querySelector("button")?.removeAttribute("disabled");
            console.error(error);
        }
    });

    await updateTerrain(DEFAULT_TERRAIN_SEED);
    renderEraScaleControls();
    setEraScale(DEFAULT_ERA_SCALE);
    onResize();
    hidePlateHoverPopup();

    function frame(nowMs) {
        advanceWorldLoop(nowMs);
        activeControls.update();
        renderer.render(scene, camera);
        requestAnimationFrame(frame);
    }

    requestAnimationFrame(frame);
}

bootstrap().catch((error) => {
    const statusMessage = document.getElementById("status-message");
    if (statusMessage instanceof HTMLElement) {
        statusMessage.textContent = `Initialization failed: ${String(error)}`;
    }
    console.error(error);
});
