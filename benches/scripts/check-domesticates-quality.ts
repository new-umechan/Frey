import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type NumberLike = number | null | undefined;

interface Args {
    jsonl: string;
    baseline: string;
    thresholdRuntime: number;
    maxMetricDrop: number;
    maxRegionalCoverageDrop: number;
}

interface DomesticatesRecord {
    runtime?: {
        domesticates_step_ms?: NumberLike;
    };
    metrics?: Record<string, NumberLike>;
}

const METRIC_KEYS = [
    "crop_intensity_rho",
    "crop_presence_f1",
    "livestock_intensity_rho",
    "livestock_presence_f1",
] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/domesticates_main_scores.jsonl",
        baseline: "tests/perf/domesticates-bench-baseline.json",
        thresholdRuntime: 0.20,
        maxMetricDrop: 0.03,
        maxRegionalCoverageDrop: 0.05,
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
        case "--threshold-runtime":
            args.thresholdRuntime = Math.max(0, Number(next ?? args.thresholdRuntime));
            i += 1;
            break;
        case "--max-metric-drop":
            args.maxMetricDrop = Math.max(0, Number(next ?? args.maxMetricDrop));
            i += 1;
            break;
        case "--max-regional-coverage-drop":
            args.maxRegionalCoverageDrop = Math.max(0, Number(next ?? args.maxRegionalCoverageDrop));
            i += 1;
            break;
        case "--help":
            console.error("Usage: tsx benches/scripts/check-domesticates-quality.ts [options]");
            console.error("  --jsonl <path>");
            console.error("  --baseline <path>");
            console.error("  --threshold-runtime <ratio>");
            console.error("  --max-metric-drop <value>");
            console.error("  --max-regional-coverage-drop <value>");
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

async function loadJsonlRecords(pathname: string): Promise<DomesticatesRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as DomesticatesRecord);
}

async function loadJson(pathname: string): Promise<DomesticatesRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as DomesticatesRecord;
}

function format(value: number | null): string {
    return value === null || !Number.isFinite(value) ? "n/a" : value.toFixed(6);
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const records = await loadJsonlRecords(args.jsonl);
    if (records.length === 0) {
        throw new Error(`No records found in ${args.jsonl}`);
    }

    const current = records[records.length - 1];
    const baseline = await loadJson(args.baseline);
    const currentRuntime = toFinite(current.runtime?.domesticates_step_ms);
    const baselineRuntime = toFinite(baseline.runtime?.domesticates_step_ms);
    if (currentRuntime === null || baselineRuntime === null || baselineRuntime <= 0) {
        throw new Error("Missing runtime values for quality gate");
    }

    const allowedRuntime = baselineRuntime * (1 + args.thresholdRuntime);
    const failures: string[] = [];
    if (currentRuntime > allowedRuntime) {
        failures.push(
            `runtime exceeded: current=${currentRuntime.toFixed(6)} baseline=${baselineRuntime.toFixed(6)} allowed=${allowedRuntime.toFixed(6)}`,
        );
    }

    for (const key of METRIC_KEYS) {
        const currentValue = toFinite(current.metrics?.[key]);
        const baselineValue = toFinite(baseline.metrics?.[key]);
        if (currentValue === null || baselineValue === null) {
            failures.push(`missing metric values for ${key}`);
            continue;
        }
        const minAllowed = baselineValue - args.maxMetricDrop;
        if (currentValue < minAllowed) {
            failures.push(
                `${key} dropped too much: current=${currentValue.toFixed(6)} baseline=${baselineValue.toFixed(6)} min_allowed=${minAllowed.toFixed(6)}`,
            );
        }
    }

    const currentRegionalCoverage = toFinite(current.metrics?.regional_assertion_coverage);
    const baselineRegionalCoverage = toFinite(baseline.metrics?.regional_assertion_coverage);
    if (currentRegionalCoverage === null || baselineRegionalCoverage === null) {
        failures.push("missing metric values for regional_assertion_coverage");
    } else {
        const minAllowed = baselineRegionalCoverage - args.maxRegionalCoverageDrop;
        if (currentRegionalCoverage < minAllowed) {
            failures.push(
                `regional_assertion_coverage dropped too much: current=${currentRegionalCoverage.toFixed(6)} baseline=${baselineRegionalCoverage.toFixed(6)} min_allowed=${minAllowed.toFixed(6)}`,
            );
        }
    }

    const currentOverallScore = toFinite(current.metrics?.overall_score);
    const baselineOverallScore = toFinite(baseline.metrics?.overall_score);
    if (currentOverallScore === null || baselineOverallScore === null) {
        failures.push("missing metric values for overall_score");
    }

    console.log(`runtime_current=${format(currentRuntime)}`);
    console.log(`runtime_baseline=${format(baselineRuntime)}`);
    console.log(`runtime_allowed_max=${format(allowedRuntime)}`);
    for (const key of METRIC_KEYS) {
        const currentValue = toFinite(current.metrics?.[key]);
        const baselineValue = toFinite(baseline.metrics?.[key]);
        const minAllowed = baselineValue === null ? null : baselineValue - args.maxMetricDrop;
        console.log(
            `${key}_current=${format(currentValue)} baseline=${format(baselineValue)} min_allowed=${format(minAllowed)}`,
        );
    }
    const regionalMinAllowed = baselineRegionalCoverage === null
        ? null
        : baselineRegionalCoverage - args.maxRegionalCoverageDrop;
    console.log(
        `regional_assertion_coverage_current=${format(currentRegionalCoverage)} baseline=${format(baselineRegionalCoverage)} min_allowed=${format(regionalMinAllowed)}`,
    );
    console.log(
        `overall_score_current=${format(currentOverallScore)} baseline=${format(baselineOverallScore)} min_allowed=n/a`,
    );

    if (failures.length > 0) {
        for (const failure of failures) {
            console.error(`[domesticates quality gate] FAIL ${failure}`);
        }
        process.exit(1);
    }

    console.log("[domesticates quality gate] PASS");
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
