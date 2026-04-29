import * as THREE from "three";
import type { Camera } from "three";
import type { NormalizedCausalExplorationDemo } from "../../app/engine/causal-exploration-demo";
import type { CausalFeatureDescriptor, CausalTraceSegment } from "../../app/engine/engine-client";

export interface CausalExplorationLayer {
    group: THREE.Group;
    setDemo: (demo: NormalizedCausalExplorationDemo | null) => void;
    update: (camera: Camera) => void;
    handlePointerMove: (event: PointerEvent) => void;
    handlePointerDown: (event: PointerEvent) => void;
    handlePointerLeave: () => void;
}

type TraceVisual = {
    trace: CausalTraceSegment;
    line: THREE.Line;
    bead: THREE.Mesh;
    points: THREE.Vector3[];
};

function hexOrFallback(value: string | undefined, fallback: string) {
    return value && value.length > 0 ? value : fallback;
}

function buildLabelText(trace: CausalTraceSegment) {
    const metric = trace.metrics[0];
    return metric ? `${metric.label} ${metric.display_value}` : trace.label;
}

export function createCausalExplorationLayer({
    canvas,
    viewportPanel,
    overlay,
}: {
    canvas: HTMLCanvasElement;
    viewportPanel: HTMLElement;
    overlay: HTMLElement;
}): CausalExplorationLayer {
    const group = new THREE.Group();
    const raycaster = new THREE.Raycaster();
    raycaster.params.Line = { threshold: 0.06 };
    const pointerNdc = new THREE.Vector2();
    const projectedPoint = new THREE.Vector3();
    const featureMeshes = new Map<string, THREE.Mesh>();
    const traceVisuals = new Map<string, TraceVisual>();
    const interactiveObjects: THREE.Object3D[] = [];
    const traceMeshByObjectId = new Map<number, string>();
    const featureIdByObjectId = new Map<number, string>();
    const labelElement = document.createElement("button");
    const panelTitle = document.createElement("p");
    const panelBody = document.createElement("p");
    let demo: NormalizedCausalExplorationDemo | null = null;
    let hoveredFeatureId: string | null = null;
    let activeFeatureId: string | null = null;
    let activeTraceId: string | null = null;

    labelElement.type = "button";
    labelElement.className = "causal-label";
    labelElement.hidden = true;
    overlay.append(labelElement);
    overlay.hidden = true;
    panelTitle.className = "causal-panel-title";
    panelBody.className = "causal-panel-body";
    overlay.replaceChildren(panelTitle, panelBody, labelElement);

    function clearLayer() {
        for (const child of [...group.children]) {
            group.remove(child);
        }
        featureMeshes.clear();
        traceVisuals.clear();
        interactiveObjects.length = 0;
        traceMeshByObjectId.clear();
        featureIdByObjectId.clear();
        hoveredFeatureId = null;
        activeFeatureId = null;
        activeTraceId = null;
        labelElement.hidden = true;
        overlay.hidden = true;
        panelTitle.textContent = "";
        panelBody.textContent = "";
    }

    function renderPanel() {
        if (!demo || !activeTraceId) {
            overlay.hidden = true;
            labelElement.hidden = true;
            return;
        }
        const trace = demo.trace_by_id.get(activeTraceId);
        if (!trace) {
            overlay.hidden = true;
            labelElement.hidden = true;
            return;
        }
        const evidence = demo.evidence_by_trace_id.get(trace.trace_id)?.[0] ?? null;
        overlay.hidden = false;
        panelTitle.textContent = `${trace.label} | ${buildLabelText(trace)}`;
        panelBody.textContent = evidence
            ? `${evidence.evidence_type} | ${evidence.uncertainty_reason}`
            : trace.metrics.map((metric) => `${metric.label} ${metric.display_value}`).join(" | ");
    }

    function applyFeatureState(timeSeconds: number) {
        for (const [featureId, mesh] of featureMeshes) {
            const style = demo?.feature_style_by_id.get(featureId);
            const material = mesh.material as THREE.MeshBasicMaterial;
            const isActive = featureId === activeFeatureId;
            const isHover = featureId === hoveredFeatureId;
            const baseOpacity = isActive ? 0.95 : isHover ? 0.82 : 0.55;
            const pulse = style ? 0.84 + Math.sin(timeSeconds * style.pulse_hz * Math.PI * 2) * 0.16 : 1;
            material.opacity = Math.min(1, Math.max(0.18, baseOpacity * pulse));
            const baseScale = style?.radius ?? 0.04;
            const scale = isActive ? 1.65 : isHover ? 1.35 : 1.0;
            mesh.scale.setScalar(baseScale * scale);
        }
    }

    function applyTraceState(timeSeconds: number) {
        for (const [traceId, visual] of traceVisuals) {
            const style = demo?.trace_style_by_id.get(traceId);
            const lineMaterial = visual.line.material as THREE.LineBasicMaterial;
            const beadMaterial = visual.bead.material as THREE.MeshBasicMaterial;
            const isActive = traceId === activeTraceId;
            const isLinked = !isActive && activeFeatureId !== null && (
                visual.trace.source_feature_id === activeFeatureId
                || visual.trace.target_feature_id === activeFeatureId
            );
            lineMaterial.opacity = isActive ? 0.98 : isLinked ? 0.62 : 0.28;
            beadMaterial.opacity = isActive ? 0.92 : isLinked ? 0.58 : 0.18;
            const points = visual.points;
            if (points.length >= 2) {
                const speed = Math.max(style?.flow_speed ?? 0.35, 0.05);
                const progress = (timeSeconds * speed) % 1;
                const scaled = progress * (points.length - 1);
                const index = Math.floor(scaled);
                const nextIndex = Math.min(index + 1, points.length - 1);
                const alpha = scaled - index;
                visual.bead.position.lerpVectors(points[index], points[nextIndex], alpha);
                const jitter = style?.jitter_amplitude ?? 0;
                visual.bead.scale.setScalar(0.015 + jitter * 0.01);
            }
        }
    }

    function updateLabel(camera: Camera) {
        if (!demo) {
            labelElement.hidden = true;
            return;
        }
        const featureId = hoveredFeatureId ?? activeFeatureId;
        if (!featureId) {
            labelElement.hidden = true;
            return;
        }
        const feature = demo.feature_by_id.get(featureId);
        const mesh = feature ? featureMeshes.get(featureId) : null;
        if (!feature || !mesh) {
            labelElement.hidden = true;
            return;
        }
        projectedPoint.copy(mesh.position).project(camera);
        if (projectedPoint.z < -1 || projectedPoint.z > 1) {
            labelElement.hidden = true;
            return;
        }
        const rect = viewportPanel.getBoundingClientRect();
        const left = ((projectedPoint.x + 1) * 0.5) * rect.width;
        const top = ((-projectedPoint.y + 1) * 0.5) * rect.height;
        const traceLabel = activeTraceId ? demo.trace_style_by_id.get(activeTraceId)?.label_short : null;
        const featureMetric = feature.metrics[0]?.display_value ?? feature.short_label;
        labelElement.textContent = traceLabel
            ? `${feature.short_label} | ${traceLabel}`
            : `${feature.short_label} | ${featureMetric}`;
        labelElement.style.left = `${left}px`;
        labelElement.style.top = `${top}px`;
        labelElement.hidden = false;
    }

    function findPreferredTraceId(featureId: string): string | null {
        if (!demo) {
            return null;
        }
        const outgoing = demo.trace_segments.find((trace) => trace.source_feature_id === featureId);
        if (outgoing) {
            return outgoing.trace_id;
        }
        const traceIds = demo.trace_ids_by_feature_id.get(featureId) ?? [];
        return traceIds[0] ?? null;
    }

    function setActiveTrace(traceId: string | null) {
        activeTraceId = traceId;
        if (demo && traceId) {
            const trace = demo.trace_by_id.get(traceId);
            if (trace) {
                activeFeatureId = trace.target_feature_id;
            }
        }
        renderPanel();
    }

    function updatePointer(event: PointerEvent) {
        const rect = canvas.getBoundingClientRect();
        pointerNdc.set(
            ((event.clientX - rect.left) / rect.width) * 2 - 1,
            -(((event.clientY - rect.top) / rect.height) * 2 - 1),
        );
    }

    function intersect(camera: Camera) {
        group.updateMatrixWorld(true);
        raycaster.setFromCamera(pointerNdc, camera);
        return raycaster.intersectObjects(interactiveObjects, false);
    }

    function rebuildLayer(nextDemo: NormalizedCausalExplorationDemo) {
        clearLayer();
        demo = nextDemo;
        for (const feature of nextDemo.features) {
            const style = nextDemo.feature_style_by_id.get(feature.feature_id);
            const geometry = new THREE.SphereGeometry(1, 14, 14);
            const material = new THREE.MeshBasicMaterial({
                color: hexOrFallback(style?.color_hex, "#ffe29a"),
                transparent: true,
                opacity: 0.7,
            });
            const mesh = new THREE.Mesh(geometry, material);
            mesh.position.set(feature.anchor.x, feature.anchor.y, feature.anchor.z);
            mesh.scale.setScalar(style?.radius ?? 0.04);
            group.add(mesh);
            featureMeshes.set(feature.feature_id, mesh);
            interactiveObjects.push(mesh);
            featureIdByObjectId.set(mesh.id, feature.feature_id);
        }

        for (const trace of nextDemo.trace_segments) {
            const style = nextDemo.trace_style_by_id.get(trace.trace_id);
            const points = trace.path.map((entry) => new THREE.Vector3(entry.x, entry.y, entry.z));
            const geometry = new THREE.BufferGeometry().setFromPoints(points);
            const material = new THREE.LineBasicMaterial({
                color: hexOrFallback(style?.color_hex, "#b8d0ff"),
                transparent: true,
                opacity: 0.4,
            });
            const line = new THREE.Line(geometry, material);
            group.add(line);
            interactiveObjects.push(line);
            traceMeshByObjectId.set(line.id, trace.trace_id);

            const bead = new THREE.Mesh(
                new THREE.SphereGeometry(1, 10, 10),
                new THREE.MeshBasicMaterial({
                    color: hexOrFallback(style?.color_hex, "#ffffff"),
                    transparent: true,
                    opacity: 0.5,
                }),
            );
            bead.scale.setScalar(0.018);
            bead.position.copy(points[0]);
            group.add(bead);
            interactiveObjects.push(bead);
            traceMeshByObjectId.set(bead.id, trace.trace_id);

            traceVisuals.set(trace.trace_id, { trace, line, bead, points });
        }

        activeFeatureId = nextDemo.features[0]?.feature_id ?? null;
        activeTraceId = nextDemo.trace_segments[0]?.trace_id ?? null;
        renderPanel();
        applyFeatureState(0);
    }

    function getCurrentCamera() {
        const runtimeCamera = (group.userData.getCamera as (() => Camera) | undefined)?.();
        return runtimeCamera ?? null;
    }

    labelElement.addEventListener("click", () => {
        if (!activeTraceId || !demo) {
            return;
        }
        const trace = demo.trace_by_id.get(activeTraceId);
        if (!trace) {
            return;
        }
        activeFeatureId = trace.target_feature_id;
        applyFeatureState(0);
        renderPanel();
    });

    return {
        group,
        setDemo(nextDemo) {
            demo = nextDemo;
            if (!nextDemo) {
                clearLayer();
                return;
            }
            rebuildLayer(nextDemo);
        },
        update(camera) {
            group.userData.getCamera = () => camera;
            group.updateMatrixWorld(true);
            const nowSeconds = performance.now() / 1000;
            applyFeatureState(nowSeconds);
            applyTraceState(nowSeconds);
            updateLabel(camera);
        },
        handlePointerMove(event) {
            const camera = getCurrentCamera();
            if (!camera || !demo) {
                return;
            }
            updatePointer(event);
            const hit = intersect(camera)[0];
            if (!hit) {
                hoveredFeatureId = null;
                applyFeatureState(0);
                return;
            }
            const featureId = featureIdByObjectId.get(hit.object.id);
            if (featureId) {
                hoveredFeatureId = featureId;
                applyFeatureState(0);
            }
        },
        handlePointerDown(event) {
            const camera = getCurrentCamera();
            if (!camera || !demo) {
                return;
            }
            updatePointer(event);
            const hit = intersect(camera)[0];
            if (!hit) {
                return;
            }
            const featureId = featureIdByObjectId.get(hit.object.id);
            if (featureId) {
                activeFeatureId = featureId;
                setActiveTrace(findPreferredTraceId(featureId));
                applyFeatureState(0);
                return;
            }
            const traceId = traceMeshByObjectId.get(hit.object.id);
            if (traceId) {
                setActiveTrace(traceId);
                applyFeatureState(0);
            }
        },
        handlePointerLeave() {
            hoveredFeatureId = null;
            applyFeatureState(0);
            if (!activeFeatureId) {
                labelElement.hidden = true;
            }
        },
    };
}
