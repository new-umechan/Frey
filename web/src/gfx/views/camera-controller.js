export function createCameraController({
    globeCamera,
    mapCamera,
    globeControls,
    mapControls,
    sphere,
    wireframe,
    halo,
    resizeViewport,
    viewportPanel,
    renderer,
    isDebugEnabled,
}) {
    const centerX = sphere.position.x;
    let currentSurfaceMode = "globe";
    let camera = globeCamera;
    let activeControls = globeControls;

    function fitCameraToCurrentSurface() {
        if (currentSurfaceMode === "map") {
            camera = mapCamera;
            mapCamera.position.set(centerX, 0, 5);
            mapCamera.up.set(0, 1, 0);
            mapCamera.lookAt(centerX, 0, 0);
            mapControls.target.set(centerX, 0, 0);
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
        globeCamera.position.set(centerX, 0, 2.7);
        globeCamera.up.set(0, 1, 0);
        globeControls.target.set(centerX, 0, 0);
        activeControls = globeControls;
        globeControls.enabled = true;
        mapControls.enabled = false;
        sphere.visible = true;
        wireframe.visible = isDebugEnabled() && currentSurfaceMode === "globe";
        halo.visible = true;
        globeControls.update();
    }

    function setSurfaceMode(nextMode) {
        const normalizedMode = nextMode === "map" ? "map" : "globe";
        currentSurfaceMode = normalizedMode;
        fitCameraToCurrentSurface();
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

    function getCamera() {
        return camera;
    }

    function getActiveControls() {
        return activeControls;
    }

    function getSurfaceMode() {
        return currentSurfaceMode;
    }

    function isMapMode() {
        return currentSurfaceMode === "map";
    }

    fitCameraToCurrentSurface();

    return {
        fitCameraToCurrentSurface,
        setSurfaceMode,
        onResize,
        getCamera,
        getActiveControls,
        getSurfaceMode,
        isMapMode,
    };
}
