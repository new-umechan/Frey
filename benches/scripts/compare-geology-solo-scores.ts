import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

interface Args {
    jsonl: string;
    baseline: string | null;
    writeBaseline: string | null;
    check: boolean;
    failOnDeviation: boolean;
    minOceanicAgeBinSpearman: number;
    minRidgeDistanceBinSpearman: number;
    checkCoastalMonotonicity: boolean;
}

type NumberLike = number | null | undefined;
const DIAGNOSTIC_KEYS = [
    "generated_land_ratio",
    "oceanic_age_min_myr",
    "oceanic_age_max_myr",
    "mean_depth",
    "oceanic_age_valid_cells",
    "oceanic_age_total_cells",
    "oceanic_age_bin_count",
    "oceanic_age_populated_bins",
    "ridge_distance_valid_cells",
    "ridge_distance_total_cells",
    "ridge_distance_bin_count",
    "ridge_distance_populated_bins",
    "continental_valid_cells",
    "continental_ocean_cells",
    "continental_mean_height",
    "ocean_mean_height",
    "continental_median_height",
    "ocean_median_height",
] as const;
type DiagnosticKey = (typeof DIAGNOSTIC_KEYS)[number];

interface GeologySoloScoreRecord {
    timestamp_unix_ms?: number;
    bench?: string;
    seed?: string;
    mesh_level?: number;
    cell_count?: number;
    runtime?: {
        geology_build_ms?: NumberLike;
    };
    phase2?: {
        state?: string;
        metrics?: Record<string, NumberLike>;
    };
    diagnostics?: Partial<Record<DiagnosticKey, NumberLike>> & {
        coastal_inundation_response?: Array<{
            sea_level_rise_m?: NumberLike;
            generated_land_ratio?: NumberLike;
            reference_land_ratio?: NumberLike;
            generated_newly_inundated_ratio?: NumberLike;
            reference_newly_inundated_ratio?: NumberLike;
        }>;
    };
}

const PHASE2_KEYS = [
    "oceanic_age_depth_spearman",
    "oceanic_age_bin_spearman",
    "oceanic_age_coverage_ratio",
    "ridge_distance_depth_spearman",
    "ridge_distance_bin_spearman",
    "ridge_distance_coverage_ratio",
    "continental_ocean_mean_gap",
    "continental_ocean_median_gap",
    "continental_ocean_overlap_ratio",
] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/geology_solo_main_scores.jsonl",
        baseline: null,
        writeBaseline: null,
        check: false,
        failOnDeviation: false,
        minOceanicAgeBinSpearman: 0.20,
        minRidgeDistanceBinSpearman: 0.20,
        checkCoastalMonotonicity: true,
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
        case "--check":
            args.check = true;
            break;
        case "--fail-on-deviation":
            args.failOnDeviation = true;
            break;
        case "--min-oceanic-age-bin-spearman":
            args.minOceanicAgeBinSpearman = Number(next ?? args.minOceanicAgeBinSpearman);
            i += 1;
            break;
        case "--min-ridge-distance-bin-spearman":
            args.minRidgeDistanceBinSpearman = Number(next ?? args.minRidgeDistanceBinSpearman);
            i += 1;
            break;
        case "--check-coastal-monotonicity":
            args.checkCoastalMonotonicity = true;
            break;
        case "--no-check-coastal-monotonicity":
            args.checkCoastalMonotonicity = false;
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
    console.error("Usage: tsx benches/scripts/compare-geology-solo-scores.ts [options]");
    console.error("  --jsonl <path>");
    console.error("  --baseline <path>");
    console.error("  --write-baseline <path>");
    console.error("  --check");
    console.error("  --fail-on-deviation");
    console.error("  --min-oceanic-age-bin-spearman <number>");
    console.error("  --min-ridge-distance-bin-spearman <number>");
    console.error("  --check-coastal-monotonicity");
    console.error("  --no-check-coastal-monotonicity");
}

