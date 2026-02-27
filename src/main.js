import * as THREE from "three";
import initWasm, {
    CrustTerrainAutomaton,
    WorldTimeController,
    generate_mesh,
    init_erosion_automaton,
    step_erosion_automaton,
} from "./wasm/frey_wasm.js";
import { collectAppElements } from "./app/dom.js";
import { createGlobeScene, resizeViewport } from "./app/scene.js";
import { TERRAIN_LEVEL, TERRAIN_PARAMS } from "./app/terrain-params.js";
import { RUNTIME_PARAMS } from "./app/runtime-params.js";
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
const LAYER_KIND = Object.freeze({
    CLIMATE: "climate",
    ECOLOGY: "ecology",
    CIVILIZATION: "civilization",
});
const ERA_SCALE_PRESETS = Object.freeze({
    crust: {
        label: "地殻形成期",
        tickLabel: "100万年",
        runtimeTickMs: 70,
        weights: { terrain: 4.0, river: 0.25, climate: 0.0, ecology: 0.0, civilization: 0.0 },
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
const SUBSYSTEM_ACTIVITY_SIGNAL_GAIN = Object.freeze({
    terrain: RUNTIME_PARAMS.activity_signal_gain_terrain,
    river: RUNTIME_PARAMS.activity_signal_gain_river,
    climate: RUNTIME_PARAMS.activity_signal_gain_climate,
    ecology: RUNTIME_PARAMS.activity_signal_gain_ecology,
    civilization: RUNTIME_PARAMS.activity_signal_gain_civilization,
});
const SUBSYSTEM_ACTIVITY_STEP_BASELINE = Object.freeze({
    terrain: RUNTIME_PARAMS.activity_step_baseline_terrain,
    river: RUNTIME_PARAMS.activity_step_baseline_river,
    climate: RUNTIME_PARAMS.activity_step_baseline_climate,
    ecology: RUNTIME_PARAMS.activity_step_baseline_ecology,
    civilization: RUNTIME_PARAMS.activity_step_baseline_civilization,
});
const SUBSYSTEM_ACTIVITY_WEIGHT_MIX = RUNTIME_PARAMS.activity_weight_mix;
const SUBSYSTEM_ACTIVITY_QUEUE_PRESSURE_GAIN = RUNTIME_PARAMS.activity_queue_pressure_gain;
const PLATE_MOTION_SPEED_BY_ERA = Object.freeze({
    crust: 0.00045,
    environment: 0.00030,
    life: 0.00020,
    civilization: 0.00014,
    history: 0.00010,
});
const PLATE_MOTION_REMAP_INTERVAL_BY_ERA = Object.freeze({
    crust: 4,
    environment: 7,
    life: 12,
    civilization: 18,
    history: 24,
});
const PLATE_MOTION_ACTIVITY_GAIN = 10.0;
const LAND_RATIO_RECOVERY_BY_ERA = Object.freeze({
    crust: 0.22,
    environment: 0.16,
    life: 0.11,
    civilization: 0.08,
    history: 0.06,
});
const LAND_RATIO_FLOOR_BY_ERA = Object.freeze({
    crust: 0.94,
    environment: 0.90,
    life: 0.86,
    civilization: 0.82,
    history: 0.80,
});
const RIVER_BUDGET_SCALE_BY_ERA = Object.freeze({
    crust: 0.08,
    environment: 0.22,
    life: 0.40,
    civilization: 0.55,
    history: 0.70,
});
const TERRAIN_DYNAMICS_BY_ERA = Object.freeze({
    crust: { diffusion: 0.034, uplift: 0.025, subsidence: 0.011, fluvial: 0.0034, coastline: 0.016 },
    environment: { diffusion: 0.021, uplift: 0.013, subsidence: 0.0067, fluvial: 0.0039, coastline: 0.012 },
    life: { diffusion: 0.013, uplift: 0.007, subsidence: 0.0042, fluvial: 0.0035, coastline: 0.0085 },
    civilization: { diffusion: 0.009, uplift: 0.0040, subsidence: 0.0027, fluvial: 0.0029, coastline: 0.0065 },
    history: { diffusion: 0.0065, uplift: 0.0028, subsidence: 0.0018, fluvial: 0.0024, coastline: 0.0056 },
});
const DEBUG_SNAPSHOT_TICKS = Object.freeze([300]);
const DEBUG_SNAPSHOT_TOPK_LIMIT = 128;
const TERRAIN_HEIGHT_CLAMP = 1.2;

function createEmptyCore() {
    return null;
}

function createEmptyLayers() {
    return {
        [LAYER_KIND.CLIMATE]: null,
        [LAYER_KIND.ECOLOGY]: null,
        [LAYER_KIND.CIVILIZATION]: null,
    };
}

function createInitialBudgets() {
    return {
        terrain: 0,
        river: 0,
        climate: 0,
        ecology: 0,
        civilization: 0,
    };
}

function createInitialRuntimeState(defaultRuntimeTickMs) {
    return {
        isRunning: true,
        accumulatorMs: 0,
        lastFrameTimeMs: null,
        runtimeTickMs: defaultRuntimeTickMs,
        maxTicksPerFrame: 20,
        maxRiverStepsPerFrame: 4,
        erosionAutomatonState: null,
        pendingRiverSteps: 0,
        terrainErosionDirty: false,
        terrainCoreDirty: false,
        latestActivity: {
            terrain: 0,
            river: 1,
            climate: 1,
            ecology: 1,
            civilization: 1,
        },
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
}

function fnv1a32(text) {
    let hash = 0x811c9dc5;
    for (let i = 0; i < text.length; i += 1) {
        hash ^= text.charCodeAt(i);
        hash = Math.imul(hash, 0x01000193);
    }
    return hash >>> 0;
}

function hash01(seed, salt) {
    const h = fnv1a32(`${seed}:${salt}`);
    return (h & 0x00ffffff) / 0x01000000;
}

function normalizeVec3(x, y, z) {
    const len = Math.hypot(x, y, z);
    if (!Number.isFinite(len) || len <= 1e-8) {
        return [0, 0, 1];
    }
    return [x / len, y / len, z / len];
}

function dotVec3(ax, ay, az, bx, by, bz) {
    return ax * bx + ay * by + az * bz;
}

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

    function createPlateMotionState(seed) {
        if (!currentTerrainData?.plateId || !currentTerrainData?.plateInfo?.isOcean) {
            return null;
        }
        const plateId = currentTerrainData.plateId;
        const plateCount = currentTerrainData.plateInfo.isOcean.length;
        if (!Number.isInteger(plateCount) || plateCount <= 0) {
            return null;
        }

        const sumX = new Float64Array(plateCount);
        const sumY = new Float64Array(plateCount);
        const sumZ = new Float64Array(plateCount);
        const counts = new Uint32Array(plateCount);
        for (let i = 0; i < plateId.length; i += 1) {
            const pid = plateId[i];
            if (!Number.isInteger(pid) || pid < 0 || pid >= plateCount) {
                continue;
            }
            const base = i * 3;
            sumX[pid] += basePositions[base] ?? 0;
            sumY[pid] += basePositions[base + 1] ?? 0;
            sumZ[pid] += basePositions[base + 2] ?? 0;
            counts[pid] += 1;
        }

        const centroids = new Float32Array(plateCount * 3);
        const velocities = new Float32Array(plateCount * 3);
        for (let pid = 0; pid < plateCount; pid += 1) {
            let cx = sumX[pid];
            let cy = sumY[pid];
            let cz = sumZ[pid];
            if (counts[pid] <= 0) {
                const rx = hash01(seed, `plate-rx-${pid}`) * 2 - 1;
                const ry = hash01(seed, `plate-ry-${pid}`) * 2 - 1;
                const rz = hash01(seed, `plate-rz-${pid}`) * 2 - 1;
                [cx, cy, cz] = normalizeVec3(rx, ry, rz);
            } else {
                [cx, cy, cz] = normalizeVec3(cx, cy, cz);
            }

            centroids[pid * 3] = cx;
            centroids[pid * 3 + 1] = cy;
            centroids[pid * 3 + 2] = cz;

            let ax = hash01(seed, `axis-x-${pid}`) * 2 - 1;
            let ay = hash01(seed, `axis-y-${pid}`) * 2 - 1;
            let az = hash01(seed, `axis-z-${pid}`) * 2 - 1;
            [ax, ay, az] = normalizeVec3(ax, ay, az);
            const proj = dotVec3(ax, ay, az, cx, cy, cz);
            let tx = ax - cx * proj;
            let ty = ay - cy * proj;
            let tz = az - cz * proj;
            if (Math.hypot(tx, ty, tz) <= 1e-6) {
                const fallback = Math.abs(cy) < 0.9 ? [0, 1, 0] : [1, 0, 0];
                const projFallback = dotVec3(fallback[0], fallback[1], fallback[2], cx, cy, cz);
                tx = fallback[0] - cx * projFallback;
                ty = fallback[1] - cy * projFallback;
                tz = fallback[2] - cz * projFallback;
            }
            [tx, ty, tz] = normalizeVec3(tx, ty, tz);
            const isOcean = currentTerrainData.plateInfo.isOcean[pid] > 0;
            const speedJitter = 0.8 + hash01(seed, `speed-${pid}`) * 0.6;
            const speedScale = speedJitter * (isOcean ? 1.15 : 0.9);
            velocities[pid * 3] = tx * speedScale;
            velocities[pid * 3 + 1] = ty * speedScale;
            velocities[pid * 3 + 2] = tz * speedScale;
        }

        return {
            centroids,
            velocities,
            remapCarry: 0,
        };
    }

    function remapPlateIdsFromMotion() {
        if (!plateMotionState || !currentTerrainData?.plateId) {
            return 0;
        }
        const centroids = plateMotionState.centroids;
        const plateCount = centroids.length / 3;
        const plateId = currentTerrainData.plateId;
        const vertexWeight = currentTerrainData.vertexWeight;
        const cellCount = plateId.length;
        let changedCount = 0;
        for (let i = 0; i < cellCount; i += 1) {
            const base = i * 3;
            const vx = basePositions[base] ?? 0;
            const vy = basePositions[base + 1] ?? 0;
            const vz = basePositions[base + 2] ?? 1;
            let bestPid = 0;
            let bestDot = -Infinity;
            let secondDot = -Infinity;
            for (let pid = 0; pid < plateCount; pid += 1) {
                const c = pid * 3;
                const score = dotVec3(vx, vy, vz, centroids[c], centroids[c + 1], centroids[c + 2]);
                if (score > bestDot) {
                    secondDot = bestDot;
                    bestDot = score;
                    bestPid = pid;
                    continue;
                }
                if (score > secondDot) {
                    secondDot = score;
                }
            }
            if (plateId[i] !== bestPid) {
                plateId[i] = bestPid;
                changedCount += 1;
            }
            if (vertexWeight && i < vertexWeight.length) {
                const confidence = Math.min(1, Math.max(0, (bestDot - secondDot) * 8));
                vertexWeight[i] = confidence;
            }
        }
        return changedCount;
    }

    function updatePlateMotionStep() {
        if (!plateMotionState) {
            return 0;
        }
        const speed = PLATE_MOTION_SPEED_BY_ERA[currentEraScale] ?? PLATE_MOTION_SPEED_BY_ERA.crust;
        const remapInterval =
            PLATE_MOTION_REMAP_INTERVAL_BY_ERA[currentEraScale] ??
            PLATE_MOTION_REMAP_INTERVAL_BY_ERA.crust;
        if (!Number.isFinite(speed) || speed <= 0 || remapInterval <= 0) {
            return 0;
        }

        const centroids = plateMotionState.centroids;
        const velocities = plateMotionState.velocities;
        for (let pid = 0; pid < centroids.length / 3; pid += 1) {
            const c = pid * 3;
            const cx = centroids[c];
            const cy = centroids[c + 1];
            const cz = centroids[c + 2];
            const vx = velocities[c];
            const vy = velocities[c + 1];
            const vz = velocities[c + 2];
            let nx = cx + vx * speed;
            let ny = cy + vy * speed;
            let nz = cz + vz * speed;
            [nx, ny, nz] = normalizeVec3(nx, ny, nz);
            centroids[c] = nx;
            centroids[c + 1] = ny;
            centroids[c + 2] = nz;
        }

        plateMotionState.remapCarry += 1;
        if (plateMotionState.remapCarry < remapInterval) {
            return 0;
        }
        plateMotionState.remapCarry = 0;
        return remapPlateIdsFromMotion();
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
    }

    function createClimateLayer(cellCount) {
        return {
            temp: new Float32Array(cellCount),
            rain: new Float32Array(cellCount),
        };
    }

    function createEcologyLayer(cellCount) {
        return {
            habitability: new Float32Array(cellCount),
            productivity: new Float32Array(cellCount),
        };
    }

    function createCivilizationLayer(cellCount) {
        return {
            population: new Float32Array(cellCount),
            stateId: new Uint32Array(cellCount),
        };
    }

    function getRequiredLayerKindsForEra(eraKey) {
        switch (eraKey) {
            case "history":
                return [LAYER_KIND.CLIMATE, LAYER_KIND.ECOLOGY, LAYER_KIND.CIVILIZATION];
            case "civilization":
                return [LAYER_KIND.CLIMATE, LAYER_KIND.ECOLOGY, LAYER_KIND.CIVILIZATION];
            case "life":
                return [LAYER_KIND.CLIMATE, LAYER_KIND.ECOLOGY];
            case "environment":
                return [LAYER_KIND.CLIMATE];
            case "crust":
            default:
                return [];
        }
    }

    function ensureRequiredLayers(nextWorld) {
        if (!nextWorld.core?.heightData) {
            return;
        }
        const cellCount = nextWorld.core.heightData.length;
        const requiredKinds = getRequiredLayerKindsForEra(nextWorld.era);
        for (const layerKind of requiredKinds) {
            if (nextWorld.layers[layerKind]) {
                continue;
            }
            if (layerKind === LAYER_KIND.CLIMATE) {
                nextWorld.layers[layerKind] = createClimateLayer(cellCount);
                continue;
            }
            if (layerKind === LAYER_KIND.ECOLOGY) {
                nextWorld.layers[layerKind] = createEcologyLayer(cellCount);
                continue;
            }
            if (layerKind === LAYER_KIND.CIVILIZATION) {
                nextWorld.layers[layerKind] = createCivilizationLayer(cellCount);
            }
        }
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

    function computeRiverBudgetCells(riverWeight) {
        if (!currentTerrainData || !Number.isFinite(riverWeight) || riverWeight <= 0) {
            return 0;
        }
        const scale = RIVER_BUDGET_SCALE_BY_ERA[currentEraScale] ?? RIVER_BUDGET_SCALE_BY_ERA.crust;
        const vertexBudgetBase = Math.max(64, Math.floor(currentTerrainData.heightData.length * 0.01));
        return Math.max(1, Math.floor(vertexBudgetBase * riverWeight * scale));
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

        const landPreserveDelta = applyLandRatioFloor(
            nextHeight,
            currentTerrainData.plateId,
            currentTerrainData.plateInfo?.isOcean,
            currentTerrainData.targetLandRatio,
        );
        currentTerrainData.heightData = nextHeight;
        currentTerrainData.riverFlux = nextRiverFlux;
        currentTerrainData.riverNext = nextRiverNext;
        if (landPreserveDelta > 0) {
            syncTerrainHeightToErosionState();
        }

        updateTerrainAttributes();
        updateRiverMaskTexture();
        updateGeometryPositions();
    }

    function estimateRiverActivitySignal(erosionState) {
        if (!erosionState || !currentTerrainData) {
            return 0;
        }
        const changedCount = Array.isArray(erosionState.recent_changed)
            ? erosionState.recent_changed.length
            : 0;
        const cellCount = Math.max(1, currentTerrainData.heightData?.length ?? 1);
        return Math.min(1, changedCount / cellCount);
    }

    function runTerrainStep(steps) {
        if (!Number.isFinite(steps) || steps <= 0) {
            return;
        }
        for (let i = 0; i < steps; i += 1) {
            runSubsystemStep("terrain");
        }
    }

    function stepRiverForCurrentTick(preset) {
        if (!worldState.erosionAutomatonState || !preset) {
            return;
        }
        const budgetCells = computeRiverBudgetCells(preset.weights.river ?? 0);
        if (budgetCells <= 0) {
            return;
        }

        worldState.erosionAutomatonState = step_erosion_automaton(
            worldState.erosionAutomatonState,
            budgetCells,
        );
        worldState.executedSteps.river += 1;
        recordSubsystemActivity(
            "river",
            estimateRiverActivitySignal(worldState.erosionAutomatonState) *
                SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.river,
        );
        worldState.terrainErosionDirty = true;
    }

    function enqueueRiverStep(steps) {
        if (!Number.isFinite(steps) || steps <= 0) {
            return;
        }
        worldState.pendingRiverSteps += steps;
    }

    function drainRiverQueue(preset) {
        if (!preset || worldState.pendingRiverSteps <= 0) {
            return;
        }

        const maxRiverStepsPerFrame = Math.max(1, worldState.maxRiverStepsPerFrame ?? 1);
        let drained = 0;
        while (worldState.pendingRiverSteps > 0 && drained < maxRiverStepsPerFrame) {
            stepRiverForCurrentTick(preset);
            worldState.pendingRiverSteps -= 1;
            drained += 1;
        }

        if (worldState.terrainErosionDirty) {
            applyErosionAutomatonStateToTerrain(worldState.erosionAutomatonState);
            worldState.terrainErosionDirty = false;
        }
    }

    function clamp01(value) {
        if (!Number.isFinite(value)) {
            return 0;
        }
        return Math.min(1, Math.max(0, value));
    }

    function recordSubsystemActivity(subsystemKey, signal) {
        const normalized = clamp01(signal);
        const prev = clamp01(worldState.latestActivity[subsystemKey] ?? 0);
        worldState.latestActivity[subsystemKey] = clamp01(prev + normalized * (1 - prev));
    }

    function buildObservedActivityForTick(subsystemKey, preset) {
        const raw = clamp01(worldState.latestActivity[subsystemKey] ?? 0);
        const steps = Math.max(0, world.budgets?.[subsystemKey] ?? 0);
        const stepBaseline = clamp01((SUBSYSTEM_ACTIVITY_STEP_BASELINE[subsystemKey] ?? 0) * steps);
        const weight = clamp01(preset?.weights?.[subsystemKey] ?? 0);
        const weightFactor = 1 - SUBSYSTEM_ACTIVITY_WEIGHT_MIX + weight * SUBSYSTEM_ACTIVITY_WEIGHT_MIX;
        return clamp01(Math.max(raw, stepBaseline) * weightFactor);
    }

    function applyLandRatioFloor(heightData, plateId, plateIsOcean, targetLandRatio) {
        if (!heightData || !plateId || !plateIsOcean || !Number.isFinite(targetLandRatio) || targetLandRatio <= 0) {
            return 0;
        }
        const cellCount = Math.min(heightData.length, plateId.length);
        if (cellCount <= 0) {
            return 0;
        }

        let landCount = 0;
        for (let i = 0; i < cellCount; i += 1) {
            if (heightData[i] > 0) {
                landCount += 1;
            }
        }
        const currentLandRatio = landCount / Math.max(1, cellCount);
        const floorScale = LAND_RATIO_FLOOR_BY_ERA[currentEraScale] ?? LAND_RATIO_FLOOR_BY_ERA.crust;
        const floorLandRatio = targetLandRatio * floorScale;
        const landDeficit = Math.max(0, floorLandRatio - currentLandRatio);
        if (landDeficit <= 0) {
            return 0;
        }

        const recoveryGain = LAND_RATIO_RECOVERY_BY_ERA[currentEraScale] ?? LAND_RATIO_RECOVERY_BY_ERA.crust;
        let deltaAbs = 0;
        for (let i = 0; i < cellCount; i += 1) {
            const pid = plateId[i];
            if (!Number.isInteger(pid) || pid < 0 || pid >= plateIsOcean.length || plateIsOcean[pid] > 0) {
                continue;
            }
            const h = heightData[i];
            const coastalBoost = Math.max(0, 1 - Math.min(1, Math.abs(h) / 0.30));
            const uplift = landDeficit * recoveryGain * (0.30 + coastalBoost);
            if (uplift <= 0) {
                continue;
            }
            const raised = Math.min(TERRAIN_HEIGHT_CLAMP, h + uplift);
            const changed = raised - h;
            if (Math.abs(changed) < 1e-8) {
                continue;
            }
            heightData[i] = raised;
            deltaAbs += Math.abs(changed);
        }
        return deltaAbs;
    }

    function syncTerrainHeightToErosionState() {
        const erosionState = worldState.erosionAutomatonState;
        const heightData = currentTerrainData?.heightData;
        if (!erosionState || !heightData) {
            return;
        }
        const stateHeight = erosionState.height;
        if (!stateHeight || stateHeight.length !== heightData.length) {
            return;
        }
        if (Array.isArray(stateHeight) || ArrayBuffer.isView(stateHeight)) {
            for (let i = 0; i < heightData.length; i += 1) {
                stateHeight[i] = heightData[i];
            }
            return;
        }
        erosionState.height = Array.from(heightData);
    }

    function updateTerrainCoreStep() {
        const heightData = currentTerrainData?.heightData;
        const plateId = currentTerrainData?.plateId;
        const plateIsOcean = currentTerrainData?.plateInfo?.isOcean;
        const riverFlux = currentTerrainData?.riverFlux;
        const targetLandRatio = currentTerrainData?.targetLandRatio;
        const erosionNbrOffsets = worldState.erosionAutomatonState?.nbr_offsets;
        const erosionNbrs = worldState.erosionAutomatonState?.nbrs;
        const nbrOffsets = world.mesh?.nbrOffsets ?? erosionNbrOffsets ?? null;
        const nbrs = world.mesh?.nbrs ?? erosionNbrs ?? null;
        if (!heightData || !plateId || !riverFlux) {
            return;
        }
        if (!nbrOffsets || !nbrs) {
            terrainSkipNoNeighbors += 1;
            return;
        }
        const cellCount = heightData.length;
        if (
            cellCount <= 0 ||
            plateId.length < cellCount ||
            riverFlux.length < cellCount ||
            nbrOffsets.length !== cellCount + 1
        ) {
            return;
        }

        if (!plateMotionState) {
            plateMotionState = createPlateMotionState(currentSeed);
            if (!plateMotionState) {
                terrainSkipNoPlateMotion += 1;
            }
        }
        const movedVertices = updatePlateMotionStep();
        const dynamics = TERRAIN_DYNAMICS_BY_ERA[currentEraScale] ?? TERRAIN_DYNAMICS_BY_ERA.crust;
        const nextHeight = new Float32Array(heightData);
        let deltaAbsSum = 0;
        for (let i = 0; i < cellCount; i += 1) {
            const start = nbrOffsets[i] ?? 0;
            const end = nbrOffsets[i + 1] ?? start;
            if (end <= start) {
                continue;
            }

            const current = heightData[i];
            let nbrCount = 0;
            let nbrHeightSum = 0;
            let boundaryCount = 0;
            let shorelineEdgeCount = 0;
            const currentPlate = plateId[i];
            for (let cursor = start; cursor < end; cursor += 1) {
                const n = nbrs[cursor];
                if (!Number.isInteger(n) || n < 0 || n >= cellCount) {
                    continue;
                }
                nbrCount += 1;
                nbrHeightSum += heightData[n];
                if (plateId[n] !== currentPlate) {
                    boundaryCount += 1;
                }
                const isCurrentLand = current > 0;
                const isNeighborLand = heightData[n] > 0;
                if (isCurrentLand !== isNeighborLand) {
                    shorelineEdgeCount += 1;
                }
            }
            if (nbrCount <= 0) {
                continue;
            }

            const meanNbrHeight = nbrHeightSum / nbrCount;
            const boundaryRatio = boundaryCount / nbrCount;
            const shorelineRatio = shorelineEdgeCount / nbrCount;
            const flux = Math.max(0, riverFlux[i]);
            const isOceanPlate =
                !!plateIsOcean &&
                currentPlate >= 0 &&
                currentPlate < plateIsOcean.length &&
                plateIsOcean[currentPlate] > 0;
            const fluvialScale = isOceanPlate ? 0.22 : 1.0;
            const fluvialErode =
                Math.log1p(flux) * dynamics.fluvial * fluvialScale * Math.max(0, current + 0.08);
            const buoyancyDelta = isOceanPlate
                ? -dynamics.subsidence * 0.04
                : dynamics.uplift * 0.18;
            const tectonicDelta =
                (boundaryRatio * dynamics.uplift - (1 - boundaryRatio) * dynamics.subsidence * 0.35) *
                (current > 0 ? 1 : 0.45);
            const diffusionDelta = (meanNbrHeight - current) * dynamics.diffusion;
            const coastalBand = Math.max(0, 1 - Math.min(1, Math.abs(current) / 0.14));
            const coastlineDelta =
                (meanNbrHeight - current) * dynamics.coastline * shorelineRatio * coastalBand;
            const delta = diffusionDelta + tectonicDelta + coastlineDelta + buoyancyDelta - fluvialErode;
            const next = Math.min(TERRAIN_HEIGHT_CLAMP, Math.max(-TERRAIN_HEIGHT_CLAMP, current + delta));
            const changed = next - current;
            if (Math.abs(changed) < 1e-8) {
                continue;
            }
            nextHeight[i] = next;
            deltaAbsSum += Math.abs(changed);
        }

        deltaAbsSum += applyLandRatioFloor(nextHeight, plateId, plateIsOcean, targetLandRatio);

        if (deltaAbsSum <= 0) {
            if (movedVertices > 0) {
                plateReassignAccum += movedVertices;
                worldState.terrainCoreDirty = true;
                recordSubsystemActivity(
                    "terrain",
                    Math.min(1, movedVertices / Math.max(1, cellCount) * PLATE_MOTION_ACTIVITY_GAIN),
                );
            }
            return;
        }
        currentTerrainData.heightData = nextHeight;
        syncTerrainHeightToErosionState();
        terrainDeltaAccum += deltaAbsSum;
        plateReassignAccum += movedVertices;
        worldState.terrainCoreDirty = true;
        const deformationSignal =
            deltaAbsSum / Math.max(1, cellCount) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.terrain;
        const plateMotionSignal =
            movedVertices > 0
                ? Math.min(1, movedVertices / Math.max(1, cellCount) * PLATE_MOTION_ACTIVITY_GAIN)
                : 0;
        recordSubsystemActivity("terrain", Math.max(deformationSignal, plateMotionSignal));
    }

    function updateClimateLayerStep() {
        const climateLayer = world.layers[LAYER_KIND.CLIMATE];
        if (!climateLayer || !currentTerrainData?.heightData || !currentTerrainData?.riverFlux) {
            return;
        }

        const { temp, rain } = climateLayer;
        const { heightData, riverFlux } = currentTerrainData;
        const cellCount = Math.min(temp.length, rain.length, heightData.length, riverFlux.length);
        if (cellCount <= 0) {
            return;
        }

        let deltaAbsSum = 0;
        const relaxGain = 0.16;
        for (let i = 0; i < cellCount; i += 1) {
            const baseIndex = i * 3;
            const latAbs = Math.min(1, Math.abs(basePositions[baseIndex + 1] ?? 0));
            const height = heightData[i];
            const flux = Math.max(0, riverFlux[i]);
            const fluxWet = clamp01(Math.log1p(flux) * 0.38);
            const oceanic = height <= 0 ? 1 : 0.25;

            const targetTemp = clamp01(1 - latAbs * 0.95 - Math.max(0, height) * 0.7);
            const targetRain = clamp01(
                (1 - latAbs) * 0.35 + fluxWet * 0.4 + oceanic * 0.3 - Math.max(0, height) * 0.2,
            );

            const prevTemp = temp[i];
            const prevRain = rain[i];
            const nextTemp = prevTemp + (targetTemp - prevTemp) * relaxGain;
            const nextRain = prevRain + (targetRain - prevRain) * relaxGain;
            temp[i] = nextTemp;
            rain[i] = nextRain;
            deltaAbsSum += Math.abs(nextTemp - prevTemp) + Math.abs(nextRain - prevRain);
        }

        recordSubsystemActivity(
            "climate",
            deltaAbsSum / Math.max(1, cellCount * 2) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.climate,
        );
    }

    function updateEcologyLayerStep() {
        const ecologyLayer = world.layers[LAYER_KIND.ECOLOGY];
        const climateLayer = world.layers[LAYER_KIND.CLIMATE];
        const heightData = currentTerrainData?.heightData;
        if (!ecologyLayer || !climateLayer || !heightData) {
            return;
        }

        const { habitability, productivity } = ecologyLayer;
        const { temp, rain } = climateLayer;
        const cellCount = Math.min(
            habitability.length,
            productivity.length,
            temp.length,
            rain.length,
            heightData.length,
        );
        if (cellCount <= 0) {
            return;
        }

        let deltaAbsSum = 0;
        const relaxGain = 0.2;
        for (let i = 0; i < cellCount; i += 1) {
            const height = heightData[i];
            const isLand = height > 0;
            const localTemp = clamp01(temp[i]);
            const localRain = clamp01(rain[i]);

            const temperatureSuitability = clamp01(1 - Math.abs(localTemp - 0.62) * 1.9);
            const moistureSuitability = clamp01(localRain * 1.05);
            const terrainPenalty = isLand ? 1 : 0;
            const targetHabitability = clamp01(
                temperatureSuitability * 0.55 + moistureSuitability * 0.45,
            ) * terrainPenalty;
            const targetProductivity = clamp01(
                targetHabitability * (localRain * 0.65 + localTemp * 0.35),
            );

            const prevHabitability = habitability[i];
            const prevProductivity = productivity[i];
            const nextHabitability =
                prevHabitability + (targetHabitability - prevHabitability) * relaxGain;
            const nextProductivity =
                prevProductivity + (targetProductivity - prevProductivity) * relaxGain;

            habitability[i] = nextHabitability;
            productivity[i] = nextProductivity;
            deltaAbsSum +=
                Math.abs(nextHabitability - prevHabitability) +
                Math.abs(nextProductivity - prevProductivity);
        }

        recordSubsystemActivity(
            "ecology",
            deltaAbsSum / Math.max(1, cellCount * 2) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.ecology,
        );
    }

    function updateCivilizationLayerStep() {
        const civilizationLayer = world.layers[LAYER_KIND.CIVILIZATION];
        const ecologyLayer = world.layers[LAYER_KIND.ECOLOGY];
        const heightData = currentTerrainData?.heightData;
        if (!civilizationLayer || !ecologyLayer || !heightData) {
            return;
        }

        const { population, stateId } = civilizationLayer;
        const { habitability, productivity } = ecologyLayer;
        const cellCount = Math.min(
            population.length,
            stateId.length,
            habitability.length,
            productivity.length,
            heightData.length,
        );
        if (cellCount <= 0) {
            return;
        }

        let populationDeltaSum = 0;
        let polityChangeCount = 0;
        const relaxGain = 0.08;
        for (let i = 0; i < cellCount; i += 1) {
            const isLand = heightData[i] > 0;
            const carrying = isLand
                ? clamp01(habitability[i] * 0.7 + productivity[i] * 0.3)
                : 0;
            const prevPopulation = population[i];
            let nextPopulation = prevPopulation + (carrying - prevPopulation) * relaxGain;
            if (carrying > 0.42 && nextPopulation < 0.02) {
                nextPopulation = 0.02;
            }
            if (!isLand || carrying < 0.05) {
                nextPopulation = 0;
            }
            nextPopulation = clamp01(nextPopulation);
            population[i] = nextPopulation;
            populationDeltaSum += Math.abs(nextPopulation - prevPopulation);

            const prevStateId = stateId[i];
            let nextStateId = prevStateId;
            if (nextPopulation > 0.18 && prevStateId === 0) {
                nextStateId = i + 1;
            } else if (nextPopulation < 0.03 && prevStateId !== 0) {
                nextStateId = 0;
            }
            if (nextStateId !== prevStateId) {
                stateId[i] = nextStateId;
                polityChangeCount += 1;
            }
        }

        const populationSignal = populationDeltaSum / Math.max(1, cellCount) * 4;
        const politySignal = polityChangeCount / Math.max(1, cellCount);
        recordSubsystemActivity(
            "civilization",
            Math.max(populationSignal, politySignal * 6) * SUBSYSTEM_ACTIVITY_SIGNAL_GAIN.civilization,
        );
    }

    function runSubsystemStep(subsystemKey) {
        if (!currentTerrainData) {
            return;
        }
        worldState.executedSteps[subsystemKey] += 1;
        if (subsystemKey === "terrain") {
            updateTerrainCoreStep();
            return;
        }
        if (subsystemKey === "climate") {
            updateClimateLayerStep();
            return;
        }
        if (subsystemKey === "ecology") {
            updateEcologyLayerStep();
            return;
        }
        if (subsystemKey === "civilization") {
            updateCivilizationLayerStep();
        }
    }

    function runClimateStep(steps) {
        if (!world.layers[LAYER_KIND.CLIMATE]) {
            return;
        }
        for (let i = 0; i < steps; i += 1) {
            runSubsystemStep("climate");
        }
    }

    function runEcologyStep(steps) {
        if (!world.layers[LAYER_KIND.ECOLOGY]) {
            return;
        }
        for (let i = 0; i < steps; i += 1) {
            runSubsystemStep("ecology");
        }
    }

    function runCivilizationStep(steps) {
        if (!world.layers[LAYER_KIND.CIVILIZATION]) {
            return;
        }
        for (let i = 0; i < steps; i += 1) {
            runSubsystemStep("civilization");
        }
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
        enqueueRiverStep(world.budgets.river);
        runClimateStep(world.budgets.climate);
        runEcologyStep(world.budgets.ecology);
        runCivilizationStep(world.budgets.civilization);

        if (worldState.terrainCoreDirty) {
            updateTerrainAttributes();
            updateGeometryPositions();
            worldState.terrainCoreDirty = false;
        }

        const terrainActivity = buildObservedActivityForTick("terrain", preset);
        const riverActivity = buildObservedActivityForTick("river", preset);
        const climateActivity = buildObservedActivityForTick("climate", preset);
        const ecologyActivity = buildObservedActivityForTick("ecology", preset);
        const civilizationActivity = buildObservedActivityForTick("civilization", preset);
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
        maybeSaveDebugSnapshot(world.tick, prevHeightForSnapshot);

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

    function maybeSaveDebugSnapshot(tick, prevHeightForSnapshot = null) {
        if (!import.meta.env.DEV) {
            return;
        }
        if (!Number.isInteger(tick) || tick < 0) {
            return;
        }
        if (!debugSnapshotTickSet.has(tick)) {
            return;
        }
        if (debugSnapshotSavedTicks.has(tick)) {
            return;
        }
        if (!currentTerrainData) {
            return;
        }

        debugSnapshotSavedTicks.add(tick);
        void postDebugSnapshot(tick, prevHeightForSnapshot);
    }

    async function postDebugSnapshot(tick, prevHeightForSnapshot = null) {
        const payload = buildDebugSnapshotPayload(tick, prevHeightForSnapshot);
        if (!payload) {
            return;
        }

        try {
            const response = await fetch("/__debug/snapshot", {
                method: "POST",
                headers: {
                    "content-type": "application/json",
                },
                body: JSON.stringify(payload),
            });
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            const result = await response.json().catch(() => null);
            const fileLabel = typeof result?.file === "string" ? result.file : "debug/snapshots/latest.json";
            setStatus(`Snapshot saved at tick=${tick}: ${fileLabel}`);
        } catch (error) {
            console.warn("[debug-snapshot] failed to save", error);
            setStatus(`Snapshot save failed at tick=${tick}`);
        }
    }

    function buildDebugSnapshotPayload(tick, prevHeightForSnapshot = null) {
        if (!currentTerrainData) {
            return null;
        }

        const heightData = currentTerrainData.heightData;
        const plateId = currentTerrainData.plateId;
        const riverFlux = currentTerrainData.riverFlux;
        if (!heightData || !plateId || !riverFlux) {
            return null;
        }

        const cellCount = Math.min(
            heightData.length,
            plateId.length,
            riverFlux.length,
        );
        if (cellCount <= 0) {
            return null;
        }

        let seaCount = 0;
        let maxHeight = -Infinity;
        let minHeight = Infinity;
        let sumHeight = 0;
        let sumRiverFlux = 0;
        let highlandCount = 0;
        let clampCount = 0;
        for (let i = 0; i < cellCount; i += 1) {
            const h = heightData[i];
            sumHeight += h;
            sumRiverFlux += riverFlux[i];
            if (h <= 0) {
                seaCount += 1;
            }
            if (h >= 0.45) {
                highlandCount += 1;
            }
            if (Math.abs(h) >= TERRAIN_HEIGHT_CLAMP - 1e-4) {
                clampCount += 1;
            }
            if (h > maxHeight) {
                maxHeight = h;
            }
            if (h < minHeight) {
                minHeight = h;
            }
        }

        const plateCount = currentTerrainData.plateInfo?.isOcean?.length ?? 0;
        const plateCellCounts = new Array(plateCount).fill(0);
        for (let i = 0; i < cellCount; i += 1) {
            const pid = plateId[i];
            if (Number.isInteger(pid) && pid >= 0 && pid < plateCount) {
                plateCellCounts[pid] += 1;
            }
        }

        const hasPrev = !!prevHeightForSnapshot && prevHeightForSnapshot.length >= cellCount;
        const topChanges = [];
        let deltaAbsSum = 0;
        let deltaAbsMax = 0;
        for (let i = 0; i < cellCount; i += 1) {
            const prev = hasPrev ? prevHeightForSnapshot[i] : heightData[i];
            const delta = heightData[i] - prev;
            const absDelta = Math.abs(delta);
            deltaAbsSum += absDelta;
            if (absDelta > deltaAbsMax) {
                deltaAbsMax = absDelta;
            }
            topChanges.push({
                i,
                p: plateId[i],
                h: Number(heightData[i].toFixed(5)),
                dh: Number(delta.toFixed(5)),
                rf: Number(riverFlux[i].toFixed(5)),
            });
        }
        topChanges.sort((a, b) => Math.abs(b.dh) - Math.abs(a.dh));
        const topKChanges = topChanges.slice(0, DEBUG_SNAPSHOT_TOPK_LIMIT);

        return {
            type: "terrain-debug-snapshot",
            version: 1,
            createdAt: new Date().toISOString(),
            tick,
            seed: currentSeed,
            era: currentEraScale,
            mesh: {
                vertexCount: cellCount,
                neighborEdgeCount: world.mesh?.nbrs?.length ?? worldState.erosionAutomatonState?.nbrs?.length ?? 0,
                level: LEVEL,
            },
            stats: {
                seaRatio: seaCount / cellCount,
                landRatio: 1 - seaCount / cellCount,
                targetLandRatio: Number.isFinite(currentTerrainData.targetLandRatio)
                    ? currentTerrainData.targetLandRatio
                    : null,
                highlandRatio: highlandCount / cellCount,
                minHeight,
                maxHeight,
                meanHeight: sumHeight / cellCount,
                meanRiverFlux: sumRiverFlux / cellCount,
                meanAbsHeightDelta: deltaAbsSum / cellCount,
                maxAbsHeightDelta: deltaAbsMax,
                clampRatio: clampCount / cellCount,
                plateCount,
            },
            plateStats: {
                cellCounts: plateCellCounts,
            },
            topKChanges,
        };
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
        drainRiverQueue(preset);
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
        const waitNextFrame = () =>
            new Promise((resolve) => requestAnimationFrame(resolve));

        setStatus(`Generating terrain for "${nextSeed}"...`);
        seedForm.querySelector("button")?.setAttribute("disabled", "disabled");
        seedInput.setAttribute("disabled", "disabled");

        await waitNextFrame();
        if (token !== generationToken) {
            return;
        }

        let terrain;
        const crustTerrainAutomaton = new CrustTerrainAutomaton(nextSeed, TERRAIN_PARAMS);
        try {
            let stepCount = 0;
            while (!crustTerrainAutomaton.isDone()) {
                if (token !== generationToken) {
                    return;
                }
                if (stepCount % 2 === 0) {
                    setStatus(
                        `Generating terrain for "${nextSeed}"... (${crustTerrainAutomaton.phaseName()})`,
                    );
                    await waitNextFrame();
                    if (token !== generationToken) {
                        return;
                    }
                }
                crustTerrainAutomaton.step(256);
                stepCount += 1;
            }
            terrain = crustTerrainAutomaton.finish();
        } finally {
            crustTerrainAutomaton.free();
        }

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

        world.core = {
            heightData,
            plateId,
            riverFlux,
            riverNext,
            lakeDepth,
            plateInfo,
            vertexWeight,
            tectonicDebug,
            targetLandRatio: Number.isFinite(terrain.land_ratio) ? terrain.land_ratio : 0.0,
        };
        currentTerrainData = world.core;
        plateMotionState = createPlateMotionState(nextSeed);
        world.layers = createEmptyLayers();
        worldState.erosionAutomatonState = erosionAutomatonState;
        worldState.pendingRiverSteps = 0;
        worldState.terrainErosionDirty = false;
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
        if (eraScaleSelect.disabled) {
            renderEraScaleControls();
            return;
        }
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
    eraScaleSelect.setAttribute("disabled", "disabled");
    eraScaleSelect.setAttribute("aria-disabled", "true");
    eraScaleSelect.title = "時代プリセットは進行状況に応じて自動切り替えされます。";
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
