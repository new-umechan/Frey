import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

type NumberLike = number | null | undefined;
type Horizon = "short" | "mid" | "long";

interface Args {
    jsonl: string;
    baseline: string | null;
    writeBaseline: string | null;
    horizon: Horizon;
}

interface RegionMetric {
    region_id?: string;
    valid_cells?: number;
    rmse?: NumberLike;
    rho?: NumberLike;
}

interface GlaciologyScoreRecord {
    timestamp_unix_ms?: number;
    run_id?: string;
    horizon?: string;
    seed?: string;
    runtime?: {
        glaciology_step_ms_median?: NumberLike;
        glaciology_step_ms_p95?: NumberLike;
    };
    metrics?: {
        sle_mm?: NumberLike;
        sle_start_mm?: NumberLike;
        sle_mean_mm?: NumberLike;
        sle_min_mm?: NumberLike;
        sle_max_mm?: NumberLike;
        land_ice_volume_km3?: NumberLike;
        grid_spearman?: NumberLike;
        grid_rmse?: NumberLike;
        region_metrics?: RegionMetric[];
    };
}

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/glaciology_sea_level_series_scores.jsonl",
        baseline: null,
        writeBaseline: null,
        horizon: "short",
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
        case "--horizon": {
            const value = String(next ?? args.horizon);
            if (value !== "short" && value !== "mid" && value !== "long") {
                throw new Error(`Unknown horizon: ${value}`);
            }
            args.horizon = value;
            i += 1;
            break;
        }
        case "--help":
            console.error("Usage: tsx benches/scripts/compare-glaciology-scores.ts [options]");
            console.error("  --jsonl <path>");
            console.error("  --baseline <path>");
            console.error("  --write-baseline <path>");
            console.error("  --horizon <short|mid|long>");
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }

    return args;
}

async function loadJsonlRecords(pathname: string): Promise<GlaciologyScoreRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as GlaciologyScoreRecord);
}

async function loadJson(pathname: string): Promise<GlaciologyScoreRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as GlaciologyScoreRecord;
}

async function saveJson(pathname: string, record: GlaciologyScoreRecord): Promise<void> {
    const outputPath = resolve(pathname);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(record, null, 2)}\n`, "utf8");
}

function toFinite(value: NumberLike): number | null {
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : null;
}

function format(value: NumberLike, digits = 6): string {
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

function printMetric(label: string, current: NumberLike, baseline: NumberLike) {
    console.log(
        `${label}: current=${format(current)} baseline=${format(baseline)} delta=${formatDelta(current, baseline)}`,
    );
}

function summarizeRegionCoverage(record: GlaciologyScoreRecord): string {
    const regions = record.metrics?.region_metrics ?? [];
    const valid = regions.filter((region) => Number.isFinite(Number(region.valid_cells)) && Number(region.valid_cells) > 0);
    return `${valid.length}/${regions.length}`;
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const records = await loadJsonlRecords(args.jsonl);
    const filtered = records.filter((record) => record.horizon === args.horizon);
    if (filtered.length === 0) {
        throw new Error(`No records found in ${args.jsonl} for horizon=${args.horizon}`);
    }

    const current = filtered[filtered.length - 1];
    if (args.writeBaseline) {
        await saveJson(args.writeBaseline, current);
        console.log(`Baseline saved: ${resolve(args.writeBaseline)}`);
    }

    const baseline = args.baseline
        ? await loadJson(args.baseline)
        : filtered.length >= 2
            ? filtered[filtered.length - 2]
            : null;

    if (!baseline) {
        console.log("Only one record is available. Baseline comparison skipped.");
        return;
    }

    console.log("=== Glaciology Sea-Level Score Comparison ===");
    console.log(`horizon=${args.horizon}`);
    console.log(`current_timestamp_unix_ms=${current.timestamp_unix_ms ?? "n/a"}`);
    console.log(`baseline_timestamp_unix_ms=${baseline.timestamp_unix_ms ?? "n/a"}`);
    console.log(`current_run_id=${current.run_id ?? "n/a"}`);
    console.log(`baseline_run_id=${baseline.run_id ?? "n/a"}`);
    console.log("");

    console.log("-- Runtime --");
    printMetric(
        "glaciology_step_ms_median",
        current.runtime?.glaciology_step_ms_median,
        baseline.runtime?.glaciology_step_ms_median,
    );
    printMetric(
        "glaciology_step_ms_p95",
        current.runtime?.glaciology_step_ms_p95,
        baseline.runtime?.glaciology_step_ms_p95,
    );
    console.log("");

    console.log("-- Core Metrics --");
    printMetric("sle_mm", current.metrics?.sle_mm, baseline.metrics?.sle_mm);
    printMetric("land_ice_volume_km3", current.metrics?.land_ice_volume_km3, baseline.metrics?.land_ice_volume_km3);
    printMetric("grid_spearman", current.metrics?.grid_spearman, baseline.metrics?.grid_spearman);
    printMetric("grid_rmse", current.metrics?.grid_rmse, baseline.metrics?.grid_rmse);
    console.log("");

    console.log("-- Region Coverage --");
    console.log(`current=${summarizeRegionCoverage(current)} baseline=${summarizeRegionCoverage(baseline)}`);
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
