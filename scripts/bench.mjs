import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

import initWasm, {
    WorldSimController,
    build_render_positions,
    generate_mesh,
} from "../src/wasm/frey_wasm.js";
import { createBenchmarkProfile } from "../src/app/perf-benchmark.js";
import { createPerfBenchmarkRunner } from "../src/app/perf-benchmark-runner.js";
import { TERRAIN_LEVEL, TERRAIN_PARAMS } from "../src/interface/params/terrain.js";

const DEFAULT_THRESHOLD = 0.10;

function parseNumber(value, flagName) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
        throw new Error(`${flagName} must be a finite number`);
    }
    return parsed;
}

function parseArgs(argv) {
    const args = {
        ticks: 32,
        seed: "alpha",
        surfaceMode: "globe",
        viewMode: "normal",
        sampleInterval: 4,
        level: TERRAIN_LEVEL,
        out: null,
        baseline: null,
        threshold: DEFAULT_THRESHOLD,
        thresholdTickTotal: null,
        thresholdStepWorld: null,
        thresholdStepGeologyRiver: null,
        progress: false,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
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
        case "--threshold-step-geology-river":
            args.thresholdStepGeologyRiver = Math.max(0, parseNumber(next, "--threshold-step-geology-river"));
            i += 1;
            break;
        case "--progress":
            args.progress = true;
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
    console.error("Usage: node scripts/bench.mjs [options]");
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
    console.error("  --threshold-step-geology-river <ratio>");
    console.error("  --progress");
}

function getPathValue(obj, path) {
    const keys = path.split(".");
    let current = obj;
    for (const key of keys) {
        if (current == null || !(key in current)) {
            return undefined;
        }
        current = current[key];
    }
    return current;
}

function formatRatio(value) {
    return `${(value * 100).toFixed(2)}%`;
}

async function loadBaseline(pathname) {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content);
}

function evaluateRegression(current, baseline, args) {
    const specs = [
        {
            label: "tick_total.mean",
            path: "metrics.tick_total.mean",
            threshold: args.thresholdTickTotal ?? args.threshold,
        },
        {
            label: "step_world.mean",
            path: "metrics.step_world.mean",
            threshold: args.thresholdStepWorld ?? args.threshold,
        },
        {
            label: "step_geology_river.mean",
            path: "metrics.step_geology_river.mean",
            threshold: args.thresholdStepGeologyRiver ?? args.threshold,
        },
    ];

    const warnings = [];
    const failures = [];

    for (const spec of specs) {
        const currentValue = Number(getPathValue(current, spec.path));
        const baselineValue = Number(getPathValue(baseline, spec.path));

        if (!Number.isFinite(currentValue) || !Number.isFinite(baselineValue)) {
            warnings.push(`skip ${spec.label}: missing numeric value`);
            continue;
        }
        if (baselineValue <= 0) {
            warnings.push(`skip ${spec.label}: baseline <= 0`);
            continue;
        }

        const allowedMax = baselineValue * (1 + spec.threshold);
        if (currentValue > allowedMax) {
            failures.push({
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
        failures,
    };
}

async function initWasmForNode() {
    const wasmPath = new URL("../src/wasm/frey_wasm_bg.wasm", import.meta.url);
    const wasmBytes = await readFile(wasmPath);
    try {
        await initWasm({ module_or_path: wasmBytes });
    } catch {
        await initWasm(wasmBytes);
    }
}

async function main() {
    const args = parseArgs(process.argv.slice(2));

    await initWasmForNode();

    const runner = createPerfBenchmarkRunner({
        WorldSimController,
        build_render_positions,
        generate_mesh,
    });

    const profile = createBenchmarkProfile({
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
        meta: {
            user_agent: `node ${process.version}`,
            timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        },
        onProgress(payload) {
            if (args.progress) {
                console.error(payload.status);
            }
        },
        onWarning(message) {
            console.error(`[bench warning] ${message}`);
        },
    });

    const output = JSON.stringify(result, null, 2);
    process.stdout.write(`${output}\n`);

    if (args.out) {
        await writeFile(resolve(args.out), output);
    }

    if (args.baseline) {
        const baseline = await loadBaseline(args.baseline);
        const gate = evaluateRegression(result, baseline, args);

        for (const warning of gate.warnings) {
            console.error(`[bench gate] ${warning}`);
        }

        if (gate.failures.length > 0) {
            for (const failure of gate.failures) {
                console.error(
                    `[bench gate] FAIL ${failure.label}: current=${failure.currentValue}, baseline=${failure.baselineValue}, threshold=${formatRatio(failure.threshold)}, allowed_max=${failure.allowedMax}`,
                );
            }
            process.exitCode = 1;
        } else {
            console.error("[bench gate] PASS");
        }
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
