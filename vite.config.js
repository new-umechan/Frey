import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { defineConfig } from "vite";

function terrainSnapshotPlugin() {
    return {
        name: "terrain-debug-snapshot",
        configureServer(server) {
            server.middlewares.use("/__debug/snapshot", async (req, res) => {
                if (req.method !== "POST") {
                    res.statusCode = 405;
                    res.setHeader("content-type", "application/json; charset=utf-8");
                    res.end(JSON.stringify({ ok: false, error: "method_not_allowed" }));
                    return;
                }

                try {
                    const chunks = [];
                    for await (const chunk of req) {
                        chunks.push(chunk);
                    }
                    const raw = Buffer.concat(chunks).toString("utf8");
                    const payload = JSON.parse(raw);

                    const tick = Number.isFinite(payload?.tick) ? Math.floor(payload.tick) : -1;
                    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
                    const snapshotsDir = path.join(process.cwd(), "debug", "snapshots");
                    await mkdir(snapshotsDir, { recursive: true });

                    const id = `${tick >= 0 ? `tick-${tick}` : "tick-unknown"}-${stamp}`;
                    const filePath = path.join(snapshotsDir, `${id}.json`);
                    const latestPath = path.join(snapshotsDir, "latest.json");
                    const content = `${JSON.stringify(payload, null, 4)}\n`;

                    await writeFile(filePath, content, "utf8");
                    await writeFile(latestPath, content, "utf8");

                    res.statusCode = 200;
                    res.setHeader("content-type", "application/json; charset=utf-8");
                    res.end(JSON.stringify({ ok: true, id, file: `debug/snapshots/${id}.json` }));
                } catch (error) {
                    res.statusCode = 400;
                    res.setHeader("content-type", "application/json; charset=utf-8");
                    res.end(
                        JSON.stringify({
                            ok: false,
                            error: "invalid_snapshot_payload",
                            message: error instanceof Error ? error.message : String(error),
                        }),
                    );
                }
            });
        },
    };
}

export default defineConfig({
    plugins: [terrainSnapshotPlugin()],
});
