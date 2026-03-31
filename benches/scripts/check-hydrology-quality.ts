import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type NumberLike = number | null | undefined;

interface Args {
    jsonl: string;
    baseline: string;
    window: number;
    thresholdRuntime: number;
    maxFlowRhoDrop: number;
    maxLakeF1Drop: number;
}

interface HydrologyRecord {
    runtime?: {
        hydrology_step_ms?: NumberLike;
    };
    runtime_stats?: {
        count?: number;
        median_ms?: NumberLike;
        p95_ms?: NumberLike;
    };
    phase2?: {
        metrics?: Record<string, NumberLike>;
    };
}

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/hydrology_main_scores.jsonl",
        baseline: "tests/perf/hydrology-bench-baseline.json",
        window: 5,
        thresholdRuntime: 0.15,
        maxFlowRhoDrop: 0.05,
        maxLakeF1Drop: 0.05,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--":
            break;
        case "--jsonl":
            args.jsonl = String(next ?? args.jsonl);
            i += 1;
            break;
        case "--baseline":
            args.baseline = String(next ?? args.baseline);
            i += 1;
            break;
        case "--window":
            args.window = Math.max(1, Number(next ?? args.window));
            i += 1;
            break;
        case "--threshold-runtime":
            args.thresholdRuntime = Math.max(0, Number(next ?? args.thresholdRuntime));
            i += 1;
            break;
        case "--max-flow-rho-drop":
            args.maxFlowRhoDrop = Math.max(0, Number(next ?? args.maxFlowRhoDrop));
            i += 1;
            break;
        case "--max-lake-f1-drop":
            args.maxLakeF1Drop = Math.max(0, Number(next ?? args.maxLakeF1Drop));
            i += 1;
            break;
        case "--help":
            console.error("Usage: tsx benches/scripts/check-hydrology-quality.ts [options]");
            console.error("  --jsonl <path>");
            console.error("  --baseline <path>");
            console.error("  --window <n>");
            console.error("  --threshold-runtime <ratio>");
            console.error("  --max-flow-rho-drop <value>");
            console.error("  --max-lake-f1-drop <value>");
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }

    return args;
}

function toFinite(value: NumberLike): number | null {
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : null;
}

function percentile(values: number[], p: number): number {
    if (values.length === 0) {
        return Number.NaN;
    }
    const sorted = [...values].sort((a, b) => a - b);
    const clamped = Math.min(1, Math.max(0, p));
    const index = Math.ceil(clamped * sorted.length) - 1;
    return sorted[Math.max(0, Math.min(index, sorted.length - 1))];
}

function median(values: number[]): number {
    return percentile(values, 0.5);
}

function extractRuntimeMs(record: HydrologyRecord): number | null {
    const fromStats = toFinite(record.runtime_stats?.p95_ms);
    if (fromStats !== null) {
        return fromStats;
    }
    return toFinite(record.runtime?.hydrology_step_ms);
}

function extractMetric(record: HydrologyRecord, key: string): number | null {
    return toFinite(record.phase2?.metrics?.[key]);
}

async function loadJsonlRecords(pathname: string): Promise<HydrologyRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as HydrologyRecord);
}

async function loadJson(pathname: string): Promise<HydrologyRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as HydrologyRecord;
}

function format(value: number): string {
    return Number.isFinite(value) ? value.toFixed(6) : "n/a";
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const records = await loadJsonlRecords(args.jsonl);
    if (records.length === 0) {
        throw new Error(`No records found in ${args.jsonl}`);
    }
    const currentRecords = records.slice(-Math.min(args.window, records.length));
    const baseline = await loadJson(args.baseline);

    const currentRuntimeSeries = currentRecords
        .map((record) => extractRuntimeMs(record))
        .filter((value): value is number => value !== null);
    const currentRuntimeP95 = percentile(currentRuntimeSeries, 0.95);
    const baselineRuntime = extractRuntimeMs(baseline);
    if (!Number.isFinite(currentRuntimeP95) || baselineRuntime === null || baselineRuntime <= 0) {
        throw new Error("Missing runtime values for quality gate");
    }

    const metricKeys = [
        { key: "river_flow_rho", maxDrop: args.maxFlowRhoDrop },
        { key: "is_lake_f1", maxDrop: args.maxLakeF1Drop },
    ];
    const metricRows = metricKeys.map((item) => {
        const currentValues = currentRecords
            .map((record) => extractMetric(record, item.key))
            .filter((value): value is number => value !== null);
        const baselineValue = extractMetric(baseline, item.key);
        return {
            key: item.key,
            maxDrop: item.maxDrop,
            current: median(currentValues),
            baseline: baselineValue,
        };
    });

    const failures: string[] = [];
    const allowedRuntime = baselineRuntime * (1 + args.thresholdRuntime);
    if (currentRuntimeP95 > allowedRuntime) {
        failures.push(
            `runtime_p95 exceeded: current=${currentRuntimeP95.toFixed(6)} baseline=${baselineRuntime.toFixed(6)} allowed=${allowedRuntime.toFixed(6)}`,
        );
    }

    for (const row of metricRows) {
        if (row.baseline === null || !Number.isFinite(row.current)) {
            failures.push(`missing metric values for ${row.key}`);
            continue;
        }
        const minAllowed = row.baseline - row.maxDrop;
        if (row.current < minAllowed) {
            failures.push(
                `${row.key} dropped too much: current=${row.current.toFixed(6)} baseline=${row.baseline.toFixed(6)} min_allowed=${minAllowed.toFixed(6)}`,
            );
        }
    }

    console.log(`window=${currentRecords.length}`);
    console.log(`runtime_p95_current=${format(currentRuntimeP95)}`);
    console.log(`runtime_baseline=${format(baselineRuntime)}`);
    console.log(`runtime_allowed_max=${format(allowedRuntime)}`);
    for (const row of metricRows) {
        console.log(
            `${row.key}_median_current=${format(row.current)} baseline=${format(row.baseline ?? Number.NaN)} min_allowed=${format((row.baseline ?? Number.NaN) - row.maxDrop)}`,
        );
    }

    if (failures.length > 0) {
        for (const failure of failures) {
            console.error(`[hydrology quality gate] FAIL ${failure}`);
        }
        process.exit(1);
    }

    console.log("[hydrology quality gate] PASS");
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
