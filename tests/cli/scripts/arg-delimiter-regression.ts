import { spawnSync } from "node:child_process";

function run(commandArgs) {
    return spawnSync("pnpm", commandArgs, {
        cwd: process.cwd(),
        encoding: "utf8",
    });
}

function assertPassWithDelimiter(label, commandArgs) {
    const result = run(commandArgs);
    if (result.status !== 0) {
        const stderr = (result.stderr ?? "").trim();
        const stdout = (result.stdout ?? "").trim();
        throw new Error(
            `${label} failed with status=${String(result.status)}\nstdout:\n${stdout}\nstderr:\n${stderr}`,
        );
    }

    const stderr = result.stderr ?? "";
    const stdout = result.stdout ?? "";
    const combined = `${stdout}\n${stderr}`;
    if (combined.includes("Unknown argument: --")) {
        throw new Error(`${label} unexpectedly rejected delimiter '--'`);
    }
}

function main() {
    assertPassWithDelimiter("perf script delimiter", [
        "exec",
        "tsx",
        "tests/perf/scripts/perf.ts",
        "--",
        "--help",
    ]);
    assertPassWithDelimiter("seed-regression script delimiter", [
        "exec",
        "tsx",
        "tests/seed-regression/scripts/seed-regression.ts",
        "--",
        "--help",
    ]);
    process.stdout.write("CLI delimiter regression checks passed.\n");
}

main();
