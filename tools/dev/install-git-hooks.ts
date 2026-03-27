import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../..");
const sourceHookPath = path.join(repoRoot, ".githooks", "pre-push");
const targetHookPath = path.join(repoRoot, ".git", "hooks", "pre-push");

if (!fs.existsSync(path.join(repoRoot, ".git"))) {
    console.error("error: .git directory not found");
    process.exit(1);
}

if (!fs.existsSync(sourceHookPath)) {
    console.error("error: source hook not found: .githooks/pre-push");
    process.exit(1);
}

fs.mkdirSync(path.dirname(targetHookPath), { recursive: true });
fs.copyFileSync(sourceHookPath, targetHookPath);
fs.chmodSync(targetHookPath, 0o755);

console.log("installed git hook:", targetHookPath);
