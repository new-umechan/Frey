import { randomUUID } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { spawn } from "node:child_process";

type NumberLike = number | null | undefined;

interface Args {
    runs: number;
    suite: string;
    jsonl: string;
    writeBaseline: string | null;
}

interface ClimateRecord {
    run_id?: string;
    runtime?: {
        climate_step_ms?: NumberLike;
    };
    runtime_stats?: {
        count?: number;
        median_ms?: NumberLike;
        p95_ms?: NumberLike;
    };
}

function parseArgs(argv: string[]): Args {
    const args: Args = {
        runs: 5,
        suite: "climate_solo",
        jsonl: "benches/results/climate_main_scores.jsonl",
        writeBaseline: null,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--":
            break;
        case "--runs":
            args.runs = Math.max(1, Number(next ?? args.runs));
            i += 1;
            break;
        case "--suite":
            args.suite = String(next ?? args.suite);
            i += 1;
            break;
        case "--jsonl":
            args.jsonl = String(next ?? args.jsonl);
            i += 1;
            break;
        case "--write-baseline":
            args.writeBaseline = String(next ?? "");
            i += 1;
            break;
        case "--help":
            console.error("Usage: tsx benches/scripts/bench-climate-series.ts [options]");
            console.error("  --runs <n>");
            console.error("  --suite <name>");
            console.error("  --jsonl <path>");
            console.error("  --write-baseline <path>");
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }
    return args;
}

function runBenchOnce(args: { suite: string; runId: string; index: number; total: number }): Promise<void> {
    return new Promise((resolvePromise, rejectPromise) => {
        const child = spawn(
            "pnpm",
            ["run", "bench", "--suite", args.suite],
            {
                stdio: "inherit",
                env: {
                    ...process.env,
                    CLIMATE_BENCH_RUN_ID: args.runId,
                    CLIMATE_BENCH_REPEAT_INDEX: String(args.index),
                    CLIMATE_BENCH_REPEAT_TOTAL: String(args.total),
                    CLIMATE_BENCH_GIT_COMMIT: process.env.CLIMATE_BENCH_GIT_COMMIT ?? "",
                },
            },
        );
        child.on("error", rejectPromise);
        child.on("exit", (code, signal) => {
            if (signal) {
                rejectPromise(new Error(`bench terminated by signal: ${signal}`));
                return;
            }
            if (code !== 0) {
                rejectPromise(new Error(`bench failed with exit code ${code}`));
                return;
            }
            resolvePromise();
        });
    });
}

async function loadJsonlRecords(pathname: string): Promise<ClimateRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as ClimateRecord);
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

function extractRuntimeMs(record: ClimateRecord): number | null {
    return toFinite(record.runtime?.climate_step_ms);
}

async function writeBaseline(pathname: string, record: ClimateRecord): Promise<void> {
    const outputPath = resolve(pathname);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(record, null, 2)}\n`, "utf8");
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const runId = `series-${Date.now()}-${randomUUID().slice(0, 8)}`;

    for (let i = 0; i < args.runs; i += 1) {
        console.error(`[climate-series] run ${i + 1}/${args.runs} run_id=${runId}`);
        await runBenchOnce({ suite: args.suite, runId, index: i + 1, total: args.runs });
    }

    const records = await loadJsonlRecords(args.jsonl);
    const series = records.filter((record) => record.run_id === runId);
    if (series.length === 0) {
        throw new Error(`No records found for run_id=${runId}`);
    }
    const runtimeSeries = series
        .map((record) => extractRuntimeMs(record))
        .filter((value): value is number => value !== null);
    if (runtimeSeries.length === 0) {
        throw new Error("No runtime values found in current series");
    }

    const runtimeMedian = median(runtimeSeries);
    const runtimeP95 = percentile(runtimeSeries, 0.95);
    console.log(`run_id=${runId}`);
    console.log(`series_count=${series.length}`);
    console.log(`runtime_median_ms=${runtimeMedian.toFixed(6)}`);
    console.log(`runtime_p95_ms=${runtimeP95.toFixed(6)}`);

    if (args.writeBaseline) {
        const latest = series[series.length - 1];
        const baselineRecord: ClimateRecord = {
            ...latest,
            runtime_stats: {
                count: series.length,
                median_ms: runtimeMedian,
                p95_ms: runtimeP95,
            },
        };
        await writeBaseline(args.writeBaseline, baselineRecord);
        console.log(`baseline_written=${resolve(args.writeBaseline)}`);
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
