import { spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import initWasm, { WorldSimController } from "../../../generated/wasm/web/frey_wasm";
import { TERRAIN_LEVEL, TERRAIN_PARAMS } from "../../../web/src/interface/params/terrain";

const DEFAULT_TICKS = 32;
const DEFAULT_THRESHOLD = 0.005;
const DEFAULT_SEEDS = ["alpha"];
const DEFAULT_JOBS = 1;
const TRANSITION_MODE = "fixed_tick";
const ERA_BOUNDARIES = [0, 800, 1300, 1395, 1445];
const SEED_REGRESSION_SCRIPT_PATH = fileURLToPath(new URL("./seed-regression.ts", import.meta.url));
const METRIC_SPECS = [
    { key: "land_cells", sourceKey: "land_cells", flagSuffix: "land-cells" },
    { key: "height_mean", sourceKey: "mean_height", flagSuffix: "height-mean" },
    { key: "height_std", sourceKey: "height_std_dev", flagSuffix: "height-std" },
    { key: "max_river_flux", sourceKey: "max_river_flux", flagSuffix: "max-river-flux" },
    {
        key: "top10_river_flux_sum",
        sourceKey: "top10_river_flux_sum",
        flagSuffix: "top10-river-flux-sum",
    },
];

function parseNumber(value: unknown, flagName: string): number {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
        throw new Error(`${flagName} must be a finite number`);
    }
    return parsed;
}

function parseSeedsCsv(raw: unknown): string[] {
    if (typeof raw !== "string") {
        return [];
    }
    return raw
        .split(",")
        .map((seed) => seed.trim())
        .filter((seed) => seed.length > 0);
}

