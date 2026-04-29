import { describe, expect, it } from "vitest";
import { normalizeCausalExplorationDemo } from "./causal-exploration-demo";

describe("causal exploration demo normalization", () => {
    it("indexes features, traces, and evidence without generating extra traces", () => {
        const demo = normalizeCausalExplorationDemo({
            demo_id: "border_mountain_plate_demo",
            features: [
                {
                    feature_id: "border",
                    feature_type: "border_segment",
                    label: "Border",
                    short_label: "B",
                    anchor: { x: 1, y: 0, z: 0 },
                    metrics: [],
                    uncertainty_stage: "medium",
                },
                {
                    feature_id: "mountain",
                    feature_type: "ridge_or_mountain_band",
                    label: "Mountain",
                    short_label: "M",
                    anchor: { x: 0, y: 1, z: 0 },
                    metrics: [],
                    uncertainty_stage: "low",
                },
            ],
            trace_segments: [
                {
                    trace_id: "ridge_alignment",
                    label: "Ridge Alignment",
                    source_feature_id: "border",
                    target_feature_id: "mountain",
                    relation_type: "constraint_alignment",
                    path: [{ x: 1, y: 0, z: 0 }, { x: 0, y: 1, z: 0 }],
                    metrics: [],
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
                    path: [{ x: 1, y: 0, z: 0 }, { x: 0.5, y: 0.5, z: 0 }],
                    metrics: [],
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
                    path: [{ x: 0, y: 1, z: 0 }, { x: 0, y: 0, z: 1 }],
                    metrics: [],
                    uncertainty_stage: "high",
                    evidence_ids: ["e3"],
                    display_key: "tectonic_driver",
                },
            ],
            metrics: [],
            display_mapping: {
                feature_styles: [],
                trace_styles: [
                    {
                        trace_id: "ridge_alignment",
                        color_hex: "#fff",
                        thickness: 0.1,
                        flow_speed: 0.2,
                        jitter_amplitude: 0.3,
                        label_short: "整合 0.82",
                    },
                ],
            },
            evidence: [
                {
                    evidence_id: "e1",
                    trace_id: "ridge_alignment",
                    evidence_type: "morphology",
                    summary: "",
                    assumptions: [],
                    approximations: [],
                    uncertainty_reason: "u1",
                    reference_model: "",
                    reference_notes: "",
                },
                {
                    evidence_id: "e2",
                    trace_id: "passability_break",
                    evidence_type: "passability_proxy",
                    summary: "",
                    assumptions: [],
                    approximations: [],
                    uncertainty_reason: "u2",
                    reference_model: "",
                    reference_notes: "",
                },
                {
                    evidence_id: "e3",
                    trace_id: "tectonic_driver",
                    evidence_type: "tectonic_proxy",
                    summary: "",
                    assumptions: [],
                    approximations: [],
                    uncertainty_reason: "u3",
                    reference_model: "",
                    reference_notes: "",
                },
            ],
        });

        expect(demo.trace_by_id.size).toBe(3);
        expect(demo.evidence_by_trace_id.get("ridge_alignment")).toHaveLength(1);
        expect(demo.trace_ids_by_feature_id.get("border")).toEqual([
            "ridge_alignment",
            "passability_break",
        ]);
        expect([...demo.trace_by_id.keys()]).not.toContain("invented_trace");
    });
});
