import * as THREE from "three";
import initWasm, { generate_mesh, generate_terrain } from "./wasm/frey_wasm.js";
import { collectAppElements } from "./app/dom.js";
import { createGlobeScene, resizeViewport } from "./app/scene.js";
import {
    buildRenderPositions,
    buildVertexColors,
    summarizeTerrain,
} from "./app/terrain-visuals.js";

const LEVEL = 6;
const DEFAULT_TERRAIN_SEED = "alpha";
const DEFAULT_VIEW_MODE = "normal";
const TERRAIN_PARAMS = {
    level: LEVEL,
    l_max: 4,
    alpha: 1.5,
    num_plates_min: 8,
    num_plates_max: 18,
    ocean_plate_ratio: 0.65,
    boundary_band: 0.08,
    uplift_gain: 0.28,
    subduct_gain: 0.24,
    divergent_gain: 0.10,
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

    const { scene, camera, renderer, controls, geometry } = createGlobeScene(canvas, indices);

    let generationToken = 0;
    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentTerrainData = null;

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
    }

    function setViewMode(nextMode) {
        const normalizedMode = nextMode === "plates" ? "plates" : "normal";
        currentViewMode = normalizedMode;
        for (const input of viewModeInputs) {
            input.checked = input.value === normalizedMode;
        }
        applyCurrentViewColors();
    }

    function onResize() {
        resizeViewport(viewportPanel, camera, renderer);
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
        const positions = buildRenderPositions(basePositions, heightData);

        if (token !== generationToken) {
            return;
        }

        geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
        currentTerrainData = {
            heightData,
            plateId,
            riverFlux,
        };
        applyCurrentViewColors();
        geometry.computeVertexNormals();
        geometry.computeBoundingSphere();

        const { plateCount, landRatio } = summarizeTerrain(heightData, plateId);

        currentSeed = nextSeed;
        statFields.vertices.textContent = `${positions.length / 3}`;
        statFields.triangles.textContent = `${indices.length / 3}`;
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

    function render() {
        controls.update();
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