function parseArgs(argv: string[]) {
    const args = {
        ticks: DEFAULT_TICKS,
        seeds: [...DEFAULT_SEEDS],
        jobs: DEFAULT_JOBS,
        level: TERRAIN_LEVEL,
        out: null as string | null,
        baseline: null as string | null,
        check: false,
        threshold: DEFAULT_THRESHOLD,
        thresholdByMetric: {} as Record<string, number>,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        if (token === "--") {
            continue;
        }

        if (token.startsWith("--threshold-") && token !== "--threshold") {
            const suffix = token.slice("--threshold-".length);
            const spec = METRIC_SPECS.find((candidate) => candidate.flagSuffix === suffix);
            if (!spec) {
                throw new Error(`Unknown argument: ${token}`);
            }
            args.thresholdByMetric[spec.key] = Math.max(0, parseNumber(next, token));
            i += 1;
            continue;
        }

        switch (token) {
        case "--ticks":
            args.ticks = Math.max(1, Math.floor(parseNumber(next, "--ticks")));
            i += 1;
            break;
        case "--seeds": {
            const parsedSeeds = parseSeedsCsv(next);
            if (parsedSeeds.length === 0) {
                throw new Error("--seeds must include at least one seed");
            }
            args.seeds = parsedSeeds;
            i += 1;
            break;
        }
        case "--jobs":
            args.jobs = Math.max(1, Math.floor(parseNumber(next, "--jobs")));
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
        case "--check":
            args.check = true;
            break;
        case "--threshold":
            args.threshold = Math.max(0, parseNumber(next, "--threshold"));
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

    if (args.check && !args.baseline) {
        throw new Error("--check requires --baseline <path>");
    }

    return args;
}

function printHelp() {
    console.error("Usage: node tests/seed-regression/scripts/seed-regression.mjs [options]");
    console.error("  --seeds <csv>");
    console.error("  --jobs <n>");
    console.error("  --ticks <n>");
    console.error("  --level <n>");
    console.error("  --out <path>");
    console.error("  --baseline <path>");
    console.error("  --check");
    console.error("  --threshold <ratio>");
    for (const spec of METRIC_SPECS) {
        console.error(`  --threshold-${spec.flagSuffix} <ratio>`);
    }
}

async function initWasmForNode() {
    const wasmPath = new URL("../../../generated/wasm/web/frey_wasm_bg.wasm", import.meta.url);
    const wasmBytes = await readFile(wasmPath);
    try {
        await initWasm({ module_or_path: wasmBytes });
    } catch {
        await initWasm(wasmBytes);
    }
}

async function loadBaseline(pathname: string): Promise<unknown> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as unknown;
}

function collectMetricsFromResponse(metrics: unknown): Record<string, number> {
    const result: Record<string, number> = {};
    for (const spec of METRIC_SPECS) {
        const value = Number((metrics as Record<string, unknown>)?.[spec.sourceKey]);
        if (!Number.isFinite(value)) {
            throw new Error(`missing numeric metric from wasm response: ${spec.sourceKey}`);
        }
        result[spec.key] = value;
    }
    return result;
}

interface Thresholds {
    [key: string]: number;
}

function buildEffectiveThresholds(args: { thresholdByMetric: Record<string, number>; threshold: number }): Thresholds {
    const thresholds: Thresholds = {};
    for (const spec of METRIC_SPECS) {
        thresholds[spec.key] = args.thresholdByMetric[spec.key] ?? args.threshold;
    }
    return thresholds;
}

interface DiffResult {
    mode: "relative" | "absolute";
    diff: number;
}

function relativeOrAbsoluteDiff(currentValue: number, baselineValue: number): DiffResult {
    const absDiff = Math.abs(currentValue - baselineValue);
    if (baselineValue === 0) {
        return {
            mode: "absolute",
            diff: absDiff,
        };
    }
    return {
        mode: "relative",
        diff: absDiff / Math.abs(baselineValue),
    };
}

function normalizeSeedSet(seeds: unknown): string[] {
    if (!Array.isArray(seeds)) {
        return [];
    }
    return [...new Set(seeds.map((seed) => String(seed)))].sort();
}

interface MetaFailure {
    seed: string;
    metric: string;
    reason: string;
    expected: unknown;
    actual: unknown;
}

interface BaselineMeta {
    ticks: number;
    level: number;
    seeds: unknown;
    transition_mode: string;
    era_boundaries: unknown;
    eras_at_measurement: Record<string, string>;
}

function validateBaselineMeta(current: { meta: BaselineMeta }, baseline: { meta: BaselineMeta }): MetaFailure[] {
    const failures: MetaFailure[] = [];
    const baselineMeta = baseline?.meta ?? {};

    const baselineTicks = Number(baselineMeta.ticks);
    if (!Number.isFinite(baselineTicks) || baselineTicks !== current.meta.ticks) {
        failures.push({
            seed: "*",
            metric: "meta.ticks",
            reason: "baseline_meta_mismatch",
            expected: current.meta.ticks,
            actual: baselineMeta.ticks,
        });
    }

    const baselineLevel = Number(baselineMeta.level);
    if (!Number.isFinite(baselineLevel) || baselineLevel !== current.meta.level) {
        failures.push({
            seed: "*",
            metric: "meta.level",
            reason: "baseline_meta_mismatch",
            expected: current.meta.level,
            actual: baselineMeta.level,
        });
    }

    const currentSeeds = normalizeSeedSet(current.meta.seeds);
    const baselineSeeds = normalizeSeedSet(baselineMeta.seeds);
    const sameSeedSet = currentSeeds.length === baselineSeeds.length
        && currentSeeds.every((seed, index) => seed === baselineSeeds[index]);
    if (!sameSeedSet) {
        failures.push({
            seed: "*",
            metric: "meta.seeds",
            reason: "baseline_meta_mismatch",
            expected: currentSeeds.join(","),
            actual: baselineSeeds.join(","),
        });
    }

    const baselineTransitionMode = String(baselineMeta.transition_mode ?? "");
    if (baselineTransitionMode !== current.meta.transition_mode) {
        failures.push({
            seed: "*",
            metric: "meta.transition_mode",
            reason: "baseline_meta_mismatch",
            expected: current.meta.transition_mode,
            actual: baselineMeta.transition_mode,
        });
    }

    const currentBoundaries = Array.isArray(current.meta.era_boundaries)
        ? (current.meta.era_boundaries as number[]).map((v: number) => Number(v))
        : [];
    const baselineBoundaries = Array.isArray(baselineMeta.era_boundaries)
        ? (baselineMeta.era_boundaries as number[]).map((v: number) => Number(v))
        : [];
    const sameBoundaries = currentBoundaries.length === baselineBoundaries.length
        && currentBoundaries.every((value, index) => value === baselineBoundaries[index]);
    if (!sameBoundaries) {
        failures.push({
            seed: "*",
            metric: "meta.era_boundaries",
            reason: "baseline_meta_mismatch",
            expected: currentBoundaries.join(","),
            actual: baselineBoundaries.join(","),
        });
    }

    const currentEras = current.meta.eras_at_measurement ?? {};
    const baselineEras = baselineMeta.eras_at_measurement ?? {};
    for (const seed of currentSeeds) {
        const currentEra = typeof currentEras[seed] === "string" ? currentEras[seed] : "";
        const baselineEra = typeof baselineEras[seed] === "string" ? baselineEras[seed] : "";
        if (currentEra !== baselineEra) {
            failures.push({
                seed,
                metric: "meta.eras_at_measurement",
                reason: "baseline_meta_mismatch",
                expected: currentEra,
                actual: baselineEras[seed],
            });
        }
    }

    return failures;
}

interface Deviation {
    seed: string;
    metric: string;
    reason?: string;
    mode?: "relative" | "absolute";
    currentValue?: number;
    baselineValue?: number;
    diff?: number;
    threshold?: number;
    expected?: unknown;
    actual?: unknown;
}

interface EvaluationResult {
    warnings: string[];
    deviations: Deviation[];
}

function evaluateAgainstBaseline(
    current: { meta: BaselineMeta; results: Array<{ seed: string; metrics: Record<string, number> }> },
    baseline: { meta: BaselineMeta; results?: Array<{ seed: string; metrics: Record<string, number> }> },
    thresholds: Thresholds,
): EvaluationResult {
    const currentBySeed = new Map(current.results.map((entry) => [entry.seed, entry.metrics]));
    const baselineBySeed = new Map(
        (baseline?.results ?? []).map((entry) => [entry.seed, entry.metrics]),
    );

    const warnings: string[] = [];
    const deviations: Deviation[] = [];

    deviations.push(...validateBaselineMeta(current, baseline));
    if (deviations.length > 0) {
        return { warnings, deviations };
    }

    for (const seed of current.meta.seeds as string[]) {
        if (!baselineBySeed.has(seed)) {
            deviations.push({
                seed,
                metric: "*",
                reason: "missing_seed_in_baseline",
            });
            continue;
        }

        const currentMetrics = currentBySeed.get(seed);
        const baselineMetrics = baselineBySeed.get(seed);

        for (const spec of METRIC_SPECS) {
            const currentValue = Number(currentMetrics?.[spec.key]);
            const baselineValue = Number(baselineMetrics?.[spec.key]);
            if (!Number.isFinite(currentValue) || !Number.isFinite(baselineValue)) {
                deviations.push({
                    seed,
                    metric: spec.key,
                    reason: "missing_numeric_metric",
                });
                continue;
            }

            const threshold = Number(thresholds[spec.key]);
            const { mode, diff } = relativeOrAbsoluteDiff(currentValue, baselineValue);
            if (diff > threshold) {
                deviations.push({
                    seed,
                    metric: spec.key,
                    mode,
                    currentValue,
                    baselineValue,
                    diff,
                    threshold,
                });
            }
        }
    }

    for (const seed of baselineBySeed.keys()) {
        if (!currentBySeed.has(seed)) {
            warnings.push(`baseline has extra seed not in current result: ${seed}`);
        }
    }

    return { warnings, deviations };
}

interface SimulationResult {
    metrics: Record<string, number>;
    era: string;
}

interface CommandResult {
    code: number | null;
    signal: NodeJS.Signals | null;
    stdout: string;
    stderr: string;
}

function runCommandCapture(command: string, args: string[]): Promise<CommandResult> {
    return new Promise((resolvePromise, rejectPromise) => {
        const child = spawn(command, args, {
            cwd: resolve("."),
            stdio: ["ignore", "pipe", "pipe"],
            shell: process.platform === "win32",
        });
        let stdout = "";
        let stderr = "";

        child.stdout?.setEncoding("utf8");
        child.stderr?.setEncoding("utf8");

        child.stdout?.on("data", (chunk: string) => {
            stdout += chunk;
        });
        child.stderr?.on("data", (chunk: string) => {
            stderr += chunk;
        });

        child.on("error", rejectPromise);
        child.on("close", (code, signal) => {
            resolvePromise({
                code,
                signal,
                stdout,
                stderr,
            });
        });
    });
}

function collectMetricsFromOutputEntry(metrics: unknown): Record<string, number> {
    const result: Record<string, number> = {};
    for (const spec of METRIC_SPECS) {
        const value = Number((metrics as Record<string, unknown>)?.[spec.key]);
        if (!Number.isFinite(value)) {
            throw new Error(`missing numeric metric from subprocess output: ${spec.key}`);
        }
        result[spec.key] = value;
    }
    return result;
}

function parseSingleSeedSubprocessOutput(stdout: string, seed: string): SimulationResult {
    let parsed: unknown;
    try {
        parsed = JSON.parse(stdout);
    } catch (error) {
        throw new Error(
            `failed to parse subprocess output for seed=${seed}: ${error instanceof Error ? error.message : String(error)}`,
        );
    }

    const results = (parsed as { results?: Array<{ seed?: string; era?: unknown; metrics?: unknown }> })?.results;
    if (!Array.isArray(results)) {
        throw new Error(`invalid subprocess output for seed=${seed}: missing results array`);
    }

    const entry = results.find((candidate) => String(candidate?.seed ?? "") === seed);
    if (!entry) {
        throw new Error(`invalid subprocess output for seed=${seed}: missing seed result`);
    }

    return {
        metrics: collectMetricsFromOutputEntry(entry.metrics),
        era: String(entry.era ?? ""),
    };
}

async function runSeedSimulation(seed: string, ticks: number, level: number): Promise<SimulationResult> {
    const controller = new WorldSimController();
    const init = controller.init_world(seed, level, {
        geology_params: TERRAIN_PARAMS,
    });
    const worldId = init?.world_id;

    if (!worldId) {
        throw new Error(`init_world failed: missing world_id for seed=${seed}`);
    }

    if (ticks > 0) {
        controller.exec_world(worldId, ticks);
    }

    const metricsResponse = controller.get_metrics(worldId);
    return {
        metrics: collectMetricsFromResponse(metricsResponse),
        era: String(metricsResponse?.era ?? ""),
    };
}

async function runSeedSimulationInSubprocess(
    seed: string,
    args: { ticks: number; level: number },
): Promise<SimulationResult> {
    const commandArgs = [
        "exec",
        "tsx",
        SEED_REGRESSION_SCRIPT_PATH,
        "--seeds",
        seed,
        "--ticks",
        String(args.ticks),
        "--level",
        String(args.level),
        "--jobs",
        "1",
    ];
    const result = await runCommandCapture("pnpm", commandArgs);
    if (result.signal) {
        throw new Error(`subprocess terminated by signal (${result.signal}) for seed=${seed}`);
    }
    if (result.code !== 0) {
        const details = result.stderr.trim() || result.stdout.trim() || "no output";
        throw new Error(`subprocess failed for seed=${seed}: ${details}`);
    }
    return parseSingleSeedSubprocessOutput(result.stdout, seed);
}

async function mapWithConcurrency<T, U>(
    values: T[],
    concurrency: number,
    mapper: (value: T, index: number) => Promise<U>,
): Promise<U[]> {
    const workerCount = Math.max(1, Math.min(concurrency, values.length));
    const results = new Array<U>(values.length);
    let nextIndex = 0;

    const runWorker = async () => {
        while (nextIndex < values.length) {
            const currentIndex = nextIndex;
            nextIndex += 1;
            results[currentIndex] = await mapper(values[currentIndex], currentIndex);
        }
    };

    await Promise.all(Array.from({ length: workerCount }, () => runWorker()));
    return results;
}

async function runSeedSimulations(args: {
    seeds: string[];
    ticks: number;
    level: number;
    jobs: number;
}): Promise<Array<{ seed: string; era: string; metrics: Record<string, number> }>> {
    if (args.jobs <= 1 || args.seeds.length <= 1) {
        await initWasmForNode();
        const results = [];
        for (const seed of args.seeds) {
            const simulation = await runSeedSimulation(seed, args.ticks, args.level);
            results.push({
                seed,
                era: simulation.era,
                metrics: simulation.metrics,
            });
        }
        return results;
    }

    return mapWithConcurrency(args.seeds, args.jobs, async (seed) => {
        const simulation = await runSeedSimulationInSubprocess(seed, args);
        return {
            seed,
            era: simulation.era,
            metrics: simulation.metrics,
        };
    });
}

interface OutputData {
    meta: {
        generated_at: string;
        ticks: number;
        level: number;
        seeds: string[];
        thresholds: Thresholds;
        transition_mode: string;
        era_boundaries: number[];
        eras_at_measurement: Record<string, string>;
    };
    results: Array<{ seed: string; tick: number; era: string; metrics: Record<string, number> }>;
}

function buildOutput(args: { ticks: number; level: number; seeds: string[] }, thresholds: Thresholds, results: Array<{ seed: string; era: string; metrics: Record<string, number> }>): OutputData {
    const erasAtMeasurement: Record<string, string> = {};
    for (const result of results) {
        erasAtMeasurement[result.seed] = result.era;
    }

    return {
        meta: {
            generated_at: new Date().toISOString(),
            ticks: args.ticks,
            level: args.level,
            seeds: args.seeds,
            thresholds,
            transition_mode: TRANSITION_MODE,
            era_boundaries: [...ERA_BOUNDARIES],
            eras_at_measurement: erasAtMeasurement,
        },
        results: results.map((r, _i) => ({ seed: r.seed, tick: args.ticks, era: r.era, metrics: r.metrics })),
    };
}

async function main() {
    const args = parseArgs(process.argv.slice(2));

    const results = await runSeedSimulations(args);

    const thresholds = buildEffectiveThresholds(args);
    const outputData = buildOutput(args, thresholds, results);
    const output = JSON.stringify(outputData, null, 2);
    process.stdout.write(`${output}\n`);

    if (args.out) {
        await writeFile(resolve(args.out), `${output}\n`);
    }

    if (args.check && args.baseline) {
        const baseline = await loadBaseline(args.baseline);
        const comparison = evaluateAgainstBaseline(
            outputData,
            baseline as { meta: BaselineMeta; results?: Array<{ seed: string; metrics: Record<string, number> }> },
            thresholds,
        );

        for (const warning of comparison.warnings) {
            console.error(`[seed-regression] warn: ${warning}`);
        }

        if (comparison.deviations.length > 0) {
            for (const deviation of comparison.deviations) {
                if (deviation.reason) {
                    console.error(
                        `[seed-regression] deviation seed=${deviation.seed} metric=${deviation.metric} reason=${deviation.reason}${"expected" in deviation ? ` expected=${deviation.expected}` : ""}${"actual" in deviation ? ` actual=${deviation.actual}` : ""}`,
                    );
                    continue;
                }
                console.error(
                    `[seed-regression] deviation seed=${deviation.seed} metric=${deviation.metric} mode=${deviation.mode} current=${deviation.currentValue} baseline=${deviation.baselineValue} diff=${deviation.diff} threshold=${deviation.threshold}`,
                );
            }
        }
        console.error(`[seed-regression] deviations=${comparison.deviations.length}`);
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
