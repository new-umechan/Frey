#![cfg(feature = "wasm_transport")]

use crate::application::world_dto::{
    CausalDisplayFeatureStyle, CausalDisplayMapping, CausalDisplayTraceStyle, CausalEvidenceEntry,
    CausalExplorationDemoResponse, CausalFeatureDescriptor, CausalFeatureType, CausalLocationPoint,
    CausalMetricValue, CausalRelationType, CausalTraceSegment, EvidenceType, UncertaintyStage,
};
use crate::application::world_service::WorldService;

fn world_not_found_error(world_id: &str) -> String {
    format!("world not found: {world_id}")
}

fn point(x: f32, y: f32, z: f32) -> CausalLocationPoint {
    CausalLocationPoint { x, y, z }
}

fn metric(
    metric_id: &str,
    label: &str,
    value: f32,
    unit: &str,
    display_value: &str,
) -> CausalMetricValue {
    CausalMetricValue {
        metric_id: metric_id.to_string(),
        label: label.to_string(),
        value,
        unit: unit.to_string(),
        display_value: display_value.to_string(),
    }
}

fn build_demo_response() -> CausalExplorationDemoResponse {
    let features = vec![
        CausalFeatureDescriptor {
            feature_id: "border_segment_demo".to_string(),
            feature_type: CausalFeatureType::BorderSegment,
            label: "Border Segment".to_string(),
            short_label: "B".to_string(),
            anchor: point(0.54, 0.69, 0.47),
            metrics: vec![
                metric("alignment_score", "整合", 0.82, "ratio", "0.82"),
                metric("passability_index", "通過", 0.31, "ratio", "0.31"),
            ],
            uncertainty_stage: UncertaintyStage::Medium,
        },
        CausalFeatureDescriptor {
            feature_id: "mountain_band_demo".to_string(),
            feature_type: CausalFeatureType::RidgeOrMountainBand,
            label: "Mountain Band".to_string(),
            short_label: "M".to_string(),
            anchor: point(0.46, 0.78, 0.41),
            metrics: vec![
                metric("relief_delta", "標高差", 1.7, "rel", "+1.7"),
                metric("ridge_continuity", "稜線", 0.76, "ratio", "0.76"),
            ],
            uncertainty_stage: UncertaintyStage::Low,
        },
        CausalFeatureDescriptor {
            feature_id: "plate_driver_demo".to_string(),
            feature_type: CausalFeatureType::TectonicCompressionOrPlateBoundary,
            label: "Compression Driver".to_string(),
            short_label: "P".to_string(),
            anchor: point(0.33, 0.86, 0.38),
            metrics: vec![
                metric("compression_strength", "圧縮", 0.68, "ratio", "0.68"),
                metric("convergence_bias", "収束", 0.59, "ratio", "0.59"),
            ],
            uncertainty_stage: UncertaintyStage::High,
        },
    ];

    let trace_segments = vec![
        CausalTraceSegment {
            trace_id: "ridge_alignment".to_string(),
            label: "Ridge Alignment".to_string(),
            source_feature_id: "border_segment_demo".to_string(),
            target_feature_id: "mountain_band_demo".to_string(),
            relation_type: CausalRelationType::ConstraintAlignment,
            path: vec![
                point(0.54, 0.69, 0.47),
                point(0.51, 0.73, 0.45),
                point(0.48, 0.76, 0.43),
                point(0.46, 0.78, 0.41),
            ],
            metrics: vec![
                metric("alignment_score", "整合", 0.82, "ratio", "0.82"),
                metric("ridge_offset", "偏差", 0.11, "rel", "0.11"),
            ],
            uncertainty_stage: UncertaintyStage::Medium,
            evidence_ids: vec!["ridge_alignment_evidence".to_string()],
            display_key: "ridge_alignment".to_string(),
        },
        CausalTraceSegment {
            trace_id: "passability_break".to_string(),
            label: "Passability Break".to_string(),
            source_feature_id: "border_segment_demo".to_string(),
            target_feature_id: "mountain_band_demo".to_string(),
            relation_type: CausalRelationType::GeomorphicStructure,
            path: vec![
                point(0.53, 0.70, 0.46),
                point(0.50, 0.71, 0.44),
                point(0.47, 0.72, 0.42),
                point(0.45, 0.75, 0.40),
            ],
            metrics: vec![
                metric("passability_index", "通過", 0.31, "ratio", "0.31"),
                metric("saddle_gap", "峠差", 0.22, "rel", "0.22"),
            ],
            uncertainty_stage: UncertaintyStage::Medium,
            evidence_ids: vec!["passability_break_evidence".to_string()],
            display_key: "passability_break".to_string(),
        },
        CausalTraceSegment {
            trace_id: "tectonic_driver".to_string(),
            label: "Tectonic Driver".to_string(),
            source_feature_id: "mountain_band_demo".to_string(),
            target_feature_id: "plate_driver_demo".to_string(),
            relation_type: CausalRelationType::TectonicDriver,
            path: vec![
                point(0.46, 0.78, 0.41),
                point(0.42, 0.82, 0.39),
                point(0.37, 0.85, 0.38),
                point(0.33, 0.86, 0.38),
            ],
            metrics: vec![
                metric("compression_strength", "圧縮", 0.68, "ratio", "0.68"),
                metric("uplift_bias", "隆起", 0.63, "ratio", "0.63"),
            ],
            uncertainty_stage: UncertaintyStage::High,
            evidence_ids: vec!["tectonic_driver_evidence".to_string()],
            display_key: "tectonic_driver".to_string(),
        },
    ];

    let metrics = vec![
        metric("alignment_score", "整合", 0.82, "ratio", "0.82"),
        metric("relief_delta", "標高差", 1.7, "rel", "+1.7"),
        metric("passability_index", "通過", 0.31, "ratio", "0.31"),
        metric("compression_strength", "圧縮", 0.68, "ratio", "0.68"),
        metric("uncertainty_stage", "不確実", 2.0, "stage", "M/H"),
    ];

    let display_mapping = CausalDisplayMapping {
        feature_styles: vec![
            CausalDisplayFeatureStyle {
                feature_id: "border_segment_demo".to_string(),
                color_hex: "#f7b267".to_string(),
                glow_intensity: 0.82,
                pulse_hz: 0.6,
                radius: 0.034,
            },
            CausalDisplayFeatureStyle {
                feature_id: "mountain_band_demo".to_string(),
                color_hex: "#7bc6cc".to_string(),
                glow_intensity: 0.76,
                pulse_hz: 0.5,
                radius: 0.041,
            },
            CausalDisplayFeatureStyle {
                feature_id: "plate_driver_demo".to_string(),
                color_hex: "#f25f5c".to_string(),
                glow_intensity: 0.7,
                pulse_hz: 0.42,
                radius: 0.048,
            },
        ],
        trace_styles: vec![
            CausalDisplayTraceStyle {
                trace_id: "ridge_alignment".to_string(),
                color_hex: "#f7d08a".to_string(),
                thickness: 0.012,
                flow_speed: 0.62,
                jitter_amplitude: 0.08,
                label_short: "整合 0.82".to_string(),
            },
            CausalDisplayTraceStyle {
                trace_id: "passability_break".to_string(),
                color_hex: "#8bd3dd".to_string(),
                thickness: 0.01,
                flow_speed: 0.48,
                jitter_amplitude: 0.11,
                label_short: "通過 0.31".to_string(),
            },
            CausalDisplayTraceStyle {
                trace_id: "tectonic_driver".to_string(),
                color_hex: "#f28482".to_string(),
                thickness: 0.014,
                flow_speed: 0.4,
                jitter_amplitude: 0.16,
                label_short: "圧縮 0.68".to_string(),
            },
        ],
    };

    let evidence = vec![
        CausalEvidenceEntry {
            evidence_id: "ridge_alignment_evidence".to_string(),
            trace_id: "ridge_alignment".to_string(),
            evidence_type: EvidenceType::Morphology,
            summary: "Border segment overlaps ridge-scale alignment in the demo slice.".to_string(),
            assumptions: vec![
                "国境は政治境界そのものではなく地形制約との整合として扱う".to_string()
            ],
            approximations: vec![
                "稜線整合は固定サンプル値で表し世界生成から再計算しない".to_string()
            ],
            uncertainty_reason: "固定スライスであり、局所分水界の再抽出をまだ行っていないため"
                .to_string(),
            reference_model: "constraint_alignment_demo_v1".to_string(),
            reference_notes: "地形整合の入口を短い数値と発光で確認する".to_string(),
        },
        CausalEvidenceEntry {
            evidence_id: "passability_break_evidence".to_string(),
            trace_id: "passability_break".to_string(),
            evidence_type: EvidenceType::PassabilityProxy,
            summary: "Low passability corridor indicates a break along saddles and lower relief."
                .to_string(),
            assumptions: vec!["通過可能性は移動コストの簡略 proxy として相対値のみ使う".to_string()],
            approximations: vec!["峠や低地の判定は静的 path 上のサンプル点で近似する".to_string()],
            uncertainty_reason: "人間移動や政治史を含まないため境界形成の説明力は限定的"
                .to_string(),
            reference_model: "geomorphic_passability_demo_v1".to_string(),
            reference_notes: "相対標高差と passability proxy を短縮表示する".to_string(),
        },
        CausalEvidenceEntry {
            evidence_id: "tectonic_driver_evidence".to_string(),
            trace_id: "tectonic_driver".to_string(),
            evidence_type: EvidenceType::TectonicProxy,
            summary: "Mountain uplift trend is mapped to a compression-oriented deep driver trace."
                .to_string(),
            assumptions: vec!["プレート境界の断定ではなく圧縮方向の痕跡として表す".to_string()],
            approximations: vec![
                "深部駆動は実プレート解ではなく代表方向ベクトルで置き換える".to_string()
            ],
            uncertainty_reason: "深部構造を地表の 1 スライスへ射影しているため方向解釈に幅がある"
                .to_string(),
            reference_model: "tectonic_driver_demo_v1".to_string(),
            reference_notes: "収束強度と不確実性理由を短く残す".to_string(),
        },
    ];

    CausalExplorationDemoResponse {
        demo_id: "border_mountain_plate_demo".to_string(),
        features,
        trace_segments,
        metrics,
        display_mapping,
        evidence,
    }
}

