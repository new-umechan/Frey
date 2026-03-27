const PERF_LABEL = "perf-32";

function roundMetric(value) {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.round(value * 1000) / 1000;
}

function percentile(sortedValues, ratio) {
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

function summarizeSamples(samples) {
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

export function createTickPerfRecorder() {
    const sampleBuckets = new Map();

    function pushSample(name, valueMs) {
        if (!sampleBuckets.has(name)) {
            sampleBuckets.set(name, []);
        }
        sampleBuckets.get(name).push(valueMs);
    }

    function measure(name, callback) {
        const start = performance.now();
        const result = callback();
        pushSample(name, performance.now() - start);
        return result;
    }

    return {
        measure,
        pushSample,
        buildSummary() {
            const summary = {};
            for (const [name, samples] of sampleBuckets.entries()) {
                summary[name] = summarizeSamples(samples);
            }
            return summary;
        },
    };
}

export function createPerfProfile(overrides = {}) {
    return {
        label: PERF_LABEL,
        tickCount: 32,
        seed: "alpha",
        surfaceMode: "globe",
        viewMode: "normal",
        ...overrides,
    };
}

export function createPerfConsoleTable(result) {
    if (!result?.metrics) {
        return [];
    }
    return Object.entries(result.metrics).map(([name, stats]) => ({
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

export function formatPerfSummaryLine(result) {
    if (!result?.metrics?.tick_total) {
        return "No performance data.";
    }
    const tickTotal = result.metrics.tick_total;
    const step = result.metrics.exec_world;
    const delta = result.metrics.delta_sync;
    const geom = result.metrics.geometry_update;
    const river = result.metrics.river_mask_update;
    return [
        `p50=${tickTotal.p50}ms`,
        `p95=${tickTotal.p95}ms`,
        step ? `step=${step.mean}ms` : null,
        delta ? `delta=${delta.mean}ms` : null,
        geom ? `geom=${geom.mean}ms` : null,
        river ? `river=${river.mean}ms` : null,
    ].filter(Boolean).join(" | ");
}
