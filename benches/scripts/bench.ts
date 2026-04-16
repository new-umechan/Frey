import { spawn } from "node:child_process";

const SUITES = [
    "climate_solo",
    "hydrology_solo",
    "ecology_solo",
    "domesticates_solo",
    "glaciology_solo",
    "glaciology_sea_level_series",
];

function parseArgs(argv: string[]) {
    const args = {
        suite: "all",
        list: false,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--suite":
            args.suite = String(next ?? "all");
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
    console.error("Usage: node benches/scripts/bench.mjs [options]");
    console.error("  --suite <all|climate_solo|hydrology_solo|ecology_solo|domesticates_solo|glaciology_solo|glaciology_sea_level_series>");
    console.error("  --list");
}

function printSuites() {
    for (const suite of SUITES) {
        console.log(suite);
    }
}

function resolveSuites(args: { suite: string; list: boolean }) {
    if (args.suite === "all") {
        return [...SUITES];
    }
    if (!SUITES.includes(args.suite)) {
        throw new Error(`Unknown suite: ${args.suite}`);
    }
    return [args.suite];
}

function runCargoBench(suite: string) {
    return new Promise((resolve, reject) => {
        const child = spawn(
            "cargo",
            ["bench", "--manifest-path", "benches/rust/Cargo.toml", "--bench", suite],
            {
                stdio: "inherit",
            },
        );

        child.on("error", reject);
        child.on("exit", (code: number | null, signal: string | null) => {
            if (signal) {
                reject(new Error(`${suite} terminated by signal: ${signal}`));
                return;
            }
            if (code !== 0) {
                reject(new Error(`${suite} failed with exit code ${code}`));
                return;
            }
            resolve(undefined);
        });
    });
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    if (args.list) {
        printSuites();
        return;
    }

    const suites = resolveSuites(args);
    for (const suite of suites) {
        console.error(`[bench] running ${suite}`);
        await runCargoBench(suite);
    }
}

main().catch((error) => {
    console.error(error.message ?? String(error));
    process.exit(1);
});
