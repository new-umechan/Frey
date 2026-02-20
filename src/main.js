import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import initWasm, { generate_mesh } from "./wasm/frey_wasm.js";

const LEVEL = 6;

async function bootstrap() {
    const canvas = document.getElementById("mesh-canvas");
    const statsElement = document.getElementById("stats");

    if (!(canvas instanceof HTMLCanvasElement) || !(statsElement instanceof HTMLElement)) {
        throw new Error("required DOM elements are missing");
    }

    await initWasm();
    const mesh = generate_mesh(LEVEL);

    const positions = new Float32Array(mesh.positions);
    const indices = new Uint32Array(mesh.indices);

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
    geometry.setIndex(new THREE.BufferAttribute(indices, 1));
    geometry.computeBoundingSphere();

    const material = new THREE.MeshBasicMaterial({
        color: "#2f2b22",
        wireframe: true,
    });

    const sphere = new THREE.Mesh(geometry, material);
    scene.add(sphere);

    const haloGeometry = new THREE.SphereGeometry(1.01, 64, 64);
    const haloMaterial = new THREE.MeshBasicMaterial({
        color: "#8d907f",
        transparent: true,
        opacity: 0.08,
    });
    const halo = new THREE.Mesh(haloGeometry, haloMaterial);
    scene.add(halo);

    statsElement.textContent =
        `Vertices: ${positions.length / 3} / Triangles: ${indices.length / 3} / Subdivision: L=${LEVEL}`;

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
