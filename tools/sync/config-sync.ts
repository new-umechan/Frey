import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, "..", "..");

const SYNC_STEPS = [
    "tsx tools/sync/sync-runtime-params.ts",
    "tsx tools/sync/sync-terrain-params.ts",
    "tsx tools/sync/sync-climate-params.ts",
    "tsx tools/sync/sync-glaciology-params.ts",
    "tsx tools/sync/generate-config-types.ts",
    "tsx tools/sync/generate-config-docs.ts",
];

function runStep(command: string) {
    const result = spawnSync(command, {
        cwd: ROOT_DIR,
        stdio: "inherit",
        shell: true,
    });
    if (result.status !== 0) {
        throw new Error(`config sync failed: ${command}`);
    }
}

function main() {
    for (const step of SYNC_STEPS) {
        runStep(step);
    }
    console.log("config:sync completed");
}

main();
