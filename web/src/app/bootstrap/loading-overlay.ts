interface LoadingOverlayOptions {
    loadingOverlayCanvas: HTMLCanvasElement;
}

export function createLoadingOverlayController(options: LoadingOverlayOptions) {
    const {
        loadingOverlayCanvas,
    } = options;
    const loadingOverlayContextRaw = loadingOverlayCanvas.getContext("2d");
    if (!loadingOverlayContextRaw) {
        throw new Error("loading overlay canvas context is unavailable");
    }
    const loadingOverlayContext: CanvasRenderingContext2D = loadingOverlayContextRaw;

    function syncVisibility() {
        loadingOverlayCanvas.hidden = true;
        loadingOverlayCanvas.setAttribute("aria-hidden", "true");
    }

    function syncCanvasSize() {
        const panelWidth = Math.max(1, loadingOverlayCanvas.width || 1);
        const panelHeight = Math.max(1, loadingOverlayCanvas.height || 1);
        const dpr = Math.min(window.devicePixelRatio || 1, 2);
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

    function render() {
        clear();
        syncVisibility();
    }

    function setWorldInitializing(_value: boolean) {
        syncVisibility();
    }

    syncVisibility();

    return {
        clear,
        render,
        setWorldInitializing,
    };
}
