import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

import initWasm, { WorldSimController } from "../../generated/wasm/web/frey_wasm.js";
import { GEOLOGY_LEVEL, GEOLOGY_PARAMS } from "../../web/src/interface/params/geology.js";

const DEFAULT_TICKS = 32;
const DEFAULT_THRESHOLD = 0.005;
const DEFAULT_SEEDS = ["alpha"];
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

function parseNumber(value, flagName) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
        throw new Error(`${flagName} must be a finite number`);
    }
    return parsed;
}

function parseSeedsCsv(raw) {
    if (typeof raw !== "string") {
        return [];
    }
    return raw
        .split(",")
        .map((seed) => seed.trim())
        .filter((seed) => seed.length > 0);
}

function parseArgs(argv) {
    const args = {
        ticks: DEFAULT_TICKS,
        seeds: [...DEFAULT_SEEDS],
        level: GEOLOGY_LEVEL,
        out: null,
        baseline: null,
        check: false,
        threshold: DEFAULT_THRESHOLD,
        thresholdByMetric: {},
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];

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
        case "--level":
            args.level = Math.max(0, Math.floor(parseNumber(next, "--level")));
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
    console.error("Usage: node tools/bench/seed-regression.mjs [options]");
    console.error("  --seeds <csv>");
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
    const wasmPath = new URL("../../generated/wasm/web/frey_wasm_bg.wasm", import.meta.url);
    const wasmBytes = await readFile(wasmPath);
    try {
        await initWasm({ module_or_path: wasmBytes });
    } catch {
        await initWasm(wasmBytes);
    }
}

async function loadBaseline(pathname) {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content);
}

function collectMetricsFromResponse(metrics) {
    const result = {};
    for (const spec of METRIC_SPECS) {
        const value = Number(metrics?.[spec.sourceKey]);
        if (!Number.isFinite(value)) {
            throw new Error(`missing numeric metric from wasm response: ${spec.sourceKey}`);
        }
        result[spec.key] = value;
    }
    return result;
}

function buildEffectiveThresholds(args) {
    const thresholds = {};
    for (const spec of METRIC_SPECS) {
        thresholds[spec.key] = args.thresholdByMetric[spec.key] ?? args.threshold;
    }
    return thresholds;
}

function relativeOrAbsoluteDiff(currentValue, baselineValue) {
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

function normalizeSeedSet(seeds) {
    if (!Array.isArray(seeds)) {
        return [];
    }
    return [...new Set(seeds.map((seed) => String(seed)))].sort();
}

function validateBaselineMeta(current, baseline) {
    const failures = [];
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

    return failures;
}

function evaluateAgainstBaseline(current, baseline, thresholds) {
    const currentBySeed = new Map(current.results.map((entry) => [entry.seed, entry.metrics]));
    const baselineBySeed = new Map(
        (baseline?.results ?? []).map((entry) => [entry.seed, entry.metrics]),
    );

    const warnings = [];
    const failures = [];

    failures.push(...validateBaselineMeta(current, baseline));
    if (failures.length > 0) {
        return { warnings, failures };
    }

    for (const seed of current.meta.seeds) {
        if (!baselineBySeed.has(seed)) {
            failures.push({
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
                failures.push({
                    seed,
                    metric: spec.key,
                    reason: "missing_numeric_metric",
                });
                continue;
            }

            const threshold = Number(thresholds[spec.key]);
            const { mode, diff } = relativeOrAbsoluteDiff(currentValue, baselineValue);
            if (diff > threshold) {
                failures.push({
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

    return { warnings, failures };
}

async function runSeedSimulation(seed, ticks, level) {
    const controller = new WorldSimController();
    const init = controller.init_world(seed, level, {
        geology_params: GEOLOGY_PARAMS,
    });
    const worldId = init?.world_id;

    if (!worldId) {
        throw new Error(`init_world failed: missing world_id for seed=${seed}`);
    }

    if (ticks > 0) {
        controller.exec_world(worldId, ticks);
    }

    const metricsResponse = controller.get_metrics(worldId);
    return collectMetricsFromResponse(metricsResponse);
}

function buildOutput(args, thresholds, results) {
    return {
        meta: {
            generated_at: new Date().toISOString(),
            ticks: args.ticks,
            level: args.level,
            seeds: args.seeds,
            thresholds,
        },
        results,
    };
}

async function main() {
    const args = parseArgs(process.argv.slice(2));

    await initWasmForNode();

    const results = [];
    for (const seed of args.seeds) {
        const metrics = await runSeedSimulation(seed, args.ticks, args.level);
        results.push({
            seed,
            tick: args.ticks,
            metrics,
        });
    }

    const thresholds = buildEffectiveThresholds(args);
    const outputData = buildOutput(args, thresholds, results);
    const output = JSON.stringify(outputData, null, 2);
    process.stdout.write(`${output}\n`);

    if (args.out) {
        await writeFile(resolve(args.out), `${output}\n`);
    }

    if (args.check) {
        const baseline = await loadBaseline(args.baseline);
        const gate = evaluateAgainstBaseline(outputData, baseline, thresholds);

        for (const warning of gate.warnings) {
            console.error(`[seed-regression] warn: ${warning}`);
        }

        if (gate.failures.length > 0) {
            for (const failure of gate.failures) {
                if (failure.reason) {
                    console.error(
                        `[seed-regression] FAIL seed=${failure.seed} metric=${failure.metric} reason=${failure.reason}${"expected" in failure ? ` expected=${failure.expected}` : ""}${"actual" in failure ? ` actual=${failure.actual}` : ""}`,
                    );
                    continue;
                }
                console.error(
                    `[seed-regression] FAIL seed=${failure.seed} metric=${failure.metric} mode=${failure.mode} current=${failure.currentValue} baseline=${failure.baselineValue} diff=${failure.diff} threshold=${failure.threshold}`,
                );
            }
            process.exitCode = 1;
        } else {
            console.error("[seed-regression] PASS");
        }
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
