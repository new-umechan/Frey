import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { spawn } from "node:child_process";

type Horizon = "short" | "mid" | "long";
type NumberLike = number | null | undefined;

interface Args {
    runs: number;
    suite: string;
    jsonl: string;
    horizon: Horizon | "all";
    modernRef: string | null;
    paleoRef: string | null;
    ticksShort: number;
    ticksMid: number;
    ticksLong: number;
}

interface GlaciologySeriesRecord {
    run_id?: string;
    horizon?: string;
    runtime?: {
        glaciology_step_ms_median?: NumberLike;
        glaciology_step_ms_p95?: NumberLike;
    };
    metrics?: {
        sle_mm?: NumberLike;
        grid_spearman?: NumberLike;
        grid_rmse?: NumberLike;
    };
}

function parseArgs(argv: string[]): Args {
    const args: Args = {
        runs: 3,
        suite: "glaciology_sea_level_series",
        jsonl: "benches/results/glaciology_sea_level_series_scores.jsonl",
        horizon: "all",
        modernRef: null,
        paleoRef: null,
        ticksShort: 32,
        ticksMid: 256,
        ticksLong: 1024,
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
        case "--horizon": {
            const value = String(next ?? args.horizon);
            if (value !== "short" && value !== "mid" && value !== "long" && value !== "all") {
                throw new Error(`Unknown horizon: ${value}`);
            }
            args.horizon = value;
            i += 1;
            break;
        }
        case "--modern-ref":
            args.modernRef = String(next ?? "");
            i += 1;
            break;
        case "--paleo-ref":
            args.paleoRef = String(next ?? "");
            i += 1;
            break;
        case "--ticks-short":
            args.ticksShort = Math.max(1, Number(next ?? args.ticksShort));
            i += 1;
            break;
        case "--ticks-mid":
            args.ticksMid = Math.max(1, Number(next ?? args.ticksMid));
            i += 1;
            break;
        case "--ticks-long":
            args.ticksLong = Math.max(1, Number(next ?? args.ticksLong));
            i += 1;
            break;
        case "--help":
            console.error("Usage: tsx benches/scripts/bench-glaciology-series.ts [options]");
            console.error("  --runs <n>");
            console.error("  --suite <name>");
            console.error("  --jsonl <path>");
            console.error("  --horizon <short|mid|long|all>");
            console.error("  --modern-ref <path>");
            console.error("  --paleo-ref <path>");
            console.error("  --ticks-short <n>");
            console.error("  --ticks-mid <n>");
            console.error("  --ticks-long <n>");
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }
    return args;
}

function resolveHorizons(horizon: Args["horizon"]): Horizon[] {
    if (horizon === "all") {
        return ["short", "mid", "long"];
    }
    return [horizon];
}

function resolveTicks(args: Args, horizon: Horizon): number {
    switch (horizon) {
    case "short":
        return args.ticksShort;
    case "mid":
        return args.ticksMid;
    case "long":
        return args.ticksLong;
    }
}

function runBenchOnce(args: {
    suite: string;
    runId: string;
    horizon: Horizon;
    ticks: number;
    index: number;
    total: number;
    modernRef: string | null;
    paleoRef: string | null;
}): Promise<void> {
    return new Promise((resolvePromise, rejectPromise) => {
        const child = spawn(
            "pnpm",
            ["run", "bench", "--suite", args.suite],
            {
                stdio: "inherit",
                env: {
                    ...process.env,
                    GLACIOLOGY_SERIES_RUN_ID: `${args.runId}-${args.horizon}`,
                    GLACIOLOGY_SERIES_REPEAT_INDEX: String(args.index),
                    GLACIOLOGY_SERIES_REPEAT_TOTAL: String(args.total),
                    GLACIOLOGY_SERIES_HORIZON: args.horizon,
                    GLACIOLOGY_SERIES_TICKS: String(args.ticks),
                    GLACIOLOGY_SERIES_MODERN_REF_PATH: args.modernRef ?? "",
                    GLACIOLOGY_SERIES_PALEO_REF_PATH: args.paleoRef ?? "",
                    GLACIOLOGY_SERIES_GIT_COMMIT: process.env.GLACIOLOGY_SERIES_GIT_COMMIT ?? "",
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

async function loadJsonlRecords(pathname: string): Promise<GlaciologySeriesRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as GlaciologySeriesRecord);
}

function toFinite(value: NumberLike): number | null {
    const numeric = Number(value);
    return Number.isFinite(numeric) ? numeric : null;
}

function median(values: number[]): number {
    if (values.length === 0) {
        return Number.NaN;
    }
    const sorted = [...values].sort((a, b) => a - b);
    const center = Math.floor(sorted.length / 2);
    if (sorted.length % 2 === 0) {
        return (sorted[center - 1] + sorted[center]) * 0.5;
    }
    return sorted[center];
}

function summarize(records: GlaciologySeriesRecord[], horizon: Horizon): string {
    const selected = records.filter((record) => record.horizon === horizon);
    const runtimeSeries = selected
        .map((record) => toFinite(record.runtime?.glaciology_step_ms_p95))
        .filter((value): value is number => value !== null);
    const sleSeries = selected
        .map((record) => toFinite(record.metrics?.sle_mm))
        .filter((value): value is number => value !== null);
    const rhoSeries = selected
        .map((record) => toFinite(record.metrics?.grid_spearman))
        .filter((value): value is number => value !== null);
    const rmseSeries = selected
        .map((record) => toFinite(record.metrics?.grid_rmse))
        .filter((value): value is number => value !== null);

    const runtimeMedian = median(runtimeSeries);
    const sleMedian = median(sleSeries);
    const rhoMedian = median(rhoSeries);
    const rmseMedian = median(rmseSeries);

    return [
        `horizon=${horizon}`,
        `count=${selected.length}`,
        `runtime_p95_median_ms=${runtimeMedian.toFixed(6)}`,
        `sle_median_mm=${sleMedian.toFixed(6)}`,
        `grid_spearman_median=${rhoMedian.toFixed(6)}`,
        `grid_rmse_median=${rmseMedian.toFixed(6)}`,
    ].join(" ");
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const horizons = resolveHorizons(args.horizon);
    const runId = `series-${Date.now()}-${randomUUID().slice(0, 8)}`;

    for (const horizon of horizons) {
        const ticks = resolveTicks(args, horizon);
        for (let i = 0; i < args.runs; i += 1) {
            console.error(
                `[glaciology-series] horizon=${horizon} run ${i + 1}/${args.runs} run_id=${runId}`,
            );
            await runBenchOnce({
                suite: args.suite,
                runId,
                horizon,
                ticks,
                index: i + 1,
                total: args.runs,
                modernRef: args.modernRef,
                paleoRef: args.paleoRef,
            });
        }
    }

    const records = await loadJsonlRecords(args.jsonl);
    const current = records.filter(
        (record) => typeof record.run_id === "string" && record.run_id?.startsWith(runId),
    );
    if (current.length === 0) {
        throw new Error(`No records found for run_id prefix=${runId}`);
    }

    for (const horizon of horizons) {
        console.log(summarize(current, horizon));
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
