import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { watch } from "node:fs";
import path from "node:path";
import process from "node:process";

const rootDir = process.cwd();
const rustDir = path.join(rootDir, "rust");
const configDir = path.join(rootDir, "config");

let shutdownRequested = false;
let serverProcess: ChildProcess | null = null;
let viteProcess: ChildProcess | null = null;
let restartRunning = false;
let restartQueued = false;
let syncConfigRunning = false;
let syncConfigQueued = false;
let rustDebounceTimer: NodeJS.Timeout | null = null;
let configDebounceTimer: NodeJS.Timeout | null = null;

function mergedEnv(extra: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
    return {
        ...process.env,
        ...extra,
    };
}

function runCommand(
    command: string,
    args: string[],
    env: NodeJS.ProcessEnv = {},
): Promise<{ code: number | null; signal: string | null }> {
    return new Promise((resolve) => {
        const child = spawn(command, args, {
            cwd: rootDir,
            env: mergedEnv(env),
            stdio: "inherit",
            shell: process.platform === "win32",
        });
        child.on("exit", (code, signal) => {
            resolve({ code, signal: signal as string | null });
        });
    });
}

function stopChild(child: ChildProcess | null) {
    if (!child || child.killed) {
        return;
    }
    child.kill("SIGTERM");
}

function waitForExit(child: ChildProcess | null): Promise<void> {
    if (!child) {
        return Promise.resolve();
    }
    return new Promise((resolve) => {
        child.once("exit", () => resolve());
    });
}

function startPrecomputeServer(bind: string, storeDir: string) {
    const child = spawn("corepack", ["pnpm", "server:precompute"], {
        cwd: rootDir,
        env: mergedEnv({
            FREY_PRECOMPUTE_BIND: bind,
            FREY_PRECOMPUTE_STORE_DIR: storeDir,
        }),
        stdio: "inherit",
        shell: process.platform === "win32",
    });
    child.on("exit", (code, signal) => {
        if (serverProcess === child) {
            serverProcess = null;
        }
        if (shutdownRequested || restartRunning) {
            return;
        }
        if (signal) {
            process.kill(process.pid, signal);
            return;
        }
        process.exit(code ?? 1);
    });
    serverProcess = child;
}

function startVite(proxyTarget: string) {
    const initialSeed = process.env.FREY_PRECOMPUTE_SEED?.trim() || "alpha";
    viteProcess = spawn(
        "corepack",
        [
            "pnpm",
            "exec",
            "vite",
            "--config",
            "web/vite.config.ts",
            "--host",
            "127.0.0.1",
            "--port",
            "5173",
        ],
        {
            cwd: rootDir,
            env: mergedEnv({
                FREY_PRECOMPUTE_PROXY_TARGET: proxyTarget,
                VITE_FREY_ENGINE: "http",
                VITE_FREY_API_BASE: "",
                VITE_FREY_INITIAL_SEED: initialSeed,
            }),
            stdio: "inherit",
            shell: process.platform === "win32",
        },
    );
    viteProcess.on("exit", (code, signal) => {
        if (shutdownRequested) {
            return;
        }
        if (signal) {
            process.kill(process.pid, signal);
            return;
        }
        process.exit(code ?? 1);
    });
}

async function restartPrecomputeServer(bind: string, storeDir: string) {
    if (restartRunning) {
        restartQueued = true;
        return;
    }
    restartRunning = true;
    console.log("[precomputed] restarting precompute server...");
    const previous = serverProcess;
    stopChild(previous);
    await waitForExit(previous);
    if (!shutdownRequested) {
        startPrecomputeServer(bind, storeDir);
    }
    restartRunning = false;
    if (restartQueued && !shutdownRequested) {
        restartQueued = false;
        await restartPrecomputeServer(bind, storeDir);
    }
}

async function syncConfig() {
    if (syncConfigRunning) {
        syncConfigQueued = true;
        return;
    }
    syncConfigRunning = true;
    console.log("[precomputed] syncing config...");
    const result = await runCommand("corepack", ["pnpm", "config:sync"]);
    if (result.code !== 0) {
        console.error(
            `[precomputed] config sync failed (code: ${result.code ?? "null"})`,
        );
    } else {
        console.log("[precomputed] config sync complete");
    }
    syncConfigRunning = false;
    if (syncConfigQueued && !shutdownRequested) {
        syncConfigQueued = false;
        await syncConfig();
    }
}

function scheduleRustRestart(bind: string, storeDir: string, filename: string) {
    if (!filename.endsWith(".rs") && filename !== "Cargo.toml") {
        return;
    }
    if (rustDebounceTimer) {
        clearTimeout(rustDebounceTimer);
    }
    rustDebounceTimer = setTimeout(() => {
        rustDebounceTimer = null;
        if (!shutdownRequested) {
            void restartPrecomputeServer(bind, storeDir);
        }
    }, 150);
}

function scheduleConfigSync(filename: string) {
    if (!filename.endsWith(".yaml")) {
        return;
    }
    if (configDebounceTimer) {
        clearTimeout(configDebounceTimer);
    }
    configDebounceTimer = setTimeout(() => {
        configDebounceTimer = null;
        if (!shutdownRequested) {
            void syncConfig();
        }
    }, 150);
}

function startRustWatcher(bind: string, storeDir: string) {
    const watcher = watch(rustDir, { recursive: true }, (_eventType, filename) => {
        if (typeof filename === "string") {
            scheduleRustRestart(bind, storeDir, filename);
        }
    });
    watcher.on("error", (error: Error) => {
        console.error("[precomputed] rust watcher error:", error);
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
        console.error("[precomputed] config watcher error:", error);
    });
    return watcher;
}

async function main() {
    const bind = process.env.FREY_PRECOMPUTE_BIND ?? "127.0.0.1:8787";
    const proxyTarget = `http://${bind}`;
    const storeDir =
        process.env.FREY_PRECOMPUTE_STORE_DIR ?? "data/precomputed/worlds";

    const initialSync = await runCommand("corepack", ["pnpm", "config:sync"]);
    if (initialSync.code !== 0) {
        process.exit(initialSync.code ?? 1);
    }

    startPrecomputeServer(bind, storeDir);
    startVite(proxyTarget);

    const rustWatcher = startRustWatcher(bind, storeDir);
    const configWatcher = startConfigWatcher();

    const shutdown = () => {
        if (shutdownRequested) {
            return;
        }
        shutdownRequested = true;
        if (rustDebounceTimer) {
            clearTimeout(rustDebounceTimer);
            rustDebounceTimer = null;
        }
        if (configDebounceTimer) {
            clearTimeout(configDebounceTimer);
            configDebounceTimer = null;
        }
        rustWatcher.close();
        configWatcher.close();
        stopChild(serverProcess);
        stopChild(viteProcess);
    };

    process.on("SIGINT", shutdown);
    process.on("SIGTERM", shutdown);
}

void main();
