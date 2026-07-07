import { execFile } from "node:child_process";
import process from "node:process";

const previewPorts = ["5173", "8787"];
const fallbackPatterns = [
    "tools/dev/dev-precomputed.ts",
    "vite --config web/vite.config.ts",
    "precompute_server",
];

function execFileQuiet(command: string, args: string[]): Promise<string> {
    return new Promise((resolve) => {
        execFile(command, args, (error, stdout) => {
            if (error) {
                resolve("");
                return;
            }
            resolve(stdout.trim());
        });
    });
}

async function pidsUsingPorts(): Promise<Set<number>> {
    const pids = new Set<number>();
    for (const port of previewPorts) {
        const output = await execFileQuiet("lsof", ["-ti", `tcp:${port}`]);
        for (const line of output.split(/\s+/)) {
            const pid = Number.parseInt(line, 10);
            if (Number.isFinite(pid) && pid > 0 && pid !== process.pid) {
                pids.add(pid);
            }
        }
    }
    return pids;
}

async function fallbackPids(): Promise<Set<number>> {
    const pids = new Set<number>();
    if (process.platform === "win32") {
        return pids;
    }
    for (const pattern of fallbackPatterns) {
        const output = await execFileQuiet("pgrep", ["-f", pattern]);
        for (const line of output.split(/\s+/)) {
            const pid = Number.parseInt(line, 10);
            if (Number.isFinite(pid) && pid > 0 && pid !== process.pid) {
                pids.add(pid);
            }
        }
    }
    return pids;
}

function killPids(pids: Set<number>, signal: NodeJS.Signals) {
    for (const pid of pids) {
        try {
            process.kill(pid, signal);
        } catch {
            // The process may have already exited between discovery and kill.
        }
    }
}

function delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

async function main() {
    const pids = await pidsUsingPorts();
    for (const pid of await fallbackPids()) {
        pids.add(pid);
    }

    if (pids.size === 0) {
        console.log("[dev] no preview processes found");
        return;
    }

    console.log(`[dev] stopping preview processes: ${[...pids].join(", ")}`);
    killPids(pids, "SIGTERM");
    await delay(750);
    killPids(pids, "SIGKILL");
}

void main();
