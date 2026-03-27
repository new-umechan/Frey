import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, "..", "..");
const YAML_PATH = path.join(ROOT_DIR, "config", "climate.yaml");
const RUST_OUT_PATH = path.join(ROOT_DIR, "rust", "src", "generated", "climate_params_defaults.rs");

const SCHEMA = [
    ["temperature.lapse_rate_c_per_km", "lapse_rate_c_per_km", "f32"],
    ["temperature.height_to_meters", "height_to_meters", "f32"],
    ["precipitation.min_mm", "precip_min_mm", "f32"],
    ["precipitation.max_mm", "precip_max_mm", "f32"],
    ["precipitation.hadley_anomaly_gain", "hadley_anomaly_gain", "f32"],
    ["precipitation.distance_scale_km", "distance_scale_km", "f32"],
    ["precipitation.continentality_gain", "continentality_gain", "f32"],
    ["precipitation.moisture_convergence_gain", "moisture_convergence_gain", "f32"],
    ["precipitation.convergence_min_mm", "convergence_min_mm", "f32"],
    ["precipitation.convergence_max_mm", "convergence_max_mm", "f32"],
    ["precipitation.convergence_blend", "convergence_blend", "f32"],
    ["orography.uplift_gain_mm", "orographic_uplift_gain_mm", "f32"],
    ["orography.rise_scale_m", "orographic_rise_scale_m", "f32"],
    ["orography.trace_steps", "orographic_trace_steps", "u32"],
    ["orography.trace_alignment_min", "orographic_trace_alignment_min", "f32"],
    ["orography.step_decay", "orographic_step_decay", "f32"],
    ["orography.rain_shadow_gain", "rain_shadow_gain", "f32"],
    ["orography.rain_shadow_scale_m", "rain_shadow_scale_m", "f32"],
    ["orography.rain_shadow_distance_km", "rain_shadow_distance_km", "f32"],
    ["orography.downwind_depletion_gain", "downwind_depletion_gain", "f32"],
    ["orography.downwind_depletion_max", "downwind_depletion_max", "f32"],
    ["orography.downwind_depletion_steps", "downwind_depletion_steps", "u32"],
    ["orography.downwind_depletion_decay", "downwind_depletion_decay", "f32"],
    ["orography.downwind_alignment_min", "downwind_alignment_min", "f32"],
    ["precipitation.cap_from_moisture", "precip_cap_from_moisture", "f32"],
    ["coastal.cold_coast_gain", "cold_coast_gain", "f32"],
];

function stripInlineComment(value) {
    let inSingle = false;
    let inDouble = false;

    for (let i = 0; i < value.length; i += 1) {
        const ch = value[i];
        if (ch === "'" && !inDouble) {
            inSingle = !inSingle;
            continue;
        }
        if (ch === "\"" && !inSingle) {
            inDouble = !inDouble;
            continue;
        }
        if (ch === "#" && !inSingle && !inDouble) {
            return value.slice(0, i).trim();
        }
    }

    return value.trim();
}

function parseYamlNumericScalars(text) {
    const result = new Map();
    const lines = text.split(/\r?\n/);
    const stack = [];

    lines.forEach((line, index) => {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith("#")) {
            return;
        }
        if (/\t/.test(line)) {
            throw new Error(`Invalid YAML line ${index + 1}: tab indentation is not supported`);
        }

        const colonIndex = line.indexOf(":");
        if (colonIndex < 0) {
            throw new Error(`Invalid YAML line ${index + 1}: missing ":"`);
        }

        const indent = line.length - line.trimStart().length;
        if (indent % 2 !== 0) {
            throw new Error(`Invalid YAML line ${index + 1}: indentation must use 2-space units`);
        }
        while (stack.length > 0 && indent <= stack[stack.length - 1].indent) {
            stack.pop();
        }

        const localKey = line.slice(0, colonIndex).trim();
        const rawValue = stripInlineComment(line.slice(colonIndex + 1));
        if (!localKey) {
            throw new Error(`Invalid YAML line ${index + 1}: empty key`);
        }

        if (!rawValue) {
            stack.push({ key: localKey, indent });
            return;
        }

        const keyPath = [...stack.map((entry) => entry.key), localKey].join(".");
        if (result.has(keyPath)) {
            throw new Error(`Duplicate key "${keyPath}" at line ${index + 1}`);
        }

        const isNumberLiteral = /^-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(rawValue);
        if (!isNumberLiteral) {
            throw new Error(
                `Unsupported value for "${keyPath}" at line ${index + 1}: expected numeric scalar`,
            );
        }

        const value = Number(rawValue);
        if (!Number.isFinite(value)) {
            throw new Error(`Invalid numeric value for "${keyPath}" at line ${index + 1}`);
        }

        result.set(keyPath, { raw: rawValue, value });
    });

    return result;
}

function validateAgainstSchema(parsed) {
    const schemaKeys = new Set(SCHEMA.map(([pathKey]) => pathKey));
    const parsedKeys = new Set(parsed.keys());

    for (const key of schemaKeys) {
        if (!parsedKeys.has(key)) {
            throw new Error(`Missing key in YAML: "${key}"`);
        }
    }

    for (const key of parsedKeys) {
        if (!schemaKeys.has(key)) {
            throw new Error(`Unknown key in YAML: "${key}"`);
        }
    }
}

function renderValue(rawValue, typeHint) {
    if (typeHint === "u32") {
        const parsed = Number(rawValue);
        if (!Number.isInteger(parsed) || parsed < 0) {
            throw new Error(`Expected unsigned integer but got "${rawValue}"`);
        }
        return `${parsed}`;
    }
    if (typeHint === "f32") {
        return `${rawValue}f32`;
    }
    throw new Error(`Unsupported type hint: ${typeHint}`);
}

function buildRustModule(parsed) {
    const lines = [];
    lines.push("// AUTO-GENERATED by tools/sync/sync-climate-params.mjs");
    lines.push("// Source: config/climate.yaml");
    lines.push("");
    lines.push("use crate::sim::climate::types::ClimateParams;");
    lines.push("");
    lines.push("pub(crate) fn build_default_climate_params() -> ClimateParams {");
    lines.push("    ClimateParams {");

    for (const [pathKey, outKey, typeHint] of SCHEMA) {
        const raw = parsed.get(pathKey).raw;
        lines.push(`        ${outKey}: ${renderValue(raw, typeHint)},`);
    }

    lines.push("    }");
    lines.push("}");
    lines.push("");
    return `${lines.join("\n")}\n`;
}

function ensureParentDir(filePath) {
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function writeFileIfChanged(filePath, nextContent) {
    const currentContent = fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : null;
    if (currentContent === nextContent) {
        return false;
    }
    ensureParentDir(filePath);
    fs.writeFileSync(filePath, nextContent);
    return true;
}

function main() {
    const yamlText = fs.readFileSync(YAML_PATH, "utf8");
    const parsed = parseYamlNumericScalars(yamlText);
    validateAgainstSchema(parsed);

    const changed = writeFileIfChanged(RUST_OUT_PATH, buildRustModule(parsed));
    console.log(
        `climate params synced from ${path.relative(ROOT_DIR, YAML_PATH)} (${Number(changed)} file(s) updated)`,
    );
}

main();