function validateCoastalMonotonicity(record: GeologySoloScoreRecord): string[] {
    const series = record.diagnostics?.coastal_inundation_response;
    if (!Array.isArray(series) || series.length < 2) {
        return ["metric=coastal_inundation_response reason=missing_or_too_short"];
    }

    const sorted = [...series].sort(
        (a, b) => Number(a.sea_level_rise_m ?? 0) - Number(b.sea_level_rise_m ?? 0),
    );
    const deviations: string[] = [];
    for (let i = 1; i < sorted.length; i += 1) {
        const prevRise = toFinite(sorted[i - 1].sea_level_rise_m);
        const currRise = toFinite(sorted[i].sea_level_rise_m);
        const prevGenLand = toFinite(sorted[i - 1].generated_land_ratio);
        const currGenLand = toFinite(sorted[i].generated_land_ratio);
        const prevRefLand = toFinite(sorted[i - 1].reference_land_ratio);
        const currRefLand = toFinite(sorted[i].reference_land_ratio);
        const prevGenInund = toFinite(sorted[i - 1].generated_newly_inundated_ratio);
        const currGenInund = toFinite(sorted[i].generated_newly_inundated_ratio);
        const prevRefInund = toFinite(sorted[i - 1].reference_newly_inundated_ratio);
        const currRefInund = toFinite(sorted[i].reference_newly_inundated_ratio);

        if (
            prevRise === null || currRise === null ||
            prevGenLand === null || currGenLand === null ||
            prevRefLand === null || currRefLand === null ||
            prevGenInund === null || currGenInund === null ||
            prevRefInund === null || currRefInund === null
        ) {
            deviations.push(`metric=coastal_inundation_response reason=non_numeric_pair index=${i}`);
            continue;
        }

        if (currRise <= prevRise) {
            deviations.push(`metric=coastal_inundation_response reason=non_increasing_sea_level prev=${prevRise} curr=${currRise}`);
        }
        if (currGenLand > prevGenLand + 1e-6) {
            deviations.push(`metric=generated_land_ratio reason=non_monotonic prev=${prevGenLand.toFixed(6)} curr=${currGenLand.toFixed(6)} rise_prev=${prevRise} rise_curr=${currRise}`);
        }
        if (currRefLand > prevRefLand + 1e-6) {
            deviations.push(`metric=reference_land_ratio reason=non_monotonic prev=${prevRefLand.toFixed(6)} curr=${currRefLand.toFixed(6)} rise_prev=${prevRise} rise_curr=${currRise}`);
        }
        if (currGenInund + 1e-6 < prevGenInund) {
            deviations.push(`metric=generated_newly_inundated_ratio reason=non_monotonic prev=${prevGenInund.toFixed(6)} curr=${currGenInund.toFixed(6)} rise_prev=${prevRise} rise_curr=${currRise}`);
        }
        if (currRefInund + 1e-6 < prevRefInund) {
            deviations.push(`metric=reference_newly_inundated_ratio reason=non_monotonic prev=${prevRefInund.toFixed(6)} curr=${currRefInund.toFixed(6)} rise_prev=${prevRise} rise_curr=${currRise}`);
        }
    }

    return deviations;
}

async function loadJsonlRecords(pathname: string): Promise<GeologySoloScoreRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as GeologySoloScoreRecord);
}

async function loadJson(pathname: string): Promise<GeologySoloScoreRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as GeologySoloScoreRecord;
}

async function saveJson(pathname: string, record: GeologySoloScoreRecord): Promise<void> {
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

function printMetricSection(
    title: string,
    rows: Array<{ label: string; current: NumberLike; baseline: NumberLike; digits?: number }>,
) {
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

    console.log("=== Geology Solo Score Comparison ===");
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
                label: "geology_build_ms",
                current: current.runtime?.geology_build_ms,
                baseline: baseline.runtime?.geology_build_ms,
                digits: 3,
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

    if (!args.check) {
        return;
    }
    const deviations: string[] = [];
    const oceanicAgeBin = toFinite(current.phase2?.metrics?.oceanic_age_bin_spearman);
    if (oceanicAgeBin === null || oceanicAgeBin < args.minOceanicAgeBinSpearman) {
        deviations.push(
            `metric=oceanic_age_bin_spearman current=${formatValue(oceanicAgeBin)} min=${args.minOceanicAgeBinSpearman.toFixed(3)}`,
        );
    }
    const ridgeDistanceBin = toFinite(current.phase2?.metrics?.ridge_distance_bin_spearman);
    if (ridgeDistanceBin === null || ridgeDistanceBin < args.minRidgeDistanceBinSpearman) {
        deviations.push(
            `metric=ridge_distance_bin_spearman current=${formatValue(ridgeDistanceBin)} min=${args.minRidgeDistanceBinSpearman.toFixed(3)}`,
        );
    }
    if (args.checkCoastalMonotonicity) {
        deviations.push(...validateCoastalMonotonicity(current));
    }

    if (deviations.length === 0) {
        console.log("[geology-solo-check] deviations=0");
        return;
    }
    for (const deviation of deviations) {
        console.error(`[geology-solo-check] deviation ${deviation}`);
    }
    console.error(`[geology-solo-check] deviations=${deviations.length}`);
    if (args.failOnDeviation) {
        process.exit(1);
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
