import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

interface Args {
    jsonl: string;
    baseline: string | null;
    writeBaseline: string | null;
}

interface ClimateScoreRecord {
    timestamp_unix_ms: number;
    bench: string;
    seed: string;
    mesh_level: number;
    cell_count: number;
    runtime?: {
        climate_step_ms?: number | null;
    };
    phase2?: {
        state?: string;
        ref_path?: string | null;
        error?: string | null;
        metrics?: Record<string, number | null>;
    };
    phase1?: Record<string, {
        matched?: number;
        total?: number;
        excluded_known_hard?: number;
        coverage_ratio?: number | null;
    }>;
    diagnostics?: {
        precipitation_process?: Record<string, number | null>;
        precipitation_lat_bands?: Record<string, number | null>;
    };
}

const METRIC_KEYS = [
    "temperature",
    "precipitation",
    "aridity",
    "evapotranspiration",
    "runoff",
] as const;

const PHASE1_KEYS = ["temperature", "precipitation", "aridity"] as const;
const PROCESS_KEYS = [
    "continental_reduction_ratio",
    "cap_reduction_ratio",
    "depletion_reduction_ratio",
    "cold_coast_reduction_ratio",
    "cap_hit_ratio",
    "mean_monsoon_boost_mm",
    "mean_hotspot_boost_mm",
] as const;
const BAND_KEYS = ["tropics", "subtropics", "midlat", "highlat"] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/climate_main_scores.jsonl",
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
    console.error("Usage: node benches/scripts/compare-climate-scores.mjs [options]");
    console.error("  --jsonl <path>");
    console.error("  --baseline <path>");
    console.error("  --write-baseline <path>");
}

async function loadJsonlRecords(pathname: string): Promise<ClimateScoreRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as ClimateScoreRecord);
}

async function loadJson(pathname: string): Promise<ClimateScoreRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as ClimateScoreRecord;
}

async function saveJson(pathname: string, record: ClimateScoreRecord): Promise<void> {
    const outputPath = resolve(pathname);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(record, null, 2)}\n`, "utf8");
}

function formatValue(value: number | null | undefined, digits = 3): string {
    if (!Number.isFinite(Number(value))) {
        return "n/a";
    }
    return Number(value).toFixed(digits);
}

function formatDelta(current: number | null | undefined, baseline: number | null | undefined, digits = 3): string {
    if (!Number.isFinite(Number(current)) || !Number.isFinite(Number(baseline))) {
        return "n/a";
    }
    const delta = Number(current) - Number(baseline);
    return `${delta >= 0 ? "+" : ""}${delta.toFixed(digits)}`;
}

function printMetricSection(title: string, rows: Array<{ label: string; current: number | null | undefined; baseline: number | null | undefined; digits?: number }>) {
    console.log(title);
    for (const row of rows) {
        const digits = row.digits ?? 3;
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

    console.log("=== Climate Score Comparison ===");
    console.log(`current_timestamp_unix_ms=${current.timestamp_unix_ms}`);
    console.log(`baseline_timestamp_unix_ms=${baseline.timestamp_unix_ms}`);
    console.log(`current_seed=${current.seed}`);
    console.log(`baseline_seed=${baseline.seed}`);
    console.log("");

    printMetricSection(
        "-- Runtime --",
        [
            {
                label: "climate_step_ms",
                current: current.runtime?.climate_step_ms,
                baseline: baseline.runtime?.climate_step_ms,
            },
        ],
    );

    printMetricSection(
        "-- Phase2 Metrics --",
        METRIC_KEYS.map((key) => ({
            label: key,
            current: current.phase2?.metrics?.[key],
            baseline: baseline.phase2?.metrics?.[key],
        })),
    );

    printMetricSection(
        "-- Phase1 Coverage --",
        PHASE1_KEYS.map((key) => ({
            label: key,
            current: current.phase1?.[key]?.coverage_ratio,
            baseline: baseline.phase1?.[key]?.coverage_ratio,
        })),
    );

    printMetricSection(
        "-- Process Diagnostics --",
        PROCESS_KEYS.map((key) => ({
            label: key,
            current: current.diagnostics?.precipitation_process?.[key],
            baseline: baseline.diagnostics?.precipitation_process?.[key],
        })),
    );

    printMetricSection(
        "-- Latitude Bands --",
        BAND_KEYS.map((key) => ({
            label: key,
            current: current.diagnostics?.precipitation_lat_bands?.[key],
            baseline: baseline.diagnostics?.precipitation_lat_bands?.[key],
        })),
    );
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
