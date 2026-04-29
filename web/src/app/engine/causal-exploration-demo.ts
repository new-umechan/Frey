import type {
    CausalDisplayFeatureStyle,
    CausalDisplayTraceStyle,
    CausalEvidenceEntry,
    CausalExplorationDemoResult,
    CausalFeatureDescriptor,
    CausalTraceSegment,
} from "./engine-client";

export interface NormalizedCausalExplorationDemo extends CausalExplorationDemoResult {
    feature_by_id: Map<string, CausalFeatureDescriptor>;
    trace_by_id: Map<string, CausalTraceSegment>;
    feature_style_by_id: Map<string, CausalDisplayFeatureStyle>;
    trace_style_by_id: Map<string, CausalDisplayTraceStyle>;
    evidence_by_id: Map<string, CausalEvidenceEntry>;
    evidence_by_trace_id: Map<string, CausalEvidenceEntry[]>;
    trace_ids_by_feature_id: Map<string, string[]>;
}

function ensureRecord(value: unknown): Record<string, unknown> {
    return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
}

function ensureArray<T>(value: unknown): T[] {
    return Array.isArray(value) ? (value as T[]) : [];
}

export function normalizeCausalExplorationDemo(input: unknown): NormalizedCausalExplorationDemo {
    const record = ensureRecord(input);
    const demo = {
        demo_id: typeof record.demo_id === "string" ? record.demo_id : "border_mountain_plate_demo",
        features: ensureArray<CausalFeatureDescriptor>(record.features),
        trace_segments: ensureArray<CausalTraceSegment>(record.trace_segments),
        metrics: ensureArray(record.metrics),
        display_mapping: ensureRecord(record.display_mapping) as CausalExplorationDemoResult["display_mapping"],
        evidence: ensureArray<CausalEvidenceEntry>(record.evidence),
    } as CausalExplorationDemoResult;

    const feature_by_id = new Map(demo.features.map((feature) => [feature.feature_id, feature]));
    const trace_by_id = new Map(demo.trace_segments.map((trace) => [trace.trace_id, trace]));
    const feature_style_by_id = new Map(
        ensureArray<CausalDisplayFeatureStyle>(demo.display_mapping.feature_styles)
            .map((style) => [style.feature_id, style]),
    );
    const trace_style_by_id = new Map(
        ensureArray<CausalDisplayTraceStyle>(demo.display_mapping.trace_styles)
            .map((style) => [style.trace_id, style]),
    );
    const evidence_by_id = new Map(demo.evidence.map((entry) => [entry.evidence_id, entry]));
    const evidence_by_trace_id = new Map<string, CausalEvidenceEntry[]>();
    for (const entry of demo.evidence) {
        const list = evidence_by_trace_id.get(entry.trace_id) ?? [];
        list.push(entry);
        evidence_by_trace_id.set(entry.trace_id, list);
    }
    const trace_ids_by_feature_id = new Map<string, string[]>();
    for (const trace of demo.trace_segments) {
        const sourceList = trace_ids_by_feature_id.get(trace.source_feature_id) ?? [];
        sourceList.push(trace.trace_id);
        trace_ids_by_feature_id.set(trace.source_feature_id, sourceList);

        const targetList = trace_ids_by_feature_id.get(trace.target_feature_id) ?? [];
        targetList.push(trace.trace_id);
        trace_ids_by_feature_id.set(trace.target_feature_id, targetList);
    }

    return {
        ...demo,
        feature_by_id,
        trace_by_id,
        feature_style_by_id,
        trace_style_by_id,
        evidence_by_id,
        evidence_by_trace_id,
        trace_ids_by_feature_id,
    };
}
