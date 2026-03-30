import * as THREE from "three";

export function createLoadingOverlayController(options: any = {}) {
    const {
        loadingOverlayCanvas,
        viewportPanel,
        sphere,
        getCamera,
        circleColor = "#E5EAEE",
    } = options;
    const loadingOverlayContext = loadingOverlayCanvas.getContext("2d");
    if (!loadingOverlayContext) {
        throw new Error("loading overlay canvas context is unavailable");
    }

    const loadingPlanetCenterWorld = new THREE.Vector3();
    const loadingPlanetEdgeWorld = new THREE.Vector3();
    const loadingPlanetCenterNdc = new THREE.Vector3();
    const loadingPlanetEdgeNdc = new THREE.Vector3();
    const loadingPlanetEdgeLocal = new THREE.Vector3(1, 0, 0);
    let isWorldInitializing = false;

    function syncCanvasSize() {
        const panelRect = viewportPanel.getBoundingClientRect();
        const panelWidth = Math.max(1, Math.floor(panelRect.width));
        const panelHeight = Math.max(1, Math.floor(panelRect.height));
        const dpr = Math.min(window.devicePixelRatio || 1, 2);
        const bufferWidth = Math.max(1, Math.floor(panelWidth * dpr));
        const bufferHeight = Math.max(1, Math.floor(panelHeight * dpr));
        if (
            loadingOverlayCanvas.width !== bufferWidth ||
            loadingOverlayCanvas.height !== bufferHeight
        ) {
            loadingOverlayCanvas.width = bufferWidth;
            loadingOverlayCanvas.height = bufferHeight;
        }
        loadingOverlayContext.setTransform(dpr, 0, 0, dpr, 0, 0);
        return {
            panelWidth,
            panelHeight,
        };
    }

    function clear() {
        const { panelWidth, panelHeight } = syncCanvasSize();
        loadingOverlayContext.clearRect(0, 0, panelWidth, panelHeight);
    }

    function measurePlanetScreenRadius(camera: THREE.Camera, panelWidth: number, panelHeight: number) {
        sphere.getWorldPosition(loadingPlanetCenterWorld);
        loadingPlanetEdgeWorld.copy(loadingPlanetEdgeLocal);
        sphere.localToWorld(loadingPlanetEdgeWorld);

        loadingPlanetCenterNdc.copy(loadingPlanetCenterWorld).project(camera);
        loadingPlanetEdgeNdc.copy(loadingPlanetEdgeWorld).project(camera);

        const centerX = (loadingPlanetCenterNdc.x * 0.5 + 0.5) * panelWidth;
        const centerY = (1 - (loadingPlanetCenterNdc.y * 0.5 + 0.5)) * panelHeight;
        const edgeX = (loadingPlanetEdgeNdc.x * 0.5 + 0.5) * panelWidth;
        const edgeY = (1 - (loadingPlanetEdgeNdc.y * 0.5 + 0.5)) * panelHeight;

        return Math.hypot(edgeX - centerX, edgeY - centerY);
    }

    function render() {
        if (!isWorldInitializing) {
            clear();
            return;
        }

        const { panelWidth, panelHeight } = syncCanvasSize();
        loadingOverlayContext.clearRect(0, 0, panelWidth, panelHeight);

        const camera = getCamera();
        const radius = measurePlanetScreenRadius(camera, panelWidth, panelHeight);
        if (!Number.isFinite(radius) || radius <= 0) {
            return;
        }

        loadingOverlayContext.fillStyle = circleColor;
        loadingOverlayContext.beginPath();
        loadingOverlayContext.arc(panelWidth * 0.5, panelHeight * 0.5, radius, 0, Math.PI * 2);
        loadingOverlayContext.fill();
    }

    function setWorldInitializing(value: boolean) {
        isWorldInitializing = Boolean(value);
    }

    return {
        clear,
        render,
        setWorldInitializing,
    };
}
