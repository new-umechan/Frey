import { spawn } from "node:child_process";

type Stage = "contract" | "regression" | "reference" | "perf";

const ORDERED_STAGES: Stage[] = ["contract", "regression", "reference", "perf"];

function parseArgs(argv: string[]) {
    const args = {
        stage: "all",
        list: false,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--stage":
            args.stage = String(next ?? "all");
            i += 1;
            break;
        case "--list":
            args.list = true;
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
    console.error("Usage: tsx benches/scripts/bench-outer.ts [options]");
    console.error("  --stage <all|contract|regression|reference|perf>");
    console.error("  --list");
}

function resolveStages(input: string): Stage[] {
    if (input === "all") {
        return [...ORDERED_STAGES];
    }
    if (!ORDERED_STAGES.includes(input as Stage)) {
        throw new Error(`Unknown stage: ${input}`);
    }
    return [input as Stage];
}

function run(command: string, args: string[], stage: Stage, name: string) {
    return new Promise<void>((resolve, reject) => {
        console.error(`[bench:${stage}] ${name}`);
        const child = spawn(command, args, {
            stdio: "inherit",
        });
        child.on("error", reject);
        child.on("exit", (code, signal) => {
            if (signal) {
                reject(new Error(`${name} terminated by signal: ${signal}`));
                return;
            }
            if (code !== 0) {
                reject(new Error(`${name} failed with exit code ${code}`));
                return;
            }
            resolve();
        });
    });
}

async function runStage(stage: Stage) {
    if (stage === "contract") {
        await run("pnpm", ["run", "test:cli-args"], stage, "cli-contract");
        return;
    }
    if (stage === "regression") {
        await run("pnpm", ["run", "seed:gate:quick"], stage, "seed-regression-quick");
        return;
    }
    if (stage === "reference") {
        await run("pnpm", ["run", "bench:check:climate-runtime"], stage, "climate-reference-runtime");
        await run("pnpm", ["run", "bench:check:hydrology-runtime"], stage, "hydrology-reference-runtime");
        return;
    }
    await run("pnpm", ["run", "bench"], stage, "module-benchmarks");
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    if (args.list) {
        for (const stage of ORDERED_STAGES) {
            console.log(stage);
        }
        return;
    }
    const stages = resolveStages(args.stage);
    for (const stage of stages) {
        await runStage(stage);
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
