import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { normalizeCausalExplorationDemo } from "../../app/engine/causal-exploration-demo";
import { createCausalExplorationLayer } from "./causal-exploration-layer";

function projectClient(point: THREE.Vector3, camera: THREE.Camera, width: number, height: number) {
    const projected = point.clone().project(camera);
    return {
        clientX: ((projected.x + 1) * 0.5) * width,
        clientY: ((-projected.y + 1) * 0.5) * height,
    };
}

describe("causal exploration layer", () => {
    it("builds only the three configured traces and switches active trace on click", () => {
        const canvas = document.createElement("canvas");
        const viewportPanel = document.createElement("div");
        const overlay = document.createElement("section");
        Object.defineProperty(canvas, "getBoundingClientRect", {
            value: () => ({ left: 0, top: 0, width: 400, height: 300 }),
        });
        Object.defineProperty(viewportPanel, "getBoundingClientRect", {
            value: () => ({ left: 0, top: 0, width: 400, height: 300 }),
        });

        const layer = createCausalExplorationLayer({ canvas, viewportPanel, overlay });
        const demo = normalizeCausalExplorationDemo({
            demo_id: "border_mountain_plate_demo",
            features: [
                {
                    feature_id: "border",
                    feature_type: "border_segment",
                    label: "Border",
                    short_label: "B",
                    anchor: { x: 0, y: 0, z: 0 },
                    metrics: [{ metric_id: "m1", label: "整合", value: 0.8, unit: "ratio", display_value: "0.80" }],
                    uncertainty_stage: "medium",
                },
                {
                    feature_id: "mountain",
                    feature_type: "ridge_or_mountain_band",
                    label: "Mountain",
                    short_label: "M",
                    anchor: { x: 0.6, y: 0, z: 0 },
                    metrics: [{ metric_id: "m2", label: "標高差", value: 1.7, unit: "rel", display_value: "+1.7" }],
                    uncertainty_stage: "low",
                },
                {
                    feature_id: "plate",
                    feature_type: "tectonic_compression_or_plate_boundary",
                    label: "Plate",
                    short_label: "P",
                    anchor: { x: 0.6, y: 0.6, z: 0 },
                    metrics: [{ metric_id: "m3", label: "圧縮", value: 0.68, unit: "ratio", display_value: "0.68" }],
                    uncertainty_stage: "high",
                },
            ],
            trace_segments: [
                {
                    trace_id: "ridge_alignment",
                    label: "Ridge Alignment",
                    source_feature_id: "border",
                    target_feature_id: "mountain",
                    relation_type: "constraint_alignment",
                    path: [{ x: 0, y: 0, z: 0 }, { x: 0.6, y: 0, z: 0 }],
                    metrics: [{ metric_id: "m1", label: "整合", value: 0.8, unit: "ratio", display_value: "0.80" }],
                    uncertainty_stage: "medium",
                    evidence_ids: ["e1"],
                    display_key: "ridge_alignment",
                },
                {
                    trace_id: "passability_break",
                    label: "Passability Break",
                    source_feature_id: "border",
                    target_feature_id: "mountain",
                    relation_type: "geomorphic_structure",
                    path: [{ x: 0, y: 0, z: 0 }, { x: 0.3, y: 0.3, z: 0 }, { x: 0.6, y: 0, z: 0 }],
                    metrics: [{ metric_id: "m2", label: "通過", value: 0.31, unit: "ratio", display_value: "0.31" }],
                    uncertainty_stage: "medium",
                    evidence_ids: ["e2"],
                    display_key: "passability_break",
                },
                {
                    trace_id: "tectonic_driver",
                    label: "Tectonic Driver",
                    source_feature_id: "mountain",
                    target_feature_id: "plate",
                    relation_type: "tectonic_driver",
                    path: [{ x: 0.6, y: 0, z: 0 }, { x: 0.6, y: 0.6, z: 0 }],
                    metrics: [{ metric_id: "m3", label: "圧縮", value: 0.68, unit: "ratio", display_value: "0.68" }],
                    uncertainty_stage: "high",
                    evidence_ids: ["e3"],
                    display_key: "tectonic_driver",
                },
            ],
            metrics: [],
            display_mapping: {
                feature_styles: [
                    { feature_id: "border", color_hex: "#f7b267", glow_intensity: 0.8, pulse_hz: 0.5, radius: 0.04 },
                    { feature_id: "mountain", color_hex: "#7bc6cc", glow_intensity: 0.8, pulse_hz: 0.5, radius: 0.04 },
                    { feature_id: "plate", color_hex: "#f25f5c", glow_intensity: 0.8, pulse_hz: 0.5, radius: 0.04 },
                ],
                trace_styles: [
                    { trace_id: "ridge_alignment", color_hex: "#fff1b5", thickness: 0.01, flow_speed: 0.4, jitter_amplitude: 0.08, label_short: "整合 0.80" },
                    { trace_id: "passability_break", color_hex: "#9dd9d2", thickness: 0.01, flow_speed: 0.4, jitter_amplitude: 0.08, label_short: "通過 0.31" },
                    { trace_id: "tectonic_driver", color_hex: "#f28482", thickness: 0.01, flow_speed: 0.4, jitter_amplitude: 0.08, label_short: "圧縮 0.68" },
                ],
            },
            evidence: [
                { evidence_id: "e1", trace_id: "ridge_alignment", evidence_type: "morphology", summary: "", assumptions: [], approximations: [], uncertainty_reason: "u1", reference_model: "", reference_notes: "" },
                { evidence_id: "e2", trace_id: "passability_break", evidence_type: "passability_proxy", summary: "", assumptions: [], approximations: [], uncertainty_reason: "u2", reference_model: "", reference_notes: "" },
                { evidence_id: "e3", trace_id: "tectonic_driver", evidence_type: "tectonic_proxy", summary: "", assumptions: [], approximations: [], uncertainty_reason: "u3", reference_model: "", reference_notes: "" },
            ],
        });

        layer.setDemo(demo);
        const camera = new THREE.PerspectiveCamera(45, 400 / 300, 0.1, 10);
        camera.position.set(0, 0, 3);
        camera.lookAt(0, 0, 0);
        camera.updateProjectionMatrix();
        layer.update(camera);

        expect(layer.group.children).toHaveLength(9);
        expect(overlay.textContent).toContain("Ridge Alignment");
        expect(overlay.textContent).not.toContain("invented");

        const mountainPoint = new THREE.Vector3(0.6, 0, 0);
        const { clientX, clientY } = projectClient(mountainPoint, camera, 400, 300);
        layer.handlePointerDown(new PointerEvent("pointerdown", { clientX, clientY }));

        expect(overlay.textContent).toContain("Tectonic Driver");
        expect(overlay.textContent).toContain("tectonic_proxy");
    });
});
