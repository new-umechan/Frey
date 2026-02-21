import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import initWasm, { generate_mesh, generate_terrain } from "./wasm/frey_wasm.js";

const LEVEL = 6;
const TERRAIN_SEED = "alpha";
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
};

async function bootstrap() {
    const canvas = document.getElementById("mesh-canvas");
    const statsElement = document.getElementById("stats");

    if (!(canvas instanceof HTMLCanvasElement) || !(statsElement instanceof HTMLElement)) {
        throw new Error("required DOM elements are missing");
    }

    await initWasm();
    const mesh = generate_mesh(LEVEL);
    const terrain = generate_terrain(TERRAIN_SEED, TERRAIN_PARAMS);

    const positions = new Float32Array(mesh.positions);
    const indices = new Uint32Array(mesh.indices);
    const height = new Float32Array(terrain.height);
    const plateId = new Uint32Array(terrain.plate_id);
    const riverFlux = new Float32Array(terrain.river_flux);

    const colors = new Float32Array((positions.length / 3) * 3);
    for (let i = 0; i < positions.length; i += 3) {
        const v = i / 3;
        const h = height[v];
        const r = positions[i];
        const g = positions[i + 1];
        const b = positions[i + 2];
        const renderHeight = h > 0.0 ? h : 0.0;
        const radius = 1.0 + renderHeight * 0.04;
        const river = riverFlux[v];

        positions[i] = r * radius;
        positions[i + 1] = g * radius;
        positions[i + 2] = b * radius;

        let color;
        if (h <= 0.0) {
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

        colors[i] = color.r;
        colors[i + 1] = color.g;
        colors[i + 2] = color.b;
    }

    const scene = new THREE.Scene();
    scene.background = new THREE.Color("#e9e4d8");

    const camera = new THREE.PerspectiveCamera(42, window.innerWidth / window.innerHeight, 0.1, 100);
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
    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
    geometry.setIndex(new THREE.BufferAttribute(indices, 1));
    geometry.computeVertexNormals();
    geometry.computeBoundingSphere();

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

    const keyLight = new THREE.DirectionalLight("#fff8eb", 1.1);
    keyLight.position.set(1.3, 1.4, 1.2);
    scene.add(keyLight);

    const fillLight = new THREE.DirectionalLight("#c9dbff", 0.6);
    fillLight.position.set(-1.5, -0.5, -1.2);
    scene.add(fillLight);

    const ambient = new THREE.AmbientLight("#f2f2e8", 0.45);
    scene.add(ambient);

    const haloGeometry = new THREE.SphereGeometry(1.01, 64, 64);
    const haloMaterial = new THREE.MeshBasicMaterial({
        color: "#8d907f",
        transparent: true,
        opacity: 0.08,
    });
    const halo = new THREE.Mesh(haloGeometry, haloMaterial);
    scene.add(halo);

    const plateCount = new Set(plateId).size;
    const landRatio = height.reduce((acc, h) => acc + (h > 0.0 ? 1 : 0), 0) / height.length;
    statsElement.textContent = `Vertices: ${positions.length / 3} / Triangles: ${indices.length / 3} / L=${LEVEL} / Seed=${TERRAIN_SEED} / Plates=${plateCount} / Land=${(landRatio * 100).toFixed(1)}%`;

    function onResize() {
        const width = window.innerWidth;
        const height = window.innerHeight;
        camera.aspect = width / height;
        camera.updateProjectionMatrix();
        renderer.setSize(width, height);
    }

    window.addEventListener("resize", onResize);

    function render() {
        controls.update();
        renderer.render(scene, camera);
        requestAnimationFrame(render);
    }

    render();
}

bootstrap().catch((error) => {
    const statsElement = document.getElementById("stats");
    if (statsElement instanceof HTMLElement) {
        statsElement.textContent = `Initialization failed: ${String(error)}`;
    }
    console.error(error);
});