pub(crate) fn get_causal_exploration_demo(
    service: &WorldService,
    world_id: &str,
) -> Result<CausalExplorationDemoResponse, String> {
    service
        .world(world_id)
        .ok_or_else(|| world_not_found_error(world_id))?;
    Ok(build_demo_response())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::build_demo_response;
    use crate::application::world_dto::{CausalRelationType, InitWorldConfig};
    use crate::application::world_service::WorldService;
    use crate::application::world_use_cases;

    #[test]
    fn causal_demo_serializes_all_relation_types_and_evidence() {
        let response = build_demo_response();
        assert_eq!(response.trace_segments.len(), 3);
        assert_eq!(response.evidence.len(), 3);
        assert_eq!(
            response
                .trace_segments
                .iter()
                .map(|trace| trace.relation_type)
                .collect::<Vec<CausalRelationType>>(),
            vec![
                CausalRelationType::ConstraintAlignment,
                CausalRelationType::GeomorphicStructure,
                CausalRelationType::TectonicDriver,
            ],
        );

        let serialized = serde_json::to_value(&response).expect("serialize demo response");
        let trace_segments = serialized["trace_segments"]
            .as_array()
            .expect("trace segments array");
        assert_eq!(trace_segments.len(), 3);
        assert_eq!(
            trace_segments[0]["relation_type"],
            Value::String("constraint_alignment".to_string())
        );
        assert_eq!(
            trace_segments[1]["relation_type"],
            Value::String("geomorphic_structure".to_string())
        );
        assert_eq!(
            trace_segments[2]["relation_type"],
            Value::String("tectonic_driver".to_string())
        );
        assert!(serialized["evidence"].as_array().is_some_and(|entries| {
            entries.iter().all(|entry| {
                entry.get("assumptions").is_some()
                    && entry.get("approximations").is_some()
                    && entry.get("uncertainty_reason").is_some()
            })
        }));
    }

    #[test]
    fn causal_demo_known_world_returns_static_demo_slice() {
        let mut service = WorldService::new();
        let init = world_use_cases::init_world(
            &mut service,
            "seed-causal-demo".to_string(),
            1,
            InitWorldConfig {
                geology_params: None,
                simulation_rate: None,
                verification_mode: None,
            },
        )
        .expect("init world");

        let response = super::get_causal_exploration_demo(&service, &init.world_id)
            .expect("get causal exploration demo");

        assert_eq!(response.demo_id, "border_mountain_plate_demo");
        assert_eq!(response.features.len(), 3);
        assert_eq!(response.trace_segments.len(), 3);
        assert_eq!(response.evidence.len(), 3);
    }

    #[test]
    fn causal_demo_unknown_world_returns_error() {
        let service = WorldService::new();

        let error = super::get_causal_exploration_demo(&service, "world-missing")
            .expect_err("unknown world should fail");

        assert!(error.contains("world not found: world-missing"));
    }
}
