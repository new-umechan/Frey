import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, "..", "..", "..");
const D_TS_PATH = path.join(ROOT_DIR, "generated", "wasm", "web", "frey_wasm.d.ts");

const REQUIRED_CLASS_SNIPPETS = [
    "export class WorldSimController {",
    "exec_module_graph(): any;",
    "exec_modules(): any;",
    "init_world(seed: string, mesh_level: number, config_js: any): any;",
    "exec_world(world_id: string, tick_count: number): void;",
    "exec_world_slice(world_id: string, work_budget: number): any;",
    "exec_world_profiled(world_id: string, tick_count: number): any;",
    "exec_world_profiled_detail(world_id: string, tick_count: number): any;",
    "get_world_delta(world_id: string, options_js: any): any;",
    "get_metrics(world_id: string): any;",
    "get_scientific_benchmark_samples(world_id: string): any;",
    "get_plate_stats(world_id: string): any;",
    "get_field(world_id: string, field_kind: string, lod: number): any;",
    "list_history_ticks(world_id: string): any;",
    "restore_world_to_tick(world_id: string, tick: number): any;",
    "set_simulation_rate(world_id: string, rate: number): void;",
    "set_target_sea_ratio(world_id: string, target_sea_ratio: number): void;",
    "fork_world(world_id: string, tick: number): any;",
];

const REQUIRED_FUNCTION_SNIPPETS = [
    "export function build_render_positions(input_js: any): any;",
    "export function generate_geology(seed: string, params_js: any): any;",
    "export function generate_mesh(level: number): any;",
    "export default function __wbg_init",
];

function main() {
    if (!fs.existsSync(D_TS_PATH)) {
        throw new Error(`contract target not found: ${path.relative(ROOT_DIR, D_TS_PATH)}`);
    }
    const text = fs.readFileSync(D_TS_PATH, "utf8");
    const requiredSnippets = [...REQUIRED_CLASS_SNIPPETS, ...REQUIRED_FUNCTION_SNIPPETS];
    const missing = requiredSnippets.filter((snippet) => !text.includes(snippet));
    if (missing.length > 0) {
        throw new Error(
            `WASM API contract mismatch (${missing.length} missing snippet(s)):\n${missing
                .map((item) => `- ${item}`)
                .join("\n")}`,
        );
    }
    console.log(`PASS: ${path.relative(ROOT_DIR, D_TS_PATH)} (${requiredSnippets.length} checks)`);
}

main();
