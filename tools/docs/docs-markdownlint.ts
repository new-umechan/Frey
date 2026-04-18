import { spawnSync } from "node:child_process";
import { collectMarkdownTargets, DEFAULT_REPO_ROOT } from "./markdown-targets.ts";

function runMarkdownLint(repoRoot: string = DEFAULT_REPO_ROOT): number {
    const targets = collectMarkdownTargets(repoRoot);
    const result = spawnSync(
        "pnpm",
        ["exec", "markdownlint-cli2", ...targets],
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
    process.exit(runMarkdownLint());
}
