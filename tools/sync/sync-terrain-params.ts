import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, "..", "..");
const YAML_PATH = path.join(ROOT_DIR, "config", "terrain.yaml");
const JS_OUT_PATH = path.join(ROOT_DIR, "web", "src", "interface", "params", "terrain.js");
const RUST_OUT_PATH = path.join(ROOT_DIR, "rust", "src", "generated", "terrain_params_defaults.rs");

const SCHEMA = [
    ["mesh.level", "level", "u32"],
    ["spectral.harmonic_max_l", "harmonic_max_l", "u32"],
    ["spectral.spectral_alpha", "spectral_alpha", "f32"],
    ["plates.plate_count_min", "plate_count_min", "u32"],
    ["plates.plate_count_max", "plate_count_max", "u32"],
    ["plates.ocean_plate_ratio", "ocean_plate_ratio", "f32"],
    ["boundary.boundary_band", "boundary_band", "f32"],
    ["boundary.boundary_convergent_base_gain", "boundary_convergent_base_gain", "f32"],
    ["boundary.boundary_divergent_base_gain", "boundary_divergent_base_gain", "f32"],
    ["boundary.boundary_transform_relief_gain", "boundary_transform_relief_gain", "f32"],
    ["boundary.trench_gain", "trench_gain", "f32"],
    ["boundary.arc_gain", "arc_gain", "f32"],
    ["boundary.collision_gain", "collision_gain", "f32"],
    ["boundary.rift_gain", "rift_gain", "f32"],
    ["boundary.boundary_trench_width", "boundary_trench_width", "f32"],
    ["boundary.boundary_arc_width", "boundary_arc_width", "f32"],
    ["boundary.boundary_collision_width", "boundary_collision_width", "f32"],
    ["boundary.boundary_rift_width", "boundary_rift_width", "f32"],
    ["boundary.boundary_obliquity_mix", "boundary_obliquity_mix", "f32"],
    ["boundary.boundary_distance_falloff", "boundary_distance_falloff", "f32"],
    ["boundary.boundary_anisotropy", "boundary_anisotropy", "f32"],
    ["boundary.rollback_gain", "rollback_gain", "f32"],
    ["boundary.rollback_suppression", "rollback_suppression", "f32"],
    ["boundary.rollback_fraction_max", "rollback_fraction_max", "f32"],
    ["boundary.rollback_threshold", "rollback_threshold", "f32"],
    ["boundary.backarc_tension_gain", "backarc_tension_gain", "f32"],
    ["boundary.dip_density_scale", "dip_density_scale", "f32"],
    ["boundary.subduction_depth_gain", "subduction_depth_gain", "f32"],
    ["boundary.convergence_memory_rate", "convergence_memory_rate", "f32"],
    [
        "boundary.convergence_memory_spatial_smooth",
        "convergence_memory_spatial_smooth",
        "f32",
    ],
    ["boundary.arc_volcanism_gain", "arc_volcanism_gain", "f32"],
    ["boundary.ridge_volcanism_gain", "ridge_volcanism_gain", "f32"],
    ["boundary.hotspot_volcanism_gain", "hotspot_volcanism_gain", "f32"],
    ["boundary.backarc_volcanism_gain", "backarc_volcanism_gain", "f32"],
    ["boundary.volcanic_uplift_gain", "volcanic_uplift_gain", "f32"],
    ["boundary.volcanic_thickening_gain", "volcanic_thickening_gain", "f32"],
    ["river.river_rain_base", "river_rain_base", "f32"],
    ["river.river_accumulation_threshold", "river_accumulation_threshold", "f32"],
    ["river.sink_local_rebuild_radius", "sink_local_rebuild_radius", "u32"],
    ["river.sink_overflow_hysteresis", "sink_overflow_hysteresis", "f32"],
    ["river.sink_min_capacity", "sink_min_capacity", "f32"],
    ["river.erosion_iterations", "erosion_iterations", "u32"],
    ["river.hydraulic_erosion_rate", "hydraulic_erosion_rate", "f32"],
    ["river.hydraulic_deposit_rate", "hydraulic_deposit_rate", "f32"],
    ["river.sediment_capacity_gain", "sediment_capacity_gain", "f32"],
    ["river.erosion_min_slope", "erosion_min_slope", "f32"],
    ["river.erosion_max_delta_per_iter", "erosion_max_delta_per_iter", "f32"],
    ["river.coastal_deposit_rate", "coastal_deposit_rate", "f32"],
    ["river.shallow_sea_floor", "shallow_sea_floor", "f32"],
    ["river.river_inertia_gain", "river_inertia_gain", "f32"],
    ["river.river_curvature_penalty", "river_curvature_penalty", "f32"],
    ["river.baseflow_infiltration_rate", "baseflow_infiltration_rate", "f32"],
    ["river.baseflow_release_rate", "baseflow_release_rate", "f32"],
    ["river.baseflow_storage_cap", "baseflow_storage_cap", "f32"],
    ["continent.continent_competence_noise_gain", "continent_competence_noise_gain", "f32"],
    ["continent.continent_competence_large_scale", "continent_competence_large_scale", "f32"],
    ["continent.continent_competence_mid_scale", "continent_competence_mid_scale", "f32"],
    ["continent.continent_competence_weight_gain", "continent_competence_weight_gain", "f32"],
    ["continent.continent_foldability_from_competence", "continent_foldability_from_competence", "f32"],
    ["continent.continent_erodibility_from_competence", "continent_erodibility_from_competence", "f32"],
    ["continent.mantle_density", "mantle_density", "f32"],
    ["continent.continental_crust_density", "continental_crust_density", "f32"],
    ["continent.oceanic_base_density", "oceanic_base_density", "f32"],
    ["continent.age_density_gain", "age_density_gain", "f32"],
    ["continent.erosion_thickness_coupling", "erosion_thickness_coupling", "f32"],
    ["continent.deposition_thickness_coupling", "deposition_thickness_coupling", "f32"],
    ["time_dynamics.tectonic_uplift_gain", "tectonic_uplift_gain", "f32"],
    ["time_dynamics.plate_motion_gain", "plate_motion_gain", "f32"],
    ["time_dynamics.boundary_reclassify_interval", "boundary_reclassify_interval", "u32"],
    ["time_dynamics.river_rebuild_interval_min", "river_rebuild_interval_min", "u32"],
    ["time_dynamics.river_rebuild_interval_max", "river_rebuild_interval_max", "u32"],
    ["time_dynamics.river_activity_high_threshold", "river_activity_high_threshold", "f32"],
    ["time_dynamics.river_activity_low_threshold", "river_activity_low_threshold", "f32"],
    ["time_dynamics.tectonic_subsidence_gain", "tectonic_subsidence_gain", "f32"],
    ["time_dynamics.thermal_subsidence_gain", "thermal_subsidence_gain", "f32"],
    ["time_dynamics.stress_relaxation_rate", "stress_relaxation_rate", "f32"],
    ["time_dynamics.isostatic_adjustment_rate", "isostatic_adjustment_rate", "f32"],
    ["time_dynamics.subduction_age_coupling", "subduction_age_coupling", "f32"],
    ["time_dynamics.subduction_initiation_threshold", "subduction_initiation_threshold", "f32"],
    ["time_dynamics.subduction_density_threshold", "subduction_density_threshold", "f32"],
    ["time_dynamics.mantle_heat_input", "mantle_heat_input", "f32"],
    ["time_dynamics.mantle_heat_loss", "mantle_heat_loss", "f32"],
    ["time_dynamics.mantle_diffusion_rate", "mantle_diffusion_rate", "f32"],
    ["time_dynamics.plume_threshold", "plume_threshold", "f32"],
    ["time_dynamics.plume_gain", "plume_gain", "f32"],
    ["time_dynamics.plume_heat_release_rate", "plume_heat_release_rate", "f32"],
    ["time_dynamics.uplift_saturation_soft", "uplift_saturation_soft", "f32"],
    ["time_dynamics.uplift_saturation_hard", "uplift_saturation_hard", "f32"],
    ["time_dynamics.age_advection_gain", "age_advection_gain", "f32"],
    ["time_dynamics.nonlinear_diffusion_gain", "nonlinear_diffusion_gain", "f32"],
    ["time_dynamics.isostatic_relax_gain", "isostatic_relax_gain", "f32"],
    ["time_dynamics.age_ref", "age_ref", "f32"],
];

