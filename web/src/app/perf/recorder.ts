const PERF_LABEL = "perf-32";

function roundMetric(value: number): number {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1_000_000) / 1_000_000;
}

function percentile(sortedValues: number[], ratio: number): number {
    if (sortedValues.length === 0) {
        return 0;
    }
    const clamped = Math.min(1, Math.max(0, ratio));
    const index = Math.min(
        sortedValues.length - 1,
        Math.max(0, Math.ceil(sortedValues.length * clamped) - 1),
    );
    return sortedValues[index];
}

export interface MetricStats {
    count: number;
    mean: number;
    min: number;
    max: number;
    p50: number;
    p95: number;
    p99: number;
}

function summarizeSamples(samples: number[]): MetricStats {
    const values = Array.isArray(samples) ? samples.filter((value) => Number.isFinite(value)) : [];
    const sorted = values.slice().sort((a, b) => a - b);
    const count = sorted.length;
    const total = sorted.reduce((sum, value) => sum + value, 0);
    const mean = count > 0 ? total / count : 0;
    const min = count > 0 ? sorted[0] : 0;
    const max = count > 0 ? sorted[count - 1] : 0;
    return {
        count,
        mean: roundMetric(mean),
        min: roundMetric(min),
        max: roundMetric(max),
        p50: roundMetric(percentile(sorted, 0.50)),
        p95: roundMetric(percentile(sorted, 0.95)),
        p99: roundMetric(percentile(sorted, 0.99)),
    };
}

export interface TickPerfRecorder {
    measure: <T>(name: string, callback: () => T) => T;
    pushSample: (name: string, valueMs: number) => void;
    buildSummary: () => Record<string, MetricStats>;
}

export interface PerfProfile {
    label?: string;
    tickCount?: number;
    seed?: string;
    surfaceMode?: string;
    viewMode?: string;
    cellMetric?: string;
    [key: string]: unknown;
}

export interface PerfResult {
    metrics?: Record<string, MetricStats>;
    [key: string]: unknown;
}

export function createTickPerfRecorder(): TickPerfRecorder {
    const sampleBuckets = new Map<string, number[]>();

    function pushSample(name: string, valueMs: number) {
        if (!sampleBuckets.has(name)) {
            sampleBuckets.set(name, []);
        }
        sampleBuckets.get(name)!.push(valueMs);
    }

    function measure<T>(name: string, callback: () => T): T {
        const start = performance.now();
        const result = callback();
        pushSample(name, performance.now() - start);
        return result;
    }

    return {
        measure,
        pushSample,
        buildSummary() {
            const summary: Record<string, MetricStats> = {};
            for (const [name, samples] of sampleBuckets.entries()) {
                summary[name] = summarizeSamples(samples);
            }
            return summary;
        },
    };
}

export function createPerfProfile(overrides: Partial<PerfProfile> = {}): PerfProfile {
    return {
        label: PERF_LABEL,
        tickCount: 32,
        seed: "alpha",
        surfaceMode: "globe",
        viewMode: "normal",
        ...overrides,
    };
}

export function createPerfConsoleTable(result: unknown): Array<{
    metric: string;
    count: number;
    mean_ms: number;
    p50_ms: number;
    p95_ms: number;
    p99_ms: number;
    min_ms: number;
    max_ms: number;
}> {
    const perfResult = result as PerfResult | null;
    if (!perfResult?.metrics) {
        return [];
    }
    return Object.entries(perfResult.metrics).map(([name, stats]) => ({
        metric: name,
        count: stats.count,
        mean_ms: stats.mean,
        p50_ms: stats.p50,
        p95_ms: stats.p95,
        p99_ms: stats.p99,
        min_ms: stats.min,
        max_ms: stats.max,
    }));
}

export function formatPerfSummaryLine(result: unknown): string {
    const perfResult = result as PerfResult | null;
    if (!perfResult?.metrics?.tick_total) {
        return "No performance data.";
    }
    const tickTotal = perfResult.metrics.tick_total;
    const step = perfResult.metrics.exec_world;
    const delta = perfResult.metrics.delta_sync;
    const geom = perfResult.metrics.geometry_update;
    const river = perfResult.metrics.river_mask_update;
    return [
        `p50=${tickTotal.p50}ms`,
        `p95=${tickTotal.p95}ms`,
        step ? `step=${step.mean}ms` : null,
        delta ? `delta=${delta.mean}ms` : null,
        geom ? `geom=${geom.mean}ms` : null,
        river ? `river=${river.mean}ms` : null,
    ].filter(Boolean).join(" | ");
}
