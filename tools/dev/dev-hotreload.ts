import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { watch } from "node:fs";
import path from "node:path";
import process from "node:process";

const rootDir = process.cwd();
const rustDir = path.join(rootDir, "rust");
const configDir = path.join(rootDir, "config");

let buildRunning = false;
let buildQueued = false;
let syncConfigRunning = false;
let syncConfigQueued = false;
let viteProcess: ChildProcess | null = null;
let shutdownRequested = false;
let debounceTimer: NodeJS.Timeout | null = null;
let syncConfigDebounceTimer: NodeJS.Timeout | null = null;

function runCommand(command: string, args: string[], options: Record<string, unknown> = {}): Promise<{ code: number | null; signal: string | null }> {
    return new Promise((resolve) => {
        const child = spawn(command, args, {
            cwd: rootDir,
            stdio: "inherit",
            shell: process.platform === "win32",
            ...options,
        });

        child.on("exit", (code, signal) => {
            resolve({ code, signal: signal as string | null });
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
    const result = await runCommand("pnpm", ["run", "wasm:build:dev:no-sync"]);

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

async function syncConfig() {
    if (syncConfigRunning) {
        syncConfigQueued = true;
        return;
    }

    syncConfigRunning = true;
    console.log("[dev] syncing config...");
    const result = await runCommand("pnpm", ["run", "config:sync"]);

    if (result.code !== 0) {
        console.error(`[dev] config sync failed (code: ${result.code ?? "null"})`);
    } else {
        console.log("[dev] config sync complete");
    }

    syncConfigRunning = false;

    if (syncConfigQueued && !shutdownRequested) {
        syncConfigQueued = false;
        await syncConfig();
    }
}

function startVite() {
    viteProcess = spawn("pnpm", ["exec", "vite", "--config", "web/vite.config.ts"], {
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

function scheduleBuild(filename: string) {
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

function scheduleConfigSync(filename: string) {
    if (!filename.endsWith(".yaml")) {
        return;
    }

    if (syncConfigDebounceTimer) {
        clearTimeout(syncConfigDebounceTimer);
    }

    syncConfigDebounceTimer = setTimeout(() => {
        syncConfigDebounceTimer = null;
        if (!shutdownRequested) {
            void syncConfig();
        }
    }, 150);
}

function startRustWatcher() {
    const watcher = watch(rustDir, { recursive: true }, (_eventType, filename) => {
        if (typeof filename === "string") {
            scheduleBuild(filename);
        }
    });

    watcher.on("error", (error: Error) => {
        console.error("[dev] rust watcher error:", error);
    });

    return watcher;
}

function startConfigWatcher() {
    const watcher = watch(configDir, { recursive: true }, (_eventType, filename) => {
        if (typeof filename === "string") {
            scheduleConfigSync(filename);
        }
    });

    watcher.on("error", (error: Error) => {
        console.error("[dev] config watcher error:", error);
    });

    return watcher;
}

function stopChild(child: ChildProcess | null) {
    if (!child || child.killed) {
        return;
    }

    child.kill("SIGTERM");
}

async function main() {
    const syncInitial = await runCommand("pnpm", ["run", "config:sync"]);
    if (syncInitial.code !== 0) {
        process.exit(syncInitial.code ?? 1);
    }

    const initial = await runCommand("pnpm", ["run", "wasm:build:dev:no-sync"]);
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
        if (syncConfigDebounceTimer) {
            clearTimeout(syncConfigDebounceTimer);
            syncConfigDebounceTimer = null;
        }
        rustWatcher.close();
        configWatcher.close();
        stopChild(viteProcess);
    };

    process.on("SIGINT", shutdown);
    process.on("SIGTERM", shutdown);
}

void main();
