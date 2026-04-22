import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

import initWasm, { WorldSimController } from "../../generated/wasm/web/frey_wasm";
import { TERRAIN_LEVEL, TERRAIN_PARAMS } from "../../web/src/interface/params/terrain";

interface Args {
    seed: string;
    ticks: number;
    level: number;
    out: string | null;
}

function defaultOutputPath(args: Args): string {
    const safeSeed = args.seed.replace(/[^a-zA-Z0-9_-]/g, "_");
    return resolve(
        "benches/results/scientific_benchmark_samples",
        `${safeSeed}_L${args.level}_T${args.ticks}.json`,
    );
}

function parseNumber(value: unknown, flagName: string): number {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
        throw new Error(`${flagName} must be a finite number`);
    }
    return parsed;
}

function parseArgs(argv: string[]): Args {
    const args: Args = {
        seed: "alpha",
        ticks: 32,
        level: TERRAIN_LEVEL,
        out: null,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--":
            break;
        case "--seed":
            args.seed = String(next ?? args.seed);
            i += 1;
            break;
        case "--ticks":
            args.ticks = Math.max(1, Math.floor(parseNumber(next, "--ticks")));
            i += 1;
            break;
        case "--level":
            args.level = Math.max(0, Math.floor(parseNumber(next, "--level")));
            i += 1;
            break;
        case "--out":
            args.out = String(next ?? "");
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
    console.error("Usage: tsx benches/scripts/export-scientific-benchmark-samples.ts [options]");
    console.error("  --seed <seed>");
    console.error("  --ticks <n>");
    console.error("  --level <n>");
    console.error("  --out <path>");
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

async function main() {
    const args = parseArgs(process.argv.slice(2));
    await initWasmForNode();

    const controller = new WorldSimController();
    const init = controller.init_world(args.seed, args.level, {
        geology_params: TERRAIN_PARAMS,
        verification_mode: "scientific_benchmark",
    });
    const worldId = String((init as Record<string, unknown>)?.world_id ?? "");
    if (worldId.length === 0) {
        throw new Error("init_world returned empty world_id");
    }

    controller.exec_world(worldId, args.ticks);
    const samples = controller.get_scientific_benchmark_samples(worldId);
    const output = JSON.stringify(samples, null, 2);
    process.stdout.write(`${output}\n`);

    const outputPath = args.out ? resolve(args.out) : defaultOutputPath(args);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${output}\n`, "utf8");
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