function stripInlineComment(value: string) {
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

function parseYamlNumericScalars(text: string) {
    type StackEntry = { key: string; indent: number };
    const result = new Map<string, { raw: string; value: number }>();
    const lines = text.split(/\r?\n/);
    const stack: StackEntry[] = [];

    lines.forEach((line: string, index: number) => {
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

function validateAgainstSchema(parsed: Map<string, { raw: string; value: number }>) {
    const schemaKeys = new Set(SCHEMA.map(([pathKey]) => pathKey));
    const parsedKeys = new Set(parsed.keys());

    for (const key of schemaKeys) {
        if (!parsedKeys.has(key)) {
            throw new Error(`Missing key in YAML: "${key}"`);
        }
    }

    for (const key of parsedKeys) {
        if (!schemaKeys.has(key as string)) {
            throw new Error(`Unknown key in YAML: "${key}"`);
        }
    }

    for (const [pathKey, _outKey, rustType] of SCHEMA) {
        const entry = parsed.get(pathKey);
        if (entry !== undefined && rustType === "u32" && !Number.isInteger(entry.value)) {
            throw new Error(`Key "${pathKey}" must be an integer for Rust type u32`);
        }
    }
}

function buildJsModule(parsed: Map<string, { raw: string; value: number }>) {
    const lines: string[] = [];
    lines.push("// AUTO-GENERATED by tools/sync/sync-terrain-params.mjs");
    lines.push("// Source: config/terrain.yaml");
    lines.push("");
    lines.push("export const TERRAIN_PARAMS = Object.freeze({");

    for (const [pathKey, outKey] of SCHEMA) {
        lines.push(`    ${outKey}: ${parsed.get(pathKey)!.raw},`);
    }

    lines.push("});");
    lines.push("");
    lines.push("export const TERRAIN_LEVEL = TERRAIN_PARAMS.level;");
    lines.push("");
    return `${lines.join("\n")}\n`;
}

function rustLiteral(raw: string, rustType: string) {
    if (rustType === "f32") {
        return `${raw}f32`;
    }
    return `${raw}`;
}

function buildRustModule(parsed: Map<string, { raw: string; value: number }>) {
    const lines: string[] = [];
    lines.push("// AUTO-GENERATED by tools/sync/sync-terrain-params.mjs");
    lines.push("// Source: config/terrain.yaml");
    lines.push("");
    lines.push("use crate::GeologyParams;");
    lines.push("");
    lines.push("pub(crate) fn build_default_terrain_params() -> GeologyParams {");
    lines.push("    GeologyParams {");

    for (const [pathKey, outKey, rustType] of SCHEMA) {
        lines.push(`        ${outKey}: ${rustLiteral(parsed.get(pathKey)!.raw, rustType)},`);
    }

    lines.push("    }");
    lines.push("}");
    return `${lines.join("\n")}\n`;
}

function ensureParentDir(filePath: string) {
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function writeFileIfChanged(filePath: string, nextContent: string) {
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

    const jsChanged = writeFileIfChanged(JS_OUT_PATH, buildJsModule(parsed));
    const rustChanged = writeFileIfChanged(RUST_OUT_PATH, buildRustModule(parsed));

    const changedCount = Number(jsChanged) + Number(rustChanged);
    console.log(
        `terrain params synced from ${path.relative(ROOT_DIR, YAML_PATH)} (${changedCount} file(s) updated)`,
    );
}

main();
