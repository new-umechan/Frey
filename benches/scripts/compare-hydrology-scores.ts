import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

interface Args {
    jsonl: string;
    baseline: string | null;
    writeBaseline: string | null;
}

interface HydrologyScoreRecord {
    timestamp_unix_ms: number;
    bench: string;
    seed: string;
    mesh_level: number;
    cell_count: number;
    runtime?: {
        hydrology_step_ms?: number | null;
    };
    phase2?: {
        state?: string;
        ref_path?: string | null;
        erosion_ref_path?: string | null;
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
        flow_stats?: Record<string, number | null>;
        lake_stats?: Record<string, number | null>;
        fill_spill_stats?: Record<string, number | null>;
    };
}

const METRIC_KEYS = [
    "river_flow_rho",
    "is_lake_precision",
    "is_lake_recall",
    "is_lake_f1",
    "erosion_rate_spearman",
    "sediment_budget_ratio",
    "coastal_deposition_share",
    "low_slope_deposition_share",
] as const;

const PHASE1_KEYS = ["river_flow_ranking"] as const;
const FILL_SPILL_DIAGNOSTIC_KEYS = [
    "active_sink_count",
    "overflow_active_ratio",
    "mean_sink_fill_ratio",
    "ponded_cell_count",
] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/hydrology_main_scores.jsonl",
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
    console.error("Usage: tsx benches/scripts/compare-hydrology-scores.ts [options]");
    console.error("  --jsonl <path>");
    console.error("  --baseline <path>");
    console.error("  --write-baseline <path>");
}

async function loadJsonlRecords(pathname: string): Promise<HydrologyScoreRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as HydrologyScoreRecord);
}

async function loadJson(pathname: string): Promise<HydrologyScoreRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as HydrologyScoreRecord;
}

async function saveJson(pathname: string, record: HydrologyScoreRecord): Promise<void> {
    const outputPath = resolve(pathname);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(record, null, 2)}\n`, "utf8");
}

function formatValue(value: number | null | undefined, digits = 3): string {
    if (typeof value !== "number" || !Number.isFinite(value)) {
        return "n/a";
    }
    return value.toFixed(digits);
}

function formatDelta(current: number | null | undefined, baseline: number | null | undefined, digits = 3): string {
    if (typeof current !== "number" || !Number.isFinite(current) || typeof baseline !== "number" || !Number.isFinite(baseline)) {
        return "n/a";
    }
    const delta = current - baseline;
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

    console.log("=== Hydrology Score Comparison ===");
    console.log(`current_timestamp_unix_ms=${current.timestamp_unix_ms}`);
    console.log(`baseline_timestamp_unix_ms=${baseline.timestamp_unix_ms}`);
    console.log(`current_seed=${current.seed}`);
    console.log(`baseline_seed=${baseline.seed}`);
    console.log(`current_phase2_state=${current.phase2?.state ?? "n/a"}`);
    console.log(`baseline_phase2_state=${baseline.phase2?.state ?? "n/a"}`);
    console.log(`current_phase2_ref_path=${current.phase2?.ref_path ?? "n/a"}`);
    console.log(`baseline_phase2_ref_path=${baseline.phase2?.ref_path ?? "n/a"}`);
    console.log(`current_phase2_erosion_ref_path=${current.phase2?.erosion_ref_path ?? "n/a"}`);
    console.log(`baseline_phase2_erosion_ref_path=${baseline.phase2?.erosion_ref_path ?? "n/a"}`);
    console.log("");

    printMetricSection(
        "-- Runtime --",
        [
            {
                label: "hydrology_step_ms",
                current: current.runtime?.hydrology_step_ms,
                baseline: baseline.runtime?.hydrology_step_ms,
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
        "-- Fill-Spill Diagnostics --",
        FILL_SPILL_DIAGNOSTIC_KEYS.map((key) => ({
            label: key,
            current: current.diagnostics?.fill_spill_stats?.[key],
            baseline: baseline.diagnostics?.fill_spill_stats?.[key],
        })),
    );
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
