import * as THREE from "three";
import { PLATE_HOVER_POPUP_DELAY_MS } from "../core/constants.js";

export function createPlateHoverController({
    canvas,
    sphere,
    geometry,
    viewportPanel,
    plateHoverPopup,
    getState,
}) {
    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();
    const hoverLocalPoint = new THREE.Vector3();
    const hoverTriA = new THREE.Vector3();
    const hoverTriB = new THREE.Vector3();
    const hoverTriC = new THREE.Vector3();
    const hoverBarycoord = new THREE.Vector3();
    let plateHoverTimerId = null;
    let pendingPlateHover = null;
    let visiblePlateHoverId = null;

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
    }

    function showPlateHoverPopup(clientX, clientY, plateIdValue, hoverDiagnostics) {
        const { currentTerrainData, currentViewMode, debugEnabled } = getState();
        if (!currentTerrainData || currentViewMode !== "plates") {
            hidePopup();
            return;
        }

        const plateIndex = Number(plateIdValue);
        const { plateInfo } = currentTerrainData;
        if (
            !Number.isInteger(plateIndex) ||
            plateIndex < 0 ||
            plateIndex >= plateInfo.isOcean.length
        ) {
            hidePopup();
            return;
        }

        const plateKind = plateInfo.isOcean[plateIndex] ? "海洋プレート" : "大陸プレート";
        const weight = Number.isFinite(hoverDiagnostics?.weight)
            ? hoverDiagnostics.weight
            : plateInfo.baseWeight[plateIndex];
        const height = plateInfo.baseHeight[plateIndex];
        const debugLines = debugEnabled ? (hoverDiagnostics?.debugLines ?? []) : [];
        plateHoverPopup.textContent = [
            `Plate #${plateIndex}`,
            plateKind,
            `weight: ${weight.toFixed(3)}`,
            `height: ${height.toFixed(3)}`,
            ...debugLines,
        ].join("\n");
        plateHoverPopup.hidden = false;

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
        visiblePlateHoverId = plateIndex;
    }

    function schedulePlateHoverPopup(clientX, clientY, plateIdValue, hoverDiagnostics) {
        const plateIndex = Number(plateIdValue);
        if (!Number.isInteger(plateIndex)) {
            hidePopup();
            return;
        }

        if (visiblePlateHoverId === plateIndex && !plateHoverPopup.hidden) {
            showPlateHoverPopup(clientX, clientY, plateIndex, hoverDiagnostics);
            return;
        }

        pendingPlateHover = {
            clientX,
            clientY,
            plateId: plateIndex,
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
                plateId: nextPlateId,
                hoverDiagnostics: nextHoverDiagnostics,
            } = pendingPlateHover;
            pendingPlateHover = null;
            showPlateHoverPopup(nextX, nextY, nextPlateId, nextHoverDiagnostics);
        }, PLATE_HOVER_POPUP_DELAY_MS);
    }

    function sampleHoverWeight(hit, plateIndexFallback) {
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

        const weightA = currentTerrainData.vertexWeight[face.a];
        const weightB = currentTerrainData.vertexWeight[face.b];
        const weightC = currentTerrainData.vertexWeight[face.c];
        const plateA = currentTerrainData.plateId[face.a];
        const plateB = currentTerrainData.plateId[face.b];
        const plateC = currentTerrainData.plateId[face.c];
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

        if (samePlateA && samePlateB && samePlateC) {
            return {
                weight:
                    hoverBarycoord.x * weightA +
                    hoverBarycoord.y * weightB +
                    hoverBarycoord.z * weightC,
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
        if (samePlateA && Number.isFinite(weightA)) {
            sum += hoverBarycoord.x * weightA;
            wsum += hoverBarycoord.x;
        }
        if (samePlateB && Number.isFinite(weightB)) {
            sum += hoverBarycoord.y * weightB;
            wsum += hoverBarycoord.y;
        }
        if (samePlateC && Number.isFinite(weightC)) {
            sum += hoverBarycoord.z * weightC;
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

    function updateFromPointer(event) {
        const {
            currentTerrainData,
            currentViewMode,
            currentSurfaceMode,
            camera,
        } = getState();
        if (!currentTerrainData || currentViewMode !== "plates" || currentSurfaceMode !== "globe") {
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
        const hoveredPlateId = currentTerrainData.plateId[hoveredVertexIndex];
        const hoveredPlateIndex = Number(hoveredPlateId);
        if (!Number.isInteger(hoveredPlateIndex)) {
            hidePopup();
            return;
        }

        if (pendingPlateHover && pendingPlateHover.plateId !== hoveredPlateIndex) {
            clearPlateHoverTimer();
            pendingPlateHover = null;
            plateHoverPopup.hidden = true;
            plateHoverPopup.textContent = "";
            visiblePlateHoverId = null;
        }

        const sampledWeightResult = sampleHoverWeight(hit, hoveredPlateIndex);
        const sampledWeight = Number.isFinite(sampledWeightResult?.weight)
            ? sampledWeightResult.weight
            : currentTerrainData.vertexWeight[hoveredVertexIndex];
        const hoverDiagnostics = {
            weight: sampledWeight,
            debugLines: [
                ...(sampledWeightResult?.debugLines ?? ["debug: source=unknown"]),
                `debug: faceAWeight=${currentTerrainData.vertexWeight[hoveredVertexIndex].toFixed(3)}`,
            ],
        };

        pendingPlateHover = {
            clientX: event.clientX,
            clientY: event.clientY,
            plateId: hoveredPlateIndex,
            hoverDiagnostics,
        };
        schedulePlateHoverPopup(
            event.clientX,
            event.clientY,
            hoveredPlateIndex,
            hoverDiagnostics,
        );
    }

    function syncDebugMode() {
        if (!plateHoverPopup.hidden && pendingPlateHover) {
            showPlateHoverPopup(
                pendingPlateHover.clientX,
                pendingPlateHover.clientY,
                pendingPlateHover.plateId,
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
