import * as THREE from "three";
import initWasm, { generate_mesh, generate_terrain } from "./wasm/frey_wasm.js";
import { collectAppElements } from "./app/dom.js";
import { createGlobeScene, resizeViewport } from "./app/scene.js";
import { buildEquirectangularMapTexture } from "./app/map-texture.js";
import {
    buildRenderPositions,
    buildVertexColors,
    summarizeTerrain,
} from "./app/terrain-visuals.js";

const LEVEL = 6;
const DEFAULT_TERRAIN_SEED = "alpha";
const DEFAULT_VIEW_MODE = "normal";
const DEFAULT_SURFACE_MODE = "globe";
const PLATE_HOVER_POPUP_DELAY_MS = 450;
const TERRAIN_PARAMS = {
    level: LEVEL,
    l_max: 4,
    alpha: 1.5,
    num_plates_min: 8,
    num_plates_max: 18,
    ocean_plate_ratio: 0.65,
    boundary_band: 0.08,
    boundary_convergent_base_gain: 0.58,
    boundary_divergent_base_gain: 0.34,
    boundary_transform_relief_gain: 0.08,
    trench_gain: 0.34,
    arc_gain: 0.28,
    collision_gain: 0.44,
    rift_gain: 0.22,
    boundary_width_trench: 0.11,
    boundary_width_arc: 0.22,
    boundary_width_collision: 0.30,
    boundary_width_rift: 0.19,
    boundary_obliquity_mix: 0.50,
    boundary_distance_falloff: 1.0,
    boundary_anisotropy: 0.40,
    smooth_iter: 8,
    smooth_lambda: 0.38,
    river_rain_base: 0.5,
    river_accum_threshold: 0.035,
    erosion_iter: 12,
    hydraulic_erode_rate: 0.02,
    hydraulic_deposit_rate: 0.35,
    sediment_capacity_gain: 0.9,
    erosion_min_slope: 0.002,
    erosion_max_delta_per_iter: 0.015,
    coastal_deposit_rate: 0.45,
    shallow_sea_floor: -0.08,
};

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
        mapPlane,
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
    let currentMapTexture = null;

    function updateMapTexture(vertexColors) {
        const nextTexture = buildEquirectangularMapTexture(basePositions, indices, vertexColors);
        const mapMaterial = mapPlane.material;
        if (!(mapMaterial instanceof THREE.MeshBasicMaterial)) {
            return;
        }
        if (currentMapTexture) {
            currentMapTexture.dispose();
        }
        currentMapTexture = nextTexture;
        mapMaterial.map = nextTexture;
        mapMaterial.needsUpdate = true;
    }

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
            sphere.visible = false;
            wireframe.visible = false;
            halo.visible = false;
            mapPlane.visible = true;
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
        mapPlane.visible = false;
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

    function applyCurrentViewColors() {
        if (!currentTerrainData) {
            return;
        }
        const colors = buildVertexColors(
            currentTerrainData.heightData,
            currentTerrainData.plateId,
            currentTerrainData.riverFlux,
            currentViewMode,
        );
        geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
        updateMapTexture(colors);
    }

    function setViewMode(nextMode) {
        const normalizedMode = nextMode === "plates" ? "plates" : "normal";
        currentViewMode = normalizedMode;
        for (const input of viewModeInputs) {
            input.checked = input.value === normalizedMode;
        }
        applyCurrentViewColors();
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
        const heightData = new Float32Array(terrain.height);
        const plateId = new Uint32Array(terrain.plate_id);
        const riverFlux = new Float32Array(terrain.river_flux);
        const plateInfo = {
            isOcean: new Uint8Array(terrain.plate_is_ocean),
            baseHeight: new Float32Array(terrain.plate_base_height),
            baseWeight: new Float32Array(terrain.plate_base_weight),
        };
        const vertexWeight = new Float32Array(terrain.vertex_weight);
        if (token !== generationToken) {
            return;
        }

        currentTerrainData = {
            heightData,
            plateId,
            riverFlux,
            plateInfo,
            vertexWeight,
        };
        updateGeometryPositions();
        applyCurrentViewColors();
        hidePlateHoverPopup();

        const { plateCount, landRatio } = summarizeTerrain(heightData, plateId);

        currentSeed = nextSeed;
        statFields.vertices.textContent = `${basePositions.length / 3}`;
        statFields.level.textContent = `${LEVEL}`;
        statFields.seed.textContent = currentSeed;
        statFields.plates.textContent = `${plateCount}`;
        statFields.land.textContent = `${(landRatio * 100).toFixed(1)}%`;

        setStatus(`Ready (${currentSeed})`);
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
    onResize();
    hidePlateHoverPopup();

    function render() {
        activeControls.update();
        renderer.render(scene, camera);
        requestAnimationFrame(render);
    }

    render();
}

bootstrap().catch((error) => {
    const statusMessage = document.getElementById("status-message");
    if (statusMessage instanceof HTMLElement) {
        statusMessage.textContent = `Initialization failed: ${String(error)}`;
    }
    console.error(error);
});
