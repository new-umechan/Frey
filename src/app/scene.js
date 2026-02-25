import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

export function createGlobeScene(canvas, indices) {
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

    return {
        scene,
        camera,
        renderer,
        controls,
        geometry,
        sphere,
    };
}

export function resizeViewport(viewportPanel, camera, renderer) {
    const width = viewportPanel.clientWidth;
    const height = viewportPanel.clientHeight;
    if (width <= 0 || height <= 0) {
        return;
    }
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    renderer.setSize(width, height);
}
