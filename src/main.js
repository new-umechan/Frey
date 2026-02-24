import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import initWasm, { generate_mesh, generate_terrain } from "./wasm/frey_wasm.js";

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

function requireElement(id, type) {
    const element = document.getElementById(id);
    if (!(element instanceof type)) {
        throw new Error(`required DOM element is missing: #${id}`);
    }
    return element;
}

async function bootstrap() {
    const appShell = requireElement("mesh-canvas", HTMLCanvasElement).closest(".app-shell");
    const canvas = requireElement("mesh-canvas", HTMLCanvasElement);
    const viewportPanel = requireElement("viewport-panel", HTMLDivElement);
    const seedForm = requireElement("seed-form", HTMLFormElement);
    const seedInput = requireElement("seed-input", HTMLInputElement);
    const sidebarToggle = requireElement("sidebar-toggle", HTMLButtonElement);
    const statusMessage = requireElement("status-message", HTMLElement);
    const viewModeInputs = Array.from(
        document.querySelectorAll('input[name="view-mode"]'),
    ).filter((input) => input instanceof HTMLInputElement);

    if (!(appShell instanceof HTMLElement)) {
        throw new Error("required app shell is missing");
    }

    const statFields = {
        vertices: requireElement("stat-vertices", HTMLElement),
        triangles: requireElement("stat-triangles", HTMLElement),
        level: requireElement("stat-level", HTMLElement),
        seed: requireElement("stat-seed", HTMLElement),
        plates: requireElement("stat-plates", HTMLElement),
        land: requireElement("stat-land", HTMLElement),
    };

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

    const scene = new THREE.Scene();
    scene.background = new THREE.Color("#e8edf3");

    const camera = new THREE.PerspectiveCamera(42, 1, 0.1, 100);
    camera.position.set(0, 0, 2.7);

    const renderer = new THREE.WebGLRenderer({
        antialias: true,
        canvas,
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(window.innerWidth, window.innerHeight);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.minDistance = 1.2;
    controls.maxDistance = 6.0;

    const geometry = new THREE.BufferGeometry();
    geometry.setIndex(new THREE.BufferAttribute(indices, 1));

    const material = new THREE.MeshStandardMaterial({
        vertexColors: true,
        roughness: 0.95,
        metalness: 0.02,
    });

    const sphere = new THREE.Mesh(geometry, material);
    scene.add(sphere);

    const wireframe = new THREE.Mesh(
        geometry,
        new THREE.MeshBasicMaterial({
            color: "#2f2b22",
            wireframe: true,
            transparent: true,
            opacity: 0.08,
        }),
    );
    scene.add(wireframe);

    const keyLight = new THREE.DirectionalLight("#f1f6ff", 1.05);
    keyLight.position.set(1.3, 1.4, 1.2);
    scene.add(keyLight);

    const fillLight = new THREE.DirectionalLight("#c8daf1", 0.55);
    fillLight.position.set(-1.5, -0.5, -1.2);
    scene.add(fillLight);

    const ambient = new THREE.AmbientLight("#eef2f8", 0.42);
    scene.add(ambient);

    const haloGeometry = new THREE.SphereGeometry(1.01, 64, 64);
    const haloMaterial = new THREE.MeshBasicMaterial({
        color: "#8390a3",
        transparent: true,
        opacity: 0.08,
    });
    const halo = new THREE.Mesh(haloGeometry, haloMaterial);
    scene.add(halo);

    let generationToken = 0;
    let currentSeed = DEFAULT_TERRAIN_SEED;
    let currentViewMode = DEFAULT_VIEW_MODE;
    let currentTerrainData = null;

    function plateModeColor(plate, heightValue) {
        const hue = ((plate * 137.508) % 360) / 360;
        const saturation = 0.58;
        const lightness = heightValue > 0.0 ? 0.54 : 0.38;
        return new THREE.Color().setHSL(hue, saturation, lightness);
    }

    function buildVertexColors(heightData, plateId, riverFlux, viewMode) {
        const colors = new Float32Array(heightData.length * 3);

        for (let v = 0; v < heightData.length; v += 1) {
            const h = heightData[v];
            const river = riverFlux[v];
            let color;

            if (viewMode === "plates") {
                color = plateModeColor(plateId[v], h);
                if (h <= 0.0) {
                    color.lerp(new THREE.Color("#0e2847"), 0.25);
                }
            } else if (h <= 0.0) {
                color = new THREE.Color("#12406a");
            } else {
                const t = Math.min(1.0, h);
                color = new THREE.Color(
                    THREE.MathUtils.lerp(0.18, 0.62, t),
                    THREE.MathUtils.lerp(0.42, 0.56, t),
                    THREE.MathUtils.lerp(0.20, 0.48, t),
                );
                if (river > 0.10 && h < 0.45) {
                    color.lerp(new THREE.Color("#4ca3dd"), Math.min(0.35, river * 0.45));
                }
            }

            const i = v * 3;
            colors[i] = color.r;
            colors[i + 1] = color.g;
            colors[i + 2] = color.b;
        }

        return colors;
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
    }

    function onResize() {
        const width = viewportPanel.clientWidth;
        const height = viewportPanel.clientHeight;
        if (width <= 0 || height <= 0) {
            return;
        }
        camera.aspect = width / height;
        camera.updateProjectionMatrix();
        renderer.setSize(width, height);
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
        const positions = new Float32Array(basePositions);
        const heightData = new Float32Array(terrain.height);
        const plateId = new Uint32Array(terrain.plate_id);
        const riverFlux = new Float32Array(terrain.river_flux);

        for (let i = 0; i < positions.length; i += 3) {
            const v = i / 3;
            const h = heightData[v];
            const x = positions[i];
            const y = positions[i + 1];
            const z = positions[i + 2];
            const renderHeight = h > 0.0 ? h : 0.0;
            const radius = 1.0 + renderHeight * 0.04;

            positions[i] = x * radius;
            positions[i + 1] = y * radius;
            positions[i + 2] = z * radius;
        }

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

        const plateCount = new Set(plateId).size;
        const landRatio =
            heightData.reduce((acc, h) => acc + (h > 0.0 ? 1 : 0), 0) / Math.max(1, heightData.length);

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
            currentViewMode = input.value === "plates" ? "plates" : "normal";
            applyCurrentViewColors();
        });
    }

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
