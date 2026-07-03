import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type NumberLike = number | null | undefined;

interface Args {
    freyJsonl: string;
    earthJson: string;
    runId?: string;
}

interface FreyRecord {
    run_id?: string;
    seed?: string;
    plate_shape?: Record<string, NumberLike>;
}

interface EarthShapeRecord {
    time_ma?: number;
    top8_p99_elongation?: NumberLike;
    top8_p99_narrow_connection_cell_ratio?: NumberLike;
    top8_p99_boundary_complexity?: NumberLike;
    area_ge_1pct_p99_elongation?: NumberLike;
    area_ge_1pct_p99_narrow_connection_cell_ratio?: NumberLike;
    area_ge_1pct_p99_boundary_complexity?: NumberLike;
}

const COMPARISONS = [
    {
        label: "elongation",
        freyMaxKey: "max_elongation",
        freyTop8Key: "top8_p99_elongation",
        freyAreaKey: "area_ge_1pct_p99_elongation",
        top8Key: "top8_p99_elongation",
        areaKey: "area_ge_1pct_p99_elongation",
    },
    {
        label: "narrow_connection_cell_ratio",
        freyMaxKey: "max_narrow_connection_cell_ratio",
        freyTop8Key: "top8_p99_narrow_connection_cell_ratio",
        freyAreaKey: "area_ge_1pct_p99_narrow_connection_cell_ratio",
        top8Key: "top8_p99_narrow_connection_cell_ratio",
        areaKey: "area_ge_1pct_p99_narrow_connection_cell_ratio",
    },
    {
        label: "boundary_complexity",
        freyMaxKey: "max_boundary_complexity",
        freyTop8Key: "top8_p99_boundary_complexity",
        freyAreaKey: "area_ge_1pct_p99_boundary_complexity",
        top8Key: "top8_p99_boundary_complexity",
        areaKey: "area_ge_1pct_p99_boundary_complexity",
    },
] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        freyJsonl: "benches/results/geology_validation_main_scores.jsonl",
        earthJson: "benches/results/earth_plate_shape_stats.json",
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--":
            break;
        case "--frey-jsonl":
            args.freyJsonl = String(next ?? args.freyJsonl);
            i += 1;
            break;
        case "--earth-json":
            args.earthJson = String(next ?? args.earthJson);
            i += 1;
            break;
        case "--run-id":
            args.runId = String(next ?? "");
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
    console.error("Usage: tsx benches/scripts/compare-plate-shape-earth.ts [options]");
    console.error("  --frey-jsonl <path>");
    console.error("  --earth-json <path>");
    console.error("  --run-id <id>    Compare a specific Frey JSONL run_id instead of the latest record.");
}

async function loadFreyRecord(pathname: string, runId?: string): Promise<FreyRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    const records = content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as FreyRecord)
        .filter((record) => record.plate_shape);
    if (records.length === 0) {
        throw new Error(`No Frey records with plate_shape found in ${pathname}`);
    }
    if (runId) {
        const record = records.find((candidate) => candidate.run_id === runId);
        if (!record) {
            throw new Error(`No Frey record with run_id=${runId} found in ${pathname}`);
        }
        return record;
    }
    return records[records.length - 1];
}

async function loadEarthRecords(pathname: string): Promise<EarthShapeRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    const records = JSON.parse(content) as EarthShapeRecord[];
    if (!Array.isArray(records) || records.length === 0) {
        throw new Error(`No Earth shape records found in ${pathname}`);
    }
    return records;
}

function finite(value: NumberLike): number | null {
    return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function maxEarth(records: EarthShapeRecord[], key: keyof EarthShapeRecord): number | null {
    const values = records
        .map((record) => finite(record[key] as NumberLike))
        .filter((value): value is number => value !== null);
    if (values.length === 0) {
        return null;
    }
    return Math.max(...values);
}

function format(value: number | null): string {
    return value === null ? "n/a" : value.toFixed(6);
}

function ratio(current: number | null, baseline: number | null): string {
    if (current === null || baseline === null || baseline === 0) {
        return "n/a";
    }
    return `${(current / baseline).toFixed(3)}x`;
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const frey = await loadFreyRecord(args.freyJsonl, args.runId);
    const earth = await loadEarthRecords(args.earthJson);

    console.log("=== Plate Shape Earth Comparison ===");
    console.log(`frey_run_id=${frey.run_id ?? "n/a"}`);
    console.log(`frey_seed=${frey.seed ?? "n/a"}`);
    if (frey.seed === "earth") {
        console.log(
            "warning=seed earth uses the hand-authored earth_preset branch; use a generated seed for Frey-vs-Earth plate-shape comparison.",
        );
    }
    console.log(`earth_times_ma=${earth.map((record) => record.time_ma ?? "n/a").join(",")}`);
    console.log("");

    for (const comparison of COMPARISONS) {
        const freyMax = finite(frey.plate_shape?.[comparison.freyMaxKey]);
        const freyTop8 = finite(frey.plate_shape?.[comparison.freyTop8Key]) ?? freyMax;
        const freyArea = finite(frey.plate_shape?.[comparison.freyAreaKey]) ?? freyMax;
        const top8P99 = maxEarth(earth, comparison.top8Key);
        const areaP99 = maxEarth(earth, comparison.areaKey);
        console.log(
            `${comparison.label}: frey_top8_p99=${format(freyTop8)} earth_top8_p99_max=${format(top8P99)} ratio=${ratio(freyTop8, top8P99)} frey_area_ge_1pct_p99=${format(freyArea)} earth_area_ge_1pct_p99_max=${format(areaP99)} ratio=${ratio(freyArea, areaP99)} frey_max=${format(freyMax)}`,
        );
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
