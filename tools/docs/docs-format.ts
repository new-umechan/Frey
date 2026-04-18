import { spawnSync } from "node:child_process";
import { collectMarkdownTargets, DEFAULT_REPO_ROOT } from "./markdown-targets.ts";

type FormatMode = "check" | "write";

function parseMode(argv: string[]): FormatMode {
    if (argv.includes("--write")) {
        return "write";
    }

    return "check";
}

function runPrettier(repoRoot: string = DEFAULT_REPO_ROOT, mode: FormatMode = "check"): number {
    const targets = collectMarkdownTargets(repoRoot);
    const result = spawnSync(
        "pnpm",
        [
            "exec",
            "prettier",
            mode === "write" ? "--write" : "--check",
            ...targets,
        ],
        {
            cwd: repoRoot,
            stdio: "inherit",
        },
    );

    if (result.error) {
        throw result.error;
    }

    return result.status ?? 1;
}

if (import.meta.url === `file://${process.argv[1]}`) {
    process.exit(runPrettier(DEFAULT_REPO_ROOT, parseMode(process.argv.slice(2))));
}
