import { spawn } from "node:child_process";
import { watch } from "node:fs";
import path from "node:path";
import process from "node:process";

const rootDir = process.cwd();
const rustDir = path.join(rootDir, "rust");
const configDir = path.join(rootDir, "config");

let buildRunning = false;
let buildQueued = false;
let syncRunning = false;
let syncQueued = false;
let viteProcess = null;
let shutdownRequested = false;
let debounceTimer = null;
let syncDebounceTimer = null;

function runCommand(command, args, options = {}) {
    return new Promise((resolve) => {
        const child = spawn(command, args, {
            cwd: rootDir,
            stdio: "inherit",
            shell: process.platform === "win32",
            ...options,
        });

        child.on("exit", (code, signal) => {
            resolve({ code, signal });
        });
    });
}

async function buildWasm() {
    if (buildRunning) {
        buildQueued = true;
        return;
    }

    buildRunning = true;
    console.log("[dev] rebuilding wasm...");
    const result = await runCommand("npm", ["run", "wasm:build:dev:no-sync"]);

    if (result.code !== 0) {
        console.error(`[dev] wasm build failed (code: ${result.code ?? "null"})`);
    } else {
        console.log("[dev] wasm rebuild complete");
    }

    buildRunning = false;

    if (buildQueued && !shutdownRequested) {
        buildQueued = false;
        await buildWasm();
    }
}

async function syncTerrainParams() {
    if (syncRunning) {
        syncQueued = true;
        return;
    }

    syncRunning = true;
    console.log("[dev] syncing terrain params...");
    const result = await runCommand("npm", ["run", "terrain-params:sync"]);

    if (result.code !== 0) {
        console.error(`[dev] terrain params sync failed (code: ${result.code ?? "null"})`);
    } else {
        console.log("[dev] terrain params sync complete");
    }

    syncRunning = false;

    if (syncQueued && !shutdownRequested) {
        syncQueued = false;
        await syncTerrainParams();
    }
}

function startVite() {
    viteProcess = spawn("npx", ["vite"], {
        cwd: rootDir,
        stdio: "inherit",
        shell: process.platform === "win32",
    });

    viteProcess.on("exit", (code) => {
        if (!shutdownRequested) {
            process.exit(code ?? 1);
        }
    });
}

function shouldTriggerBuild(filename = "") {
    if (!filename.endsWith(".rs") && filename !== "Cargo.toml") {
        return false;
    }

    return true;
}

function scheduleBuild(filename) {
    if (!shouldTriggerBuild(filename)) {
        return;
    }

    if (debounceTimer) {
        clearTimeout(debounceTimer);
    }

    debounceTimer = setTimeout(() => {
        debounceTimer = null;
        if (!shutdownRequested) {
            void buildWasm();
        }
    }, 150);
}

function scheduleTerrainParamsSync(filename) {
    if (filename !== "terrain-params.yaml") {
        return;
    }

    if (syncDebounceTimer) {
        clearTimeout(syncDebounceTimer);
    }

    syncDebounceTimer = setTimeout(() => {
        syncDebounceTimer = null;
        if (!shutdownRequested) {
            void syncTerrainParams();
        }
    }, 150);
}

function startRustWatcher() {
    const watcher = watch(rustDir, { recursive: true }, (_eventType, filename) => {
        if (typeof filename === "string") {
            scheduleBuild(filename);
        }
    });

    watcher.on("error", (error) => {
        console.error("[dev] rust watcher error:", error);
    });

    return watcher;
}

function startConfigWatcher() {
    const watcher = watch(configDir, { recursive: true }, (_eventType, filename) => {
        if (typeof filename === "string") {
            scheduleTerrainParamsSync(filename);
        }
    });

    watcher.on("error", (error) => {
        console.error("[dev] config watcher error:", error);
    });

    return watcher;
}

function stopChild(child) {
    if (!child || child.killed) {
        return;
    }

    child.kill("SIGTERM");
}

async function main() {
    const syncInitial = await runCommand("npm", ["run", "terrain-params:sync"]);
    if (syncInitial.code !== 0) {
        process.exit(syncInitial.code ?? 1);
    }

    const initial = await runCommand("npm", ["run", "wasm:build:dev:no-sync"]);
    if (initial.code !== 0) {
        process.exit(initial.code ?? 1);
    }

    const rustWatcher = startRustWatcher();
    const configWatcher = startConfigWatcher();
    startVite();

    const shutdown = () => {
        if (shutdownRequested) {
            return;
        }

        shutdownRequested = true;
        if (debounceTimer) {
            clearTimeout(debounceTimer);
            debounceTimer = null;
        }
        if (syncDebounceTimer) {
            clearTimeout(syncDebounceTimer);
            syncDebounceTimer = null;
        }
        rustWatcher.close();
        configWatcher.close();
        stopChild(viteProcess);
    };

    process.on("SIGINT", shutdown);
    process.on("SIGTERM", shutdown);
}

void main();
