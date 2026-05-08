import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_mesh,
} from "../../../generated/wasm/web/frey_wasm";
import { createPerfProfile } from "../../../web/src/app/perf/recorder";
import { createPerfRunner } from "../../../web/src/app/perf/runner";
import { type VerificationMode } from "../../../web/src/app/perf/controller-state";
import { TERRAIN_LEVEL, TERRAIN_PARAMS } from "../../../web/src/interface/params/terrain";

const DEFAULT_THRESHOLD = 0.10;
const METRIC_NOISE_FLOOR_MS = 0.01;
const execFileAsync = promisify(execFile);
const HISTORY_FILE_PATH = resolve("tests/perf/history/perf-history.jsonl");
const PERF_LANES = ["wasm", "worker", "native"] as const;
type PerfLane = (typeof PERF_LANES)[number];
const VERIFICATION_MODES: VerificationMode[] = [
    "interactive",
    "headless_metrics",
    "scientific_benchmark",
];

interface GitMeta {
    commit: string;
    branch: string;
}

interface InitWasmOutputLike {
    memory?: {
        buffer?: {
            byteLength?: number;
        };
    };
}

function parseNumber(value: unknown, flagName: string): number {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
        throw new Error(`${flagName} must be a finite number`);
    }
    return parsed;
}

