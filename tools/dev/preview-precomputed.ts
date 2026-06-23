import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

type CommandResult = {
    code: number | null;
    signal: string | null;
};

type PrecomputedManifest = {
    format_version?: number;
    seed?: string;
    mesh_level?: number;
    max_tick?: number;
    keyframe_interval?: number;
};

const rootDir = process.cwd();
const rustPrecomputeServerPath = path.join(
    rootDir,
    "rust/src/precompute_server.rs",
);

function parseSeedList(): string[] {
    const csv =
        process.env.FREY_PRECOMPUTE_SEEDS ??
        process.env.FREY_PRECOMPUTE_SEED ??
        "alpha";
    return csv
        .split(",")
        .map((value) => value.trim())
        .filter((value) => value.length > 0);
}

function envNumber(name: string, fallback: number): number {
    const value = process.env[name];
    if (!value) {
        return fallback;
    }
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) ? parsed : fallback;
}

function runCommand(
    command: string,
    args: string[],
    env: NodeJS.ProcessEnv = {},
): Promise<CommandResult> {
    return new Promise((resolve) => {
        const child = spawn(command, args, {
            cwd: rootDir,
            env: {
                ...process.env,
                ...env,
            },
            stdio: "inherit",
            shell: process.platform === "win32",
        });
        child.on("exit", (code, signal) => {
            resolve({ code, signal: signal as string | null });
        });
    });
}

function runCommandCapture(
    command: string,
    args: string[],
    env: NodeJS.ProcessEnv = {},
): Promise<{ stdout: string; stderr: string; code: number | null; signal: string | null }> {
    return new Promise((resolve) => {
        const child = spawn(command, args, {
            cwd: rootDir,
            env: {
                ...process.env,
                ...env,
            },
            stdio: ["ignore", "pipe", "pipe"],
            shell: process.platform === "win32",
        });
        let stdout = "";
        let stderr = "";
        child.stdout?.on("data", (chunk: Buffer | string) => {
            stdout += chunk.toString();
        });
        child.stderr?.on("data", (chunk: Buffer | string) => {
            stderr += chunk.toString();
        });
        child.on("exit", (code, signal) => {
            resolve({ stdout, stderr, code, signal: signal as string | null });
        });
    });
}

function parsePort(bind: string, fallback: number): number {
    const match = bind.match(/:(\d+)$/);
    if (!match) {
        return fallback;
    }
    const port = Number.parseInt(match[1], 10);
    return Number.isFinite(port) ? port : fallback;
}

async function terminateListeningProcesses(
    port: number,
    label: string,
): Promise<void> {
    const lookup = await runCommandCapture("lsof", [
        "-tiTCP:" + String(port),
        "-sTCP:LISTEN",
    ]);
    if (lookup.signal) {
        process.kill(process.pid, lookup.signal);
        return;
    }
    if (lookup.code !== 0 && lookup.code !== 1) {
        throw new Error(
            `failed to inspect ${label} port ${port}: ${lookup.stderr.trim() || lookup.stdout.trim()}`,
        );
    }

    const pids = lookup.stdout
        .split(/\s+/)
        .map((value) => value.trim())
        .filter((value) => value.length > 0)
        .filter((value) => value !== String(process.pid));
    if (pids.length === 0) {
        return;
    }

    console.log(
        `[preview:precomputed] terminating stale ${label} listener(s) on port ${port}: ${pids.join(", ")}`,
    );
    const killResult = await runCommand("kill", ["-TERM", ...pids]);
    if (killResult.signal) {
        process.kill(process.pid, killResult.signal);
        return;
    }
    if (killResult.code !== 0) {
        throw new Error(
            `failed to terminate stale ${label} listener(s) on port ${port}`,
        );
    }
}

async function readStoreFormatVersion(): Promise<number> {
    const source = await readFile(rustPrecomputeServerPath, "utf8");
    const match = source.match(/const STORE_FORMAT_VERSION: u32 = (\d+);/);
    if (!match) {
        throw new Error("STORE_FORMAT_VERSION not found in rust/src/precompute_server.rs");
    }
    return Number.parseInt(match[1], 10);
}

async function readManifest(
    storeDir: string,
    seed: string,
): Promise<PrecomputedManifest | null> {
    const manifestPath = path.join(storeDir, seed, "manifest.json");
    try {
        const raw = await readFile(manifestPath, "utf8");
        return JSON.parse(raw) as PrecomputedManifest;
    } catch {
        return null;
    }
}

