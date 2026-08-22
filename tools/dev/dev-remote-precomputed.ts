import { spawn } from "node:child_process";
import process from "node:process";

function remoteApiBase(): string {
    const value = process.env.FREY_REMOTE_API_BASE?.trim();
    if (!value) {
        throw new Error(
            "FREY_REMOTE_API_BASE is required, for example: https://frey-api.example.com",
        );
    }
    const url = new URL(value);
    if (url.protocol !== "https:") {
        throw new Error("FREY_REMOTE_API_BASE must use HTTPS");
    }
    return url.toString().replace(/\/+$/, "");
}

async function main() {
    const apiBase = remoteApiBase();
    const initialSeed = process.env.FREY_REMOTE_SEED?.trim() || "alpha";
    console.log(`[remote-precomputed] proxying /api to ${apiBase}`);
    console.log(`[remote-precomputed] initial seed: ${initialSeed}`);

    const vite = spawn(
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
            env: {
                ...process.env,
                FREY_PRECOMPUTE_PROXY_TARGET: apiBase,
                VITE_FREY_ENGINE: "http",
                VITE_FREY_API_BASE: "",
                VITE_FREY_INITIAL_SEED: initialSeed,
            },
            stdio: "inherit",
            shell: process.platform === "win32",
        },
    );

    vite.on("exit", (code, signal) => {
        if (signal) {
            process.kill(process.pid, signal);
        }
        process.exit(code ?? 1);
    });
}

try {
    await main();
} catch (error) {
    console.error(`[remote-precomputed] ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
}
