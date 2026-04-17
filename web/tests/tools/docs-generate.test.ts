import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { runDocsGenerate } from "../../../tools/docs/generate-docs";

function makeTempRepo(): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), "frey-docs-generate-"));
}

function writeFile(repoRoot: string, relativePath: string, content: string): void {
    const filePath = path.join(repoRoot, relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content);
}

const tempDirs: string[] = [];

afterEach(() => {
    for (const dir of tempDirs.splice(0)) {
        fs.rmSync(dir, { recursive: true, force: true });
    }
});

describe("docs-generate", () => {
    it("generates bench README from module directories", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "docs/operations/bench/climate/solo.md", "# climate");
        writeFile(repoRoot, "docs/operations/bench/hydrology/solo.md", "# hydrology");
        writeFile(repoRoot, "docs/operations/bench/hydrology/tuning.md", "# tuning");

        const logs: string[] = [];
        const exitCode = runDocsGenerate(repoRoot, (line) => logs.push(line));

        expect(exitCode).toBe(0);
        expect(logs).toEqual(["updated docs/operations/bench/README.md"]);

        const generated = fs.readFileSync(
            path.join(repoRoot, "docs/operations/bench/README.md"),
            "utf8",
        );

        expect(generated).toContain("### `climate/` (Climate)");
        expect(generated).toContain("`docs/operations/bench/climate/solo.md`");
        expect(generated).toContain("### `hydrology/` (Hydrology)");
        expect(generated).toContain("`docs/operations/bench/hydrology/solo.md`");
        expect(generated).toContain("`docs/operations/bench/hydrology/tuning.md`");
    });

    it("is idempotent when content is unchanged", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "docs/operations/bench/geology/validation.md", "# geology");

        const firstLogs: string[] = [];
        runDocsGenerate(repoRoot, (line) => firstLogs.push(line));
        expect(firstLogs).toEqual(["updated docs/operations/bench/README.md"]);

        const secondLogs: string[] = [];
        runDocsGenerate(repoRoot, (line) => secondLogs.push(line));
        expect(secondLogs).toEqual(["unchanged docs/operations/bench/README.md"]);
    });
});