function regenerationReason(
    manifest: PrecomputedManifest | null,
    expected: {
        formatVersion: number;
        seed: string;
        level: number;
        ticks: number;
        keyframeInterval: number;
    },
    options: {
        ignoreForceRebuild?: boolean;
    } = {},
): string | null {
    if (
        !options.ignoreForceRebuild &&
        process.env.FREY_PRECOMPUTE_FORCE_REBUILD === "true"
    ) {
        return "forced rebuild requested";
    }
    if (!manifest) {
        return "manifest is missing";
    }
    if (manifest.format_version !== expected.formatVersion) {
        return `store format mismatch (expected ${expected.formatVersion}, got ${manifest.format_version ?? "missing"})`;
    }
    if (manifest.seed !== expected.seed) {
        return `seed mismatch (expected ${expected.seed}, got ${manifest.seed ?? "missing"})`;
    }
    if (manifest.mesh_level !== expected.level) {
        return `mesh level mismatch (expected ${expected.level}, got ${manifest.mesh_level ?? "missing"})`;
    }
    if ((manifest.max_tick ?? -1) < expected.ticks) {
        return `max tick is too small (expected at least ${expected.ticks}, got ${manifest.max_tick ?? "missing"})`;
    }
    if (manifest.keyframe_interval !== expected.keyframeInterval) {
        return `keyframe interval mismatch (expected ${expected.keyframeInterval}, got ${manifest.keyframe_interval ?? "missing"})`;
    }
    return null;
}

async function ensurePrecomputedWorld(seed: string) {
    const level = envNumber("FREY_PRECOMPUTE_LEVEL", 6);
    const ticks = envNumber("FREY_PRECOMPUTE_TICKS", 1600);
    const keyframeInterval = envNumber("FREY_PRECOMPUTE_KEYFRAME_INTERVAL", 64);
    const storeDir =
        process.env.FREY_PRECOMPUTE_STORE_DIR ?? "data/precomputed/worlds";
    const formatVersion = await readStoreFormatVersion();
    const manifest = await readManifest(storeDir, seed);
    const reason = regenerationReason(manifest, {
        formatVersion,
        seed,
        level,
        ticks,
        keyframeInterval,
    });

    if (!reason) {
        console.log(
            `[preview:precomputed] using existing store seed=${seed} level=${level} ticks=${ticks}`,
        );
        return;
    }

    console.log(
        `[preview:precomputed] regenerating precomputed world: ${reason}`,
    );
    const result = await runCommand("corepack", [
        "pnpm",
        "precompute:world:release",
        "--",
        "--seed",
        seed,
        "--level",
        String(level),
        "--ticks",
        String(ticks),
        "--out-dir",
        storeDir,
        "--keyframe-interval",
        String(keyframeInterval),
    ]);
    if (result.signal) {
        process.kill(process.pid, result.signal);
        return;
    }
    if (result.code !== 0) {
        process.exit(result.code ?? 1);
    }

    const verifiedManifest = await readManifest(storeDir, seed);
    const verifiedReason = regenerationReason(verifiedManifest, {
        formatVersion,
        seed,
        level,
        ticks,
        keyframeInterval,
    }, {
        ignoreForceRebuild: true,
    });
    if (verifiedReason) {
        throw new Error(
            `[preview:precomputed] regenerated store is still invalid for seed=${seed}: ${verifiedReason}`,
        );
    }
    if ((verifiedManifest?.max_tick ?? 0) <= 0 && ticks > 0) {
        throw new Error(
            `[preview:precomputed] regenerated store for seed=${seed} has max_tick=${verifiedManifest?.max_tick ?? "missing"} despite requested ticks=${ticks}`,
        );
    }
}

async function cleanupStalePreviewProcesses() {
    const bind = process.env.FREY_PRECOMPUTE_BIND ?? "127.0.0.1:8787";
    const precomputePort = parsePort(bind, 8787);
    await terminateListeningProcesses(precomputePort, "precompute server");
    await terminateListeningProcesses(5173, "vite preview");
}

async function main() {
    const seeds = parseSeedList();
    for (const seed of seeds) {
        await ensurePrecomputedWorld(seed);
    }
    await cleanupStalePreviewProcesses();
    const initialSeed = seeds[0] ?? "alpha";
    const result = await runCommand(
        "corepack",
        ["pnpm", "dev:precomputed"],
        {
            FREY_PRECOMPUTE_SEED: initialSeed,
            FREY_PUBLIC_SEEDS: seeds.join(","),
        },
    );
    if (result.signal) {
        process.kill(process.pid, result.signal);
        return;
    }
    process.exit(result.code ?? 0);
}

void main();
