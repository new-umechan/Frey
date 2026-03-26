import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

import initWasm, { generate_mesh } from "../../generated/wasm/web/frey_wasm.js";

function parseNumber(value, name) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
        throw new Error(`${name} must be a finite number`);
    }
    return parsed;
}

function parseArgs(argv) {
    const args = {
        level: 6,
        out: "bench/data/cell_centroids.csv",
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--level":
            args.level = Math.max(0, Math.floor(parseNumber(next, "--level")));
            i += 1;
            break;
        case "--out":
            args.out = String(next);
            i += 1;
            break;
        case "--help":
            printHelp();
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }

    return args;
}

function printHelp() {
    console.error("Usage: node bench/scripts/dump-cell-centroids.mjs [options]");
    console.error("  --level <n>");
    console.error("  --out <path>");
}

async function initWasmForNode() {
    const wasmPath = new URL("../../generated/wasm/web/frey_wasm_bg.wasm", import.meta.url);
    const wasmBytes = await readFile(wasmPath);
    try {
        await initWasm({ module_or_path: wasmBytes });
    } catch {
        await initWasm(wasmBytes);
    }
}

function normalizeLongitude(lonDeg) {
    let lon = lonDeg;
    while (lon <= -180) {
        lon += 360;
    }
    while (lon > 180) {
        lon -= 360;
    }
    return lon;
}

function buildCsv(positionsFlat) {
    const lines = ["cell_id,latitude,longitude"];
    const count = Math.floor(positionsFlat.length / 3);
    for (let i = 0; i < count; i += 1) {
        const x = positionsFlat[i * 3 + 0];
        const y = positionsFlat[i * 3 + 1];
        const z = positionsFlat[i * 3 + 2];
        const lat = Math.asin(Math.max(-1, Math.min(1, y))) * (180 / Math.PI);
        const lon = Math.atan2(z, x) * (180 / Math.PI);
        lines.push(`${i},${lat.toFixed(8)},${normalizeLongitude(lon).toFixed(8)}`);
    }
    return `${lines.join("\n")}\n`;
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    await initWasmForNode();

    const meshJs = await generate_mesh(args.level);
    const mesh = meshJs && typeof meshJs === "object" ? meshJs : {};
    const positions = Array.isArray(mesh.positions) ? mesh.positions : [];
    if (positions.length === 0) {
        throw new Error("generate_mesh returned empty positions");
    }

    const csv = buildCsv(positions);
    const outPath = resolve(args.out);
    await mkdir(dirname(outPath), { recursive: true });
    await writeFile(outPath, csv, "utf8");

    process.stdout.write(`WROTE ${outPath}\n`);
    process.stdout.write(`CELL_COUNT ${Math.floor(positions.length / 3)}\n`);
}

main().catch((error) => {
    process.stderr.write(`${String(error?.stack ?? error)}\n`);
    process.exit(1);
});
