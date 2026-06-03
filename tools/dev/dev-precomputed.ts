import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import process from "node:process";

const rootDir = process.cwd();

let shutdownRequested = false;
const children = new Set<ChildProcess>();

function mergedEnv(extra: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
    return {
        ...process.env,
        ...extra,
    };
}

function startProcess(
    command: string,
    args: string[],
    env: NodeJS.ProcessEnv = {},
): ChildProcess {
    const child = spawn(command, args, {
        cwd: rootDir,
        env: mergedEnv(env),
        stdio: "inherit",
        shell: process.platform === "win32",
    });
    children.add(child);
    child.on("exit", (code, signal) => {
        children.delete(child);
        if (!shutdownRequested) {
            shutdownRequested = true;
            stopChildren();
            if (signal) {
                process.kill(process.pid, signal);
                return;
            }
            process.exit(code ?? 1);
        }
    });
    return child;
}

function stopChildren() {
    for (const child of children) {
        if (!child.killed) {
            child.kill("SIGTERM");
        }
    }
}

function shutdown() {
    if (shutdownRequested) {
        return;
    }
    shutdownRequested = true;
    stopChildren();
}

function main() {
    const bind = process.env.FREY_PRECOMPUTE_BIND ?? "127.0.0.1:8787";
    const proxyTarget = `http://${bind}`;
    const storeDir =
        process.env.FREY_PRECOMPUTE_STORE_DIR ?? "data/precomputed/worlds";

    startProcess("corepack", ["pnpm", "server:precompute"], {
        FREY_PRECOMPUTE_BIND: bind,
        FREY_PRECOMPUTE_STORE_DIR: storeDir,
    });
    startProcess(
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
            FREY_PRECOMPUTE_PROXY_TARGET: proxyTarget,
            VITE_FREY_ENGINE: "http",
            VITE_FREY_API_BASE: "",
        },
    );
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

main();
