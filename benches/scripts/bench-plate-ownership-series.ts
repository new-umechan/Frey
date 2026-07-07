import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { spawn } from "node:child_process";

interface Args {
    seeds: string[];
    ticks: number;
    recordEvery: number;
    level: number;
    out: string;
    cargoManifest: string;
}

interface BenchRecord {
    run_id?: string;
    seed?: string;
    level?: number;
    samples: TickRecord[];
}

interface TickRecord {
    tick: number;
    plate_count: number;
    mean_boundary_complexity_growth?: number;
    max_boundary_complexity_growth?: number;
    persistent_boundary_complexity_growth_plate_ratio?: number;
    mean_euler_rotation_residual_ratio?: number;
    reciprocal_churn_ratio?: number;
    mean_abs_plate_area_delta_ratio?: number;
    max_abs_plate_area_delta_ratio?: number;
    max_plate_area_growth_from_initial?: number;
    max_enclosed_plate_risk?: number;
    max_appendage_isolation_risk?: number;
}

interface GateRow {
    seed: string;
    current: TickRecord;
}

const DEFAULT_SEEDS = ["alpha", "beta", "gamma", "delta"];

function parseArgs(argv: string[]): Args {
    const args: Args = {
        seeds: DEFAULT_SEEDS,
        ticks: 160,
        recordEvery: 1,
        level: 6,
        out: "benches/results/plate_ownership_series.jsonl",
        cargoManifest: "rust/Cargo.toml",
    };
    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--":
            break;
        case "--seeds":
            args.seeds = String(next ?? "")
                .split(",")
                .map((seed) => seed.trim())
                .filter(Boolean);
            i += 1;
            break;
        case "--ticks":
            args.ticks = Math.max(1, Number(next ?? args.ticks));
            i += 1;
            break;
        case "--record-every":
            args.recordEvery = Math.max(1, Number(next ?? args.recordEvery));
            i += 1;
            break;
        case "--level":
            args.level = Math.max(0, Number(next ?? args.level));
            i += 1;
            break;
        case "--out":
            args.out = String(next ?? args.out);
            i += 1;
            break;
        case "--cargo-manifest":
            args.cargoManifest = String(next ?? args.cargoManifest);
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
    if (args.seeds.length === 0) {
        throw new Error("--seeds must include at least one seed");
    }
    return args;
}

function printHelp() {
    console.error("Usage: tsx benches/scripts/bench-plate-ownership-series.ts [options]");
    console.error("  --seeds <csv>          default: alpha,beta,gamma,delta");
    console.error("  --ticks <n>            default: 160");
    console.error("  --record-every <n>     default: 1");
    console.error("  --level <n>            default: 6");
    console.error("  --out <path>           default: benches/results/plate_ownership_series.jsonl");
    console.error("  --cargo-manifest <path>");
}

function runOne(args: Args, seed: string, runId: string): Promise<void> {
    return new Promise((resolvePromise, rejectPromise) => {
        const child = spawn(
            "cargo",
            [
                "run",
                "--manifest-path",
                args.cargoManifest,
                "--bin",
                "crust_plate_count_series",
            ],
            {
                stdio: "inherit",
                env: {
                    ...process.env,
                    CRUST_PLATE_SERIES_SEED: seed,
                    CRUST_PLATE_SERIES_LEVEL: String(args.level),
                    CRUST_PLATE_SERIES_TICKS: String(args.ticks),
                    CRUST_PLATE_SERIES_RECORD_EVERY: String(args.recordEvery),
                    CRUST_PLATE_SERIES_BENCH_OUT: args.out,
                    CRUST_PLATE_SERIES_RUN_ID: `${runId}-${seed}`,
                },
            },
        );
        child.on("error", rejectPromise);
        child.on("exit", (code, signal) => {
            if (signal) {
                rejectPromise(new Error(`plate ownership run terminated by signal: ${signal}`));
                return;
            }
            if (code !== 0) {
                rejectPromise(new Error(`plate ownership run failed with exit code ${code}`));
                return;
            }
            resolvePromise();
        });
    });
}

async function loadRecords(pathname: string): Promise<BenchRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as BenchRecord)
        .filter((record) => Array.isArray(record.samples));
}

function findSample(record: BenchRecord, tick: number): TickRecord {
    const sample = record.samples.find((candidate) => candidate.tick === tick);
    if (!sample) {
        throw new Error(`No tick=${tick} sample for run_id=${record.run_id ?? "unknown"}`);
    }
    return sample;
}

function latestRecord(records: BenchRecord[], runId: string): BenchRecord {
    const matches = records.filter((record) => record.run_id === runId);
    if (matches.length === 0) {
        throw new Error(`No record found for run_id=${runId}`);
    }
    return matches[matches.length - 1];
}

function finite(value: number | undefined): number {
    return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function status(row: GateRow): "pass" | "warn" {
    const complexitySafe = finite(row.current.max_boundary_complexity_growth) <= 1.25;
    const persistenceSafe =
        finite(row.current.persistent_boundary_complexity_growth_plate_ratio) <= 0.01;
    const areaGrowthSafe = finite(row.current.max_plate_area_growth_from_initial) <= 2.0;
    const areaDeltaSafe = finite(row.current.max_abs_plate_area_delta_ratio) <= 0.05;
    const enclosureSafe = finite(row.current.max_enclosed_plate_risk) <= 0.8;
    return complexitySafe && persistenceSafe && areaGrowthSafe && areaDeltaSafe && enclosureSafe
        ? "pass"
        : "warn";
}

function fmt(value: number | undefined): string {
    return finite(value).toFixed(6);
}

function printRows(rows: GateRow[]) {
    console.log("| seed | status | max_complexity | persistent | max_area_growth | max_area_delta | max_enclosed | max_appendage | mean_euler_residual |");
    console.log("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for (const row of rows) {
        console.log(
            [
                `| ${row.seed}`,
                status(row),
                fmt(row.current.max_boundary_complexity_growth),
                fmt(row.current.persistent_boundary_complexity_growth_plate_ratio),
                fmt(row.current.max_plate_area_growth_from_initial),
                fmt(row.current.max_abs_plate_area_delta_ratio),
                fmt(row.current.max_enclosed_plate_risk),
                fmt(row.current.max_appendage_isolation_risk),
                fmt(row.current.mean_euler_rotation_residual_ratio),
                "|",
            ].join(" | "),
        );
    }
    const warnCount = rows.filter((row) => status(row) === "warn").length;
    console.log(`summary_total=${rows.length}`);
    console.log(`summary_warn=${warnCount}`);
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const runId = `plate-ownership-${Date.now()}-${randomUUID().slice(0, 8)}`;
    for (const seed of args.seeds) {
        console.error(`[plate-ownership-series] seed=${seed} run_id=${runId}`);
        await runOne(args, seed, runId);
    }

    const records = await loadRecords(args.out);
    const rows = args.seeds.map((seed) => {
        const current = latestRecord(records, `${runId}-${seed}`);
        return {
            seed,
            current: findSample(current, args.ticks),
        };
    });
    console.log(`run_id=${runId}`);
    console.log(`ticks=${args.ticks}`);
    console.log(`record_every=${args.recordEvery}`);
    printRows(rows);
}

main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
