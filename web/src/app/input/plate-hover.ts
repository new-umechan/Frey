import * as THREE from "three";
import { PLATE_HOVER_POPUP_DELAY_MS } from "../../shared/constants";
import { formatBiomeLabel, getCellMetricMeta } from "../visualizers/cell-metric";
import { type CoreBuffers } from "../sim/sync/types";

export interface PlateHoverController {
    hidePopup: () => void;
    updateFromPointer: (event: PointerEvent) => void;
}

interface PlateHoverState {
    currentTerrainData: CoreBuffers | null;
    currentViewMode: string;
    currentCellMetric: string;
    camera: THREE.Camera;
}

interface PendingPlateHover {
    clientX: number;
    clientY: number;
    vertexIndex: number;
}

interface MetricHoverValue {
    meta: { label: string; unit: string; formatter: (value: number) => string; dataKey: string };
    vertexIndex: number;
    value: number;
    formattedValue: string;
}

function formatWindDirection(u: number, v: number): string {
    const speed = Math.hypot(u, v);
    if (!Number.isFinite(speed) || speed < 1e-6) {
        return "無風";
    }
    const angle = Math.atan2(v, u);
    const degree = ((angle * 180) / Math.PI + 360) % 360;
    const points = ["→", "↗", "↑", "↖", "←", "↙", "↓", "↘"];
    const index = Math.round(degree / 45) % points.length;
    return `${points[index]} ${speed.toFixed(2)} m/s`;
}

export function createPlateHover({
    canvas,
    sphere,
    viewportPanel,
    plateHoverPopup,
    getState,
    onClimateHover,
}: {
    canvas: HTMLCanvasElement;
    sphere: THREE.Mesh;
    viewportPanel: HTMLElement;
    plateHoverPopup: HTMLElement;
    getState: () => PlateHoverState;
    onClimateHover?: (data: { label: string; value: string } | null) => void;
}): PlateHoverController {
    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();
    let plateHoverTimerId: number | null = null;
    let pendingPlateHover: PendingPlateHover | null = null;
    let visiblePlateHoverId: number | null = null;

    function clearPlateHoverTimer() {
        if (plateHoverTimerId !== null) {
            window.clearTimeout(plateHoverTimerId);
            plateHoverTimerId = null;
        }
    }

    function hidePopup() {
        clearPlateHoverTimer();
        pendingPlateHover = null;
        visiblePlateHoverId = null;
        plateHoverPopup.hidden = true;
        plateHoverPopup.textContent = "";
        onClimateHover?.(null);
    }

    function showPlateHoverPopup(clientX: number, clientY: number, vertexIndexValue: number) {
        const {
            currentTerrainData,
            currentViewMode,
            currentCellMetric,
        } = getState();
        if (!currentTerrainData || currentViewMode !== "metric") {
            hidePopup();
            return;
        }

        const metricHover = readMetricHoverValue(currentTerrainData, currentCellMetric, vertexIndexValue);
        if (!metricHover) {
            hidePopup();
            return;
        }
        const popupLines = [metricHover.meta.label];
        if (currentCellMetric === "plate_id") {
            const heightValue = (currentTerrainData.heightData as Float32Array | undefined)?.[metricHover.vertexIndex];
            popupLines.push(`plate: ${metricHover.formattedValue}`);
            popupLines.push(`cell: ${metricHover.vertexIndex}`);
            popupLines.push(`height: ${Number.isFinite(heightValue) ? Number(heightValue).toFixed(3) : "-"}`);
        } else {
            popupLines.push(`cell: ${metricHover.vertexIndex}`);
            popupLines.push(`value: ${metricHover.formattedValue}`);
        }
        plateHoverPopup.textContent = popupLines.join("\n");
        plateHoverPopup.hidden = false;
        positionPopup(clientX, clientY);
        visiblePlateHoverId = metricHover.vertexIndex;
        onClimateHover?.({
            label: metricHover.meta.label,
            value: metricHover.formattedValue,
        });
    }

    function positionPopup(clientX: number, clientY: number) {
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
    }

    function schedulePlateHoverPopup(clientX: number, clientY: number, vertexIndexValue: number) {
        const vertexIndex = Number(vertexIndexValue);
        if (!Number.isInteger(vertexIndex)) {
            hidePopup();
            return;
        }

        if (visiblePlateHoverId === vertexIndex && !plateHoverPopup.hidden) {
            showPlateHoverPopup(clientX, clientY, vertexIndex);
            return;
        }

        pendingPlateHover = {
            clientX,
            clientY,
            vertexIndex,
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
                vertexIndex: nextVertexIndex,
            } = pendingPlateHover;
            pendingPlateHover = null;
            showPlateHoverPopup(nextX, nextY, nextVertexIndex);
        }, PLATE_HOVER_POPUP_DELAY_MS);
    }

    function updateFromPointer(event: PointerEvent) {
        const {
            currentTerrainData,
            currentViewMode,
            camera,
            currentCellMetric,
        } = getState();
        if (!currentTerrainData || currentViewMode !== "metric") {
            hidePopup();
            return;
        }

        const rect = canvas.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) {
            hidePopup();
            return;
        }

        pointerNdc.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
        pointerNdc.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
        raycaster.setFromCamera(pointerNdc, camera);

        const [hit] = raycaster.intersectObject(sphere, false);
        const face = hit?.face;
        if (!face) {
            hidePopup();
            return;
        }

        const hoveredVertexIndex = face.a;
        if (!readMetricHoverValue(currentTerrainData, currentCellMetric, hoveredVertexIndex)) {
            hidePopup();
            return;
        }

        if (pendingPlateHover && pendingPlateHover.vertexIndex !== hoveredVertexIndex) {
            clearPlateHoverTimer();
            pendingPlateHover = null;
            plateHoverPopup.hidden = true;
            plateHoverPopup.textContent = "";
            visiblePlateHoverId = null;
        }

        pendingPlateHover = {
            clientX: event.clientX,
            clientY: event.clientY,
            vertexIndex: hoveredVertexIndex,
        };
        schedulePlateHoverPopup(
            event.clientX,
            event.clientY,
            hoveredVertexIndex,
        );
    }

    function readMetricHoverValue(currentTerrainData: CoreBuffers, currentCellMetric: string, vertexIndexValue: number): MetricHoverValue | null {
        const meta = getCellMetricMeta(currentCellMetric);
        const vertexIndex = Number(vertexIndexValue);
        if (!Number.isInteger(vertexIndex)) {
            return null;
        }
        if (currentCellMetric === "wind_direction") {
            const u = currentTerrainData.windU?.[vertexIndex];
            const v = currentTerrainData.windV?.[vertexIndex];
            if (!Number.isFinite(u) || !Number.isFinite(v)) {
                return null;
            }
            return {
                meta,
                vertexIndex,
                value: Math.hypot(u, v),
                formattedValue: formatWindDirection(u, v),
            };
        }
        if (currentCellMetric === "biome") {
            const raw = currentTerrainData.biome?.[vertexIndex];
            if (!Number.isFinite(raw)) {
                return null;
            }
            return {
                meta,
                vertexIndex,
                value: raw,
                formattedValue: formatBiomeLabel(raw),
            };
        }
        const values = currentTerrainData[meta.dataKey] as Float32Array | Int32Array | Uint32Array | undefined;
        const value = values?.[vertexIndex];
        if (value === undefined || !Number.isFinite(value)) {
            return null;
        }
        return {
            meta,
            vertexIndex,
            value,
            formattedValue: meta.formatter(value),
        };
    }

    return {
        hidePopup,
        updateFromPointer,
    };
}