function parseArgs(argv: string[]) {
    const args = {
        ticks: 32,
        seed: "alpha",
        surfaceMode: "globe",
        viewMode: "normal",
        sampleInterval: 4,
        level: TERRAIN_LEVEL,
        out: null as string | null,
        baseline: null as string | null,
        threshold: DEFAULT_THRESHOLD,
        thresholdTickTotal: null as number | null,
        thresholdStepWorld: null as number | null,
        thresholdStepClimate: null as number | null,
        thresholdStepGeologyRiver: null as number | null,
        progress: false,
        noGeometry: false,
        profileEveryTick: false,
        geometryUpdateMinChangedRatio: 0,
        verificationMode: "interactive" as VerificationMode,
        lane: "wasm" as PerfLane,
        record: false,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        if (token === "--") {
            continue;
        }
        switch (token) {
        case "--ticks":
            args.ticks = Math.max(1, Math.floor(parseNumber(next, "--ticks")));
            i += 1;
            break;
        case "--seed":
            args.seed = String(next ?? "alpha");
            i += 1;
            break;
        case "--surface-mode":
            args.surfaceMode = String(next ?? "globe");
            i += 1;
            break;
        case "--view-mode":
            args.viewMode = String(next ?? "normal");
            i += 1;
            break;
        case "--sample-interval":
            args.sampleInterval = Math.max(1, Math.floor(parseNumber(next, "--sample-interval")));
            i += 1;
            break;
        case "--level":
            (args as any).level = Math.max(0, Math.floor(parseNumber(next, "--level")));
            i += 1;
            break;
        case "--out":
            args.out = String(next);
            i += 1;
            break;
        case "--baseline":
            args.baseline = String(next);
            i += 1;
            break;
        case "--threshold":
            args.threshold = Math.max(0, parseNumber(next, "--threshold"));
            i += 1;
            break;
        case "--threshold-tick-total":
            args.thresholdTickTotal = Math.max(0, parseNumber(next, "--threshold-tick-total"));
            i += 1;
            break;
        case "--threshold-step-world":
            args.thresholdStepWorld = Math.max(0, parseNumber(next, "--threshold-step-world"));
            i += 1;
            break;
        case "--threshold-step-climate":
            args.thresholdStepClimate = Math.max(0, parseNumber(next, "--threshold-step-climate"));
            i += 1;
            break;
        case "--threshold-step-geology-river":
            args.thresholdStepGeologyRiver = Math.max(0, parseNumber(next, "--threshold-step-geology-river"));
            i += 1;
            break;
        case "--progress":
            args.progress = true;
            break;
        case "--no-geometry":
            args.noGeometry = true;
            break;
        case "--profile-every-tick":
            args.profileEveryTick = true;
            break;
        case "--geometry-update-min-changed-ratio":
            args.geometryUpdateMinChangedRatio = Math.max(
                0,
                Math.min(1, parseNumber(next, "--geometry-update-min-changed-ratio")),
            );
            i += 1;
            break;
        case "--verification-mode": {
            const mode = String(next ?? "");
            if (!VERIFICATION_MODES.includes(mode as VerificationMode)) {
                throw new Error(
                    `--verification-mode must be one of: ${VERIFICATION_MODES.join(", ")}`,
                );
            }
            args.verificationMode = mode as VerificationMode;
            i += 1;
            break;
        }
        case "--lane": {
            const lane = String(next ?? "");
            if (!PERF_LANES.includes(lane as PerfLane)) {
                throw new Error(`--lane must be one of: ${PERF_LANES.join(", ")}`);
            }
            args.lane = lane as PerfLane;
            i += 1;
            break;
        }
        case "--record":
            args.record = true;
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
    console.error("Usage: node tests/perf/scripts/perf.mjs [options]");
    console.error("  --ticks <n>");
    console.error("  --seed <seed>");
    console.error("  --surface-mode <globe|plane>");
    console.error("  --view-mode <normal|plates|mantle|climate>");
    console.error("  --sample-interval <n>");
    console.error("  --level <n>");
    console.error("  --out <path>");
    console.error("  --baseline <path>");
    console.error("  --threshold <ratio>");
    console.error("  --threshold-tick-total <ratio>");
    console.error("  --threshold-step-world <ratio>");
    console.error("  --threshold-step-climate <ratio>");
    console.error("  --threshold-step-geology-river <ratio>");
    console.error("  --progress");
    console.error("  --no-geometry");
    console.error("  --profile-every-tick");
    console.error("  --geometry-update-min-changed-ratio <0..1>");
    console.error("  --verification-mode <interactive|headless_metrics|scientific_benchmark>");
    console.error("  --lane <wasm|worker|native>");
    console.error("  --record");
}

function getPathValue(obj: Record<string, unknown>, path: string): unknown {
    const keys = path.split(".");
    let current: unknown = obj;
    for (const key of keys) {
        if (current == null || typeof current !== "object" || !(key in current)) {
            return undefined;
        }
        current = (current as Record<string, unknown>)[key];
    }
    return current;
}

function formatRatio(value: number): string {
    return `${(value * 100).toFixed(2)}%`;
}

async function loadBaseline(pathname: string): Promise<unknown> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as unknown;
}

interface RegressionArgs {
    thresholdTickTotal: number | null;
    thresholdStepWorld: number | null;
    thresholdStepClimate: number | null;
    thresholdStepGeologyRiver: number | null;
    threshold: number;
}

interface RegressionResult {
    warnings: string[];
    regressions: Array<{
        label: string;
        baselineValue: number;
        currentValue: number;
        threshold: number;
        allowedMax: number;
    }>;
}

interface RegressionSpec {
    label: string;
    path: string;
    threshold: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return value != null && typeof value === "object";
}

function buildRegressionSpecs(
    current: Record<string, unknown>,
    baseline: Record<string, unknown>,
    args: RegressionArgs,
): RegressionSpec[] {
    const hydrologyMetricPath = Number.isFinite(Number(getPathValue(current, "metrics.step_hydrology.mean")))
        && Number.isFinite(Number(getPathValue(baseline, "metrics.step_hydrology.mean")))
        ? "metrics.step_hydrology.mean"
        : "metrics.step_geology_river.mean";
    const specs: RegressionSpec[] = [
        {
            label: "tick_total.mean",
            path: "metrics.tick_total.mean",
            threshold: args.thresholdTickTotal ?? args.threshold,
        },
        {
            label: "exec_world.mean",
            path: "metrics.exec_world.mean",
            threshold: args.thresholdStepWorld ?? args.threshold,
        },
        {
            label: "step_climate.mean",
            path: "metrics.step_climate.mean",
            threshold: args.thresholdStepClimate ?? args.threshold,
        },
        {
            label: "step_hydrology.mean",
            path: hydrologyMetricPath,
            threshold: args.thresholdStepGeologyRiver ?? args.threshold,
        },
    ];

    const currentNormalized = getPathValue(current, "diagnostics.normalized");
    const baselineNormalized = getPathValue(baseline, "diagnostics.normalized");
    if (!isRecord(currentNormalized) || !isRecord(baselineNormalized)) {
        return specs;
    }

    return specs.concat([
        {
            label: "diagnostics.module_geology_exec_time_ms_total",
            path: "diagnostics.normalized.module_geology_exec_time_ms_total",
            threshold: args.thresholdStepWorld ?? args.threshold,
        },
        {
            label: "diagnostics.module_climate_exec_time_ms_total",
            path: "diagnostics.normalized.module_climate_exec_time_ms_total",
            threshold: args.thresholdStepClimate ?? args.threshold,
        },
        {
            label: "diagnostics.module_hydrology_exec_time_ms_total",
            path: "diagnostics.normalized.module_hydrology_exec_time_ms_total",
            threshold: args.thresholdStepGeologyRiver ?? args.threshold,
        },
    ]);
}

function evaluateRegression(
    current: unknown,
    baseline: unknown,
    args: RegressionArgs,
): RegressionResult {
    const currentRecord = isRecord(current) ? current : {};
    const baselineRecord = isRecord(baseline) ? baseline : {};
    const specs = buildRegressionSpecs(currentRecord, baselineRecord, args);

    const warnings = [];
    const regressions = [];

    for (const spec of specs) {
        const currentValue = Number(getPathValue(currentRecord, spec.path));
        const baselineValue = Number(getPathValue(baselineRecord, spec.path));

        if (!Number.isFinite(currentValue) || !Number.isFinite(baselineValue)) {
            warnings.push(`skip ${spec.label}: missing numeric value`);
            continue;
        }
        if (baselineValue <= 0) {
            warnings.push(`skip ${spec.label}: baseline <= 0`);
            continue;
        }
        if (baselineValue < METRIC_NOISE_FLOOR_MS) {
            warnings.push(
                `skip ${spec.label}: baseline below noise floor (${METRIC_NOISE_FLOOR_MS}ms)`,
            );
            continue;
        }

        const allowedMax = baselineValue * (1 + spec.threshold);
        if (currentValue > allowedMax) {
            regressions.push({
                label: spec.label,
                baselineValue,
                currentValue,
                threshold: spec.threshold,
                allowedMax,
            });
        }
    }

    return {
        warnings,
        regressions,
    };
}

async function initWasmForNode() {
    const wasmPath = new URL("../../../generated/wasm/web/frey_wasm_bg.wasm", import.meta.url);
    const wasmBytes = await readFile(wasmPath);
    try {
        return await initWasm({ module_or_path: wasmBytes });
    } catch {
        return await initWasm(wasmBytes);
    }
}

async function getGitMeta(): Promise<GitMeta> {
    const fallback = {
        commit: "unknown",
        branch: "unknown",
    };
    try {
        const [{ stdout: commitOut }, { stdout: branchOut }] = await Promise.all([
            execFileAsync("git", ["rev-parse", "--short", "HEAD"]),
            execFileAsync("git", ["rev-parse", "--abbrev-ref", "HEAD"]),
        ]);
        const commit = commitOut.trim();
        const branch = branchOut.trim();
        return {
            commit: commit.length > 0 ? commit : fallback.commit,
            branch: branch.length > 0 ? branch : fallback.branch,
        };
    } catch {
        return fallback;
    }
}

function getWasmLinearMemoryMb(initOutput: InitWasmOutputLike | undefined): number {
    const byteLength = initOutput?.memory?.buffer?.byteLength;
    if (!Number.isFinite(byteLength)) {
        return 0;
    }
    return Math.round(((byteLength as number) / (1024 * 1024)) * 1000) / 1000;
}

async function appendHistoryRecord(record: Record<string, unknown>) {
    const historyDir = resolve("tests/perf/history");
    await mkdir(historyDir, { recursive: true });
    await appendFile(HISTORY_FILE_PATH, `${JSON.stringify(record)}\n`, "utf8");
}

async function runWasmBenchmark(args: ReturnType<typeof parseArgs>) {
    const wasmInitStarted = performance.now();
    const wasmInitOutput = await initWasmForNode();
    const wasmInitMs = Math.round((performance.now() - wasmInitStarted) * 1000) / 1000;
    const wasmLinearMemoryMb = getWasmLinearMemoryMb(wasmInitOutput as InitWasmOutputLike);

    const runner = createPerfRunner({
        WorldSimController,
        build_render_positions,
        generate_mesh,
    });

    const profile = createPerfProfile({
        tickCount: args.ticks,
        seed: args.seed,
        surfaceMode: args.surfaceMode,
        viewMode: args.viewMode,
    });

    const result = await runner.runBenchmark({
        runId: "cli",
        profile,
        level: args.level,
        terrainParams: TERRAIN_PARAMS,
        sampleInterval: args.sampleInterval,
        profileEveryTick: args.profileEveryTick,
        skipGeometry: args.noGeometry,
        geometryUpdateMinChangedRatio: args.geometryUpdateMinChangedRatio,
        verificationMode: args.verificationMode,
        meta: {
            user_agent: `${args.lane} node ${process.version}`,
            timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        },
        onProgress(payload: { status: string }) {
            if (args.progress) {
                console.error(payload.status);
            }
        },
        onWarning(message: string) {
            console.error(`[bench warning] ${message}`);
        },
    });

    return {
        result,
        wasmInitMs,
        wasmLinearMemoryMb,
    };
}

async function runNativeBenchmark(args: ReturnType<typeof parseArgs>) {
    const nativeArgs = [
        "run",
        "--manifest-path",
        "rust/Cargo.toml",
        "--bin",
        "perf_native",
        "--",
        "--ticks",
        String(args.ticks),
        "--seed",
        args.seed,
        "--level",
        String(args.level),
        "--sample-interval",
        String(args.sampleInterval),
    ];
    const { stdout } = await execFileAsync("cargo", nativeArgs, {
        maxBuffer: 1024 * 1024 * 8,
    });
    return {
        result: JSON.parse(stdout) as Record<string, unknown>,
        wasmInitMs: 0,
        wasmLinearMemoryMb: 0,
    };
}

async function runWorkerBenchmark(args: ReturnType<typeof parseArgs>) {
    return await runWasmBenchmark(args);
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const laneRunner: Record<PerfLane, (args: ReturnType<typeof parseArgs>) => Promise<{
        result: Record<string, unknown>;
        wasmInitMs: number;
        wasmLinearMemoryMb: number;
    }>> = {
        wasm: runWasmBenchmark,
        worker: runWorkerBenchmark,
        native: runNativeBenchmark,
    };
    const laneOutput = await laneRunner[args.lane](args);
    const result = laneOutput.result;
    const output = JSON.stringify(result, null, 2);
    process.stdout.write(`${output}\n`);

    if (args.out) {
        await writeFile(resolve(args.out), output);
    }

    if (args.record) {
        const timestamp = (result as { meta?: { generated_at?: string } })?.meta?.generated_at
            ?? new Date().toISOString();
        const gitMeta = await getGitMeta();
        const record = {
            timestamp,
            commit: gitMeta.commit,
            branch: gitMeta.branch,
            lane: args.lane,
            profile: result.profile ?? {},
            totals: result.totals ?? {},
            metrics: result.metrics ?? {},
            diagnostics: result.diagnostics ?? {},
            memory: {
                wasm_linear_memory_mb: laneOutput.wasmLinearMemoryMb,
            },
            runtime: {
                wasm_init_ms: laneOutput.wasmInitMs,
            },
        };
        await appendHistoryRecord(record as Record<string, unknown>);
    }

    if (args.baseline) {
        const baseline = await loadBaseline(args.baseline);
        const comparison = evaluateRegression(result, baseline, args);

        for (const warning of comparison.warnings) {
            console.error(`[bench compare] ${warning}`);
        }

        if (comparison.regressions.length > 0) {
            for (const regression of comparison.regressions) {
                console.error(
                    `[bench compare] ${regression.label}: current=${regression.currentValue}, baseline=${regression.baselineValue}, threshold=${formatRatio(regression.threshold)}, allowed_max=${regression.allowedMax}`,
                );
            }
            process.exitCode = 1;
        }
        console.error(`[bench compare] regressions=${comparison.regressions.length}`);
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
