import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { TrackballControls } from "three/examples/jsm/controls/TrackballControls.js";
import { createTerrainMaterial, type TerrainMaterialController } from "./materials/terrain";

const PLANET_CENTER_X = 0.16;
const INITIAL_GLOBE_CAMERA_DISTANCE = 3.2;

export interface GlobeScene {
    scene: THREE.Scene;
    globeCamera: THREE.PerspectiveCamera;
    mapCamera: THREE.OrthographicCamera;
    renderer: THREE.WebGLRenderer;
    globeControls: TrackballControls;
    mapControls: OrbitControls;
    geometry: THREE.BufferGeometry;
    sphere: THREE.Mesh;
    halo: THREE.Mesh;
    terrainMaterial: TerrainMaterialController;
}

export function createGlobeScene(canvas: HTMLCanvasElement, indices: Uint32Array): GlobeScene {
    const scene = new THREE.Scene();
    scene.background = null;

    const globeCamera = new THREE.PerspectiveCamera(42, 1, 0.1, 100);
    globeCamera.position.set(PLANET_CENTER_X, 0, INITIAL_GLOBE_CAMERA_DISTANCE);
    const mapCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.01, 100);
    mapCamera.position.set(0, 0, 5);
    mapCamera.lookAt(0, 0, 0);
    mapCamera.updateProjectionMatrix();

    const renderer = new THREE.WebGLRenderer({
        antialias: true,
        canvas,
        alpha: true,
    });
    renderer.setClearAlpha(0);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(window.innerWidth, window.innerHeight);

    const globeControls = new TrackballControls(globeCamera, renderer.domElement);
    globeControls.noPan = true;
    globeControls.noZoom = true;
    globeControls.rotateSpeed = 2.4;
    globeControls.dynamicDampingFactor = 0.08;
    globeControls.staticMoving = false;
    globeControls.target.set(PLANET_CENTER_X, 0, 0);
    globeControls.minDistance = 1.2;
    globeControls.maxDistance = 6.0;
    globeControls.update();

    const mapControls = new OrbitControls(mapCamera, renderer.domElement);
    mapControls.enableRotate = false;
    mapControls.enablePan = true;
    mapControls.enableZoom = true;
    mapControls.enableDamping = false;
    mapControls.zoomSpeed = 1.0;
    mapControls.screenSpacePanning = true;
    mapControls.mouseButtons.LEFT = THREE.MOUSE.PAN;
    mapControls.touches.ONE = THREE.TOUCH.PAN;

    const geometry = new THREE.BufferGeometry();
    geometry.setIndex(new THREE.BufferAttribute(indices, 1));

    const terrainMaterial = createTerrainMaterial();
    const material = terrainMaterial.material;

    const sphere = new THREE.Mesh(geometry, material);
    sphere.position.setX(PLANET_CENTER_X);
    scene.add(sphere);

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
    halo.position.setX(PLANET_CENTER_X);
    scene.add(halo);

    return {
        scene,
        globeCamera,
        mapCamera,
        renderer,
        globeControls,
        mapControls,
        geometry,
        sphere,
        halo,
        terrainMaterial,
    };
}

export function resizeViewport(
    viewportPanel: HTMLElement,
    globeCamera: THREE.PerspectiveCamera,
    mapCamera: THREE.OrthographicCamera,
    renderer: THREE.WebGLRenderer
): void {
    const width = viewportPanel.clientWidth;
    const height = viewportPanel.clientHeight;
    if (width <= 0 || height <= 0) {
        return;
    }
    globeCamera.aspect = width / height;
    globeCamera.updateProjectionMatrix();

    const aspect = width / height;
    const mapAspect = 2;
    const margin = 1.08;
    let halfWidth = 1 * margin;
    let halfHeight = 0.5 * margin;
    if (aspect > mapAspect) {
        halfWidth = aspect * halfHeight;
    } else {
        halfHeight = (halfWidth / aspect);
    }
    mapCamera.left = -halfWidth;
    mapCamera.right = halfWidth;
    mapCamera.top = halfHeight;
    mapCamera.bottom = -halfHeight;
    mapCamera.updateProjectionMatrix();

    renderer.setSize(width, height);
}
