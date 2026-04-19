import * as THREE from "three";
import { PLATE_HOVER_POPUP_DELAY_MS } from "../../shared/constants";
import { formatBiomeLabel, getCellMetricMeta } from "../visualizers/cell-metric";
import { type CoreBuffers } from "../sim/sync/types";

export interface PlateHoverController {
    hidePopup: () => void;
    updateFromPointer: (event: PointerEvent) => void;
    syncDebugMode: () => void;
}

interface PlateHoverState {
    currentTerrainData: CoreBuffers | null;
    currentViewMode: string;
    currentCellMetric: string;
    debugEnabled: boolean;
    camera: THREE.Camera;
}

interface HoverDiagnostics {
    weight: number | null;
    debugLines: string[];
}

interface PendingPlateHover {
    clientX: number;
    clientY: number;
    vertexIndex: number;
    hoverDiagnostics: HoverDiagnostics;
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
    geometry,
    viewportPanel,
    plateHoverPopup,
    getState,
    onClimateHover,
}: {
    canvas: HTMLCanvasElement;
    sphere: THREE.Mesh;
    geometry: THREE.BufferGeometry;
    viewportPanel: HTMLElement;
    plateHoverPopup: HTMLElement;
    getState: () => PlateHoverState;
    onClimateHover?: (data: { label: string; value: string } | null) => void;
}): PlateHoverController {
    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();
    const hoverLocalPoint = new THREE.Vector3();
    const hoverTriA = new THREE.Vector3();
    const hoverTriB = new THREE.Vector3();
    const hoverTriC = new THREE.Vector3();
    const hoverBarycoord = new THREE.Vector3();
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

    function showPlateHoverPopup(clientX: number, clientY: number, vertexIndexValue: number, hoverDiagnostics: HoverDiagnostics) {
        const {
            currentTerrainData,
            currentViewMode,
            currentCellMetric,
            debugEnabled,
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
        if (debugEnabled) {
            popupLines.push(...(hoverDiagnostics?.debugLines ?? []));
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

    function schedulePlateHoverPopup(clientX: number, clientY: number, vertexIndexValue: number, hoverDiagnostics: HoverDiagnostics) {
        const vertexIndex = Number(vertexIndexValue);
        if (!Number.isInteger(vertexIndex)) {
            hidePopup();
            return;
        }

        if (visiblePlateHoverId === vertexIndex && !plateHoverPopup.hidden) {
            showPlateHoverPopup(clientX, clientY, vertexIndex, hoverDiagnostics);
            return;
        }

        pendingPlateHover = {
            clientX,
            clientY,
            vertexIndex,
            hoverDiagnostics,
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
                hoverDiagnostics: nextHoverDiagnostics,
            } = pendingPlateHover;
            pendingPlateHover = null;
            showPlateHoverPopup(nextX, nextY, nextVertexIndex, nextHoverDiagnostics);
        }, PLATE_HOVER_POPUP_DELAY_MS);
    }

    function sampleHoverWeight(hit: THREE.Intersection, plateIndexFallback: number) {
        const { currentTerrainData } = getState();
        const face = hit?.face;
        const positionAttr = geometry.getAttribute("position");
        if (
            !face ||
            !positionAttr ||
            !currentTerrainData
        ) {
            return {
                weight: null,
                source: "invalid-hit",
                debugLines: ["debug: source=invalid-hit"],
            };
        }

        hoverTriA.fromBufferAttribute(positionAttr, face.a);
        hoverTriB.fromBufferAttribute(positionAttr, face.b);
        hoverTriC.fromBufferAttribute(positionAttr, face.c);
        hoverLocalPoint.copy(hit.point);
        sphere.worldToLocal(hoverLocalPoint);
        const bary = THREE.Triangle.getBarycoord(
            hoverLocalPoint,
            hoverTriA,
            hoverTriB,
            hoverTriC,
            hoverBarycoord,
        );

        const weightA = (currentTerrainData.vertexWeight as Float32Array | undefined)?.[face.a];
        const weightB = (currentTerrainData.vertexWeight as Float32Array | undefined)?.[face.b];
        const weightC = (currentTerrainData.vertexWeight as Float32Array | undefined)?.[face.c];
        const plateA = (currentTerrainData.plateId as Uint32Array | undefined)?.[face.a];
        const plateB = (currentTerrainData.plateId as Uint32Array | undefined)?.[face.b];
        const plateC = (currentTerrainData.plateId as Uint32Array | undefined)?.[face.c];
        const fallbackVertexWeight = weightA;

        const baseDebugLines = [
            `debug: face=(${face.a},${face.b},${face.c})`,
            `debug: vw=(${Number(weightA).toFixed(3)},${Number(weightB).toFixed(3)},${Number(weightC).toFixed(3)})`,
            `debug: pid=(${Number(plateA)},${Number(plateB)},${Number(plateC)})`,
        ];

        if (
            !bary ||
            !Number.isFinite(hoverBarycoord.x) ||
            !Number.isFinite(hoverBarycoord.y) ||
            !Number.isFinite(hoverBarycoord.z)
        ) {
            if (Number.isFinite(weightA)) {
                return {
                    weight: weightA,
                    source: "vertex-fallback-a",
                    debugLines: [
                        ...baseDebugLines,
                        "debug: source=vertex-fallback-a",
                        "debug: bary=invalid",
                    ],
                };
            }
            if (Number.isFinite(weightB)) {
                return {
                    weight: weightB,
                    source: "vertex-fallback-b",
                    debugLines: [
                        ...baseDebugLines,
                        "debug: source=vertex-fallback-b",
                        "debug: bary=invalid",
                    ],
                };
            }
            if (Number.isFinite(weightC)) {
                return {
                    weight: weightC,
                    source: "vertex-fallback-c",
                    debugLines: [
                        ...baseDebugLines,
                        "debug: source=vertex-fallback-c",
                        "debug: bary=invalid",
                    ],
                };
            }
            return {
                weight: null,
                source: "weight-invalid",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=weight-invalid",
                    "debug: bary=invalid",
                ],
            };
        }

        const targetPlate = Number.isInteger(plateIndexFallback)
            ? plateIndexFallback
            : Number(plateA);
        const samePlateA = Number(plateA) === targetPlate;
        const samePlateB = Number(plateB) === targetPlate;
        const samePlateC = Number(plateC) === targetPlate;
        const safeWeightA = Number(weightA);
        const safeWeightB = Number(weightB);
        const safeWeightC = Number(weightC);
        const finiteWeightA = Number.isFinite(safeWeightA);
        const finiteWeightB = Number.isFinite(safeWeightB);
        const finiteWeightC = Number.isFinite(safeWeightC);

        const allPlateWeightsFinite = finiteWeightA && finiteWeightB && finiteWeightC;
        if (samePlateA && samePlateB && samePlateC && allPlateWeightsFinite) {
            return {
                weight:
                    hoverBarycoord.x * safeWeightA +
                    hoverBarycoord.y * safeWeightB +
                    hoverBarycoord.z * safeWeightC,
                source: "interp-all",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=interp-all",
                    `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                ],
            };
        }

        let sum = 0;
        let wsum = 0;
        if (samePlateA && finiteWeightA) {
            sum += hoverBarycoord.x * safeWeightA;
            wsum += hoverBarycoord.x;
        }
        if (samePlateB && finiteWeightB) {
            sum += hoverBarycoord.y * safeWeightB;
            wsum += hoverBarycoord.y;
        }
        if (samePlateC && finiteWeightC) {
            sum += hoverBarycoord.z * safeWeightC;
            wsum += hoverBarycoord.z;
        }
        if (wsum > 1e-6) {
            return {
                weight: sum / wsum,
                source: "interp-same-plate-only",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=interp-same-plate-only",
                    `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                    `debug: wsum=${wsum.toFixed(3)}`,
                ],
            };
        }

        if (Number.isFinite(fallbackVertexWeight)) {
            return {
                weight: fallbackVertexWeight,
                source: "vertex-fallback-final",
                debugLines: [
                    ...baseDebugLines,
                    "debug: source=vertex-fallback-final",
                    `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                    `debug: wsum=${wsum.toFixed(3)}`,
                ],
            };
        }

        return {
            weight: null,
            source: "plate-fallback",
            debugLines: [
                ...baseDebugLines,
                "debug: source=plate-fallback",
                `debug: bary=(${hoverBarycoord.x.toFixed(3)},${hoverBarycoord.y.toFixed(3)},${hoverBarycoord.z.toFixed(3)})`,
                `debug: wsum=${wsum.toFixed(3)}`,
            ],
        };
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

        const hoveredPlateId = (currentTerrainData.plateId as Uint32Array | undefined)?.[hoveredVertexIndex];
        const sampledWeightResult = sampleHoverWeight(
            hit,
            Number.isInteger(hoveredPlateId) ? Number(hoveredPlateId) : hoveredVertexIndex,
        );
        const fallbackFaceWeight = (currentTerrainData.vertexWeight as Float32Array | undefined)?.[hoveredVertexIndex];
        const sampledWeight = Number.isFinite(sampledWeightResult?.weight)
            ? Number(sampledWeightResult.weight)
            : Number.isFinite(fallbackFaceWeight)
                ? Number(fallbackFaceWeight)
                : null;
        const hoverDiagnostics: HoverDiagnostics = {
            weight: sampledWeight,
            debugLines: [
                ...(sampledWeightResult?.debugLines ?? ["debug: source=unknown"]),
                `debug: faceAWeight=${Number.isFinite(fallbackFaceWeight) ? Number(fallbackFaceWeight).toFixed(3) : "-"}`,
            ],
        };

        pendingPlateHover = {
            clientX: event.clientX,
            clientY: event.clientY,
            vertexIndex: hoveredVertexIndex,
            hoverDiagnostics,
        };
        schedulePlateHoverPopup(
            event.clientX,
            event.clientY,
            hoveredVertexIndex,
            hoverDiagnostics,
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

    function syncDebugMode() {
        if (!plateHoverPopup.hidden && pendingPlateHover) {
            showPlateHoverPopup(
                pendingPlateHover.clientX,
                pendingPlateHover.clientY,
                pendingPlateHover.vertexIndex,
                pendingPlateHover.hoverDiagnostics,
            );
            return;
        }

        if (!plateHoverPopup.hidden) {
            hidePopup();
        }
    }

    return {
        hidePopup,
        updateFromPointer,
        syncDebugMode,
    };
}
