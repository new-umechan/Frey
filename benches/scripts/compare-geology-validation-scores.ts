import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

interface Args {
    jsonl: string;
    baseline: string | null;
    writeBaseline: string | null;
}

type NumberLike = number | null | undefined;

interface GeologyScoreRecord {
    timestamp_unix_ms?: number;
    bench?: string;
    seed?: string;
    mesh_level?: number;
    cell_count?: number;
    runtime?: {
        geology_step_p50_ms?: NumberLike;
        geology_step_p95_ms?: NumberLike;
        stabilization_ticks?: number;
        sample_ticks?: number;
    };
    phase2?: {
        state?: string;
        metrics?: Record<string, NumberLike>;
    };
    diagnostics?: Record<string, NumberLike>;
}

const PHASE2_KEYS = [
    "sediment_budget_ratio",
    "coastal_deposition_share",
    "low_slope_deposition_share",
] as const;

const DIAGNOSTIC_KEYS = [
    "open_boundary_export_fraction",
    "erosion_reference_coverage",
    "lake_deposition_share",
] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/geology_validation_main_scores.jsonl",
        baseline: null,
        writeBaseline: null,
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
            args.baseline = String(next ?? "");
            i += 1;
            break;
        case "--write-baseline":
            args.writeBaseline = String(next ?? "");
            i += 1;
            break;
        case "--help":
            printHelp();
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }

    return args;
}

function printHelp() {
    console.error("Usage: tsx benches/scripts/compare-geology-validation-scores.ts [options]");
    console.error("  --jsonl <path>");
    console.error("  --baseline <path>");
    console.error("  --write-baseline <path>");
}

async function loadJsonlRecords(pathname: string): Promise<GeologyScoreRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as GeologyScoreRecord);
}

async function loadJson(pathname: string): Promise<GeologyScoreRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as GeologyScoreRecord;
}

async function saveJson(pathname: string, record: GeologyScoreRecord): Promise<void> {
    const outputPath = resolve(pathname);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(record, null, 2)}\n`, "utf8");
}

function toFinite(value: NumberLike): number | null {
    return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function formatValue(value: NumberLike, digits = 6): string {
    const numeric = toFinite(value);
    return numeric === null ? "n/a" : numeric.toFixed(digits);
}

function formatDelta(current: NumberLike, baseline: NumberLike, digits = 6): string {
    const c = toFinite(current);
    const b = toFinite(baseline);
    if (c === null || b === null) {
        return "n/a";
    }
    const delta = c - b;
    return `${delta >= 0 ? "+" : ""}${delta.toFixed(digits)}`;
}

function printMetricSection(title: string, rows: Array<{ label: string; current: NumberLike; baseline: NumberLike; digits?: number }>) {
    console.log(title);
    for (const row of rows) {
        const digits = row.digits ?? 6;
        console.log(
            `${row.label}: current=${formatValue(row.current, digits)} baseline=${formatValue(row.baseline, digits)} delta=${formatDelta(row.current, row.baseline, digits)}`,
        );
    }
    console.log("");
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const records = await loadJsonlRecords(args.jsonl);
    if (records.length === 0) {
        throw new Error(`No records found in ${args.jsonl}`);
    }

    const current = records[records.length - 1];
    if (args.writeBaseline) {
        await saveJson(args.writeBaseline, current);
        console.log(`Baseline saved: ${resolve(args.writeBaseline)}`);
    }

    const baseline = args.baseline
        ? await loadJson(args.baseline)
        : records.length >= 2
            ? records[records.length - 2]
            : null;

    if (!baseline) {
        console.log("Only one record is available. Baseline comparison skipped.");
        return;
    }

    console.log("=== Geology Validation Score Comparison ===");
    console.log(`current_timestamp_unix_ms=${current.timestamp_unix_ms ?? "n/a"}`);
    console.log(`baseline_timestamp_unix_ms=${baseline.timestamp_unix_ms ?? "n/a"}`);
    console.log(`current_seed=${current.seed ?? "n/a"}`);
    console.log(`baseline_seed=${baseline.seed ?? "n/a"}`);
    console.log(`current_phase2_state=${current.phase2?.state ?? "n/a"}`);
    console.log(`baseline_phase2_state=${baseline.phase2?.state ?? "n/a"}`);
    console.log("");

    printMetricSection(
        "-- Runtime --",
        [
            {
                label: "geology_step_p50_ms",
                current: current.runtime?.geology_step_p50_ms,
                baseline: baseline.runtime?.geology_step_p50_ms,
            },
            {
                label: "geology_step_p95_ms",
                current: current.runtime?.geology_step_p95_ms,
                baseline: baseline.runtime?.geology_step_p95_ms,
            },
        ],
    );

    printMetricSection(
        "-- Phase2 Metrics --",
        PHASE2_KEYS.map((key) => ({
            label: key,
            current: current.phase2?.metrics?.[key],
            baseline: baseline.phase2?.metrics?.[key],
        })),
    );

    printMetricSection(
        "-- Diagnostics --",
        DIAGNOSTIC_KEYS.map((key) => ({
            label: key,
            current: current.diagnostics?.[key],
            baseline: baseline.diagnostics?.[key],
        })),
    );
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
