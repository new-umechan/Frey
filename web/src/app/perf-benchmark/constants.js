export const STEP_BREAKDOWN_METRIC_NAMES = [
    "step_feedback",
    "step_geology_terrain",
    "step_climate",
    "step_geology_river",
    "step_ecology",
    "step_civilization",
    "step_transition",
    "step_sync_erosion",
    "step_observe_world_change",
    "step_history_snapshot",
];

export const RIVER_BREAKDOWN_METRIC_NAMES = [
    "step_geology_river_prepare",
    "step_geology_river_automaton",
    "step_geology_river_automaton_sink",
    "step_geology_river_automaton_cell",
    "step_geology_river_automaton_queue",
    "step_geology_river_network",
    "step_geology_river_sync",
    "step_geology_river_fallback",
];

export const FLOAT32_FIELDS = new Set([
    "height",
    "river_flux",
    "mantle_heat",
    "temperature",
    "precipitation",
    "runoff",
    "ocean_temperature",
]);

export const DELTA_FIELD_KIND_BY_VIEW = Object.freeze({
    normal: ["height", "river_flux", "river_next"],
    plates: ["height", "river_flux", "river_next"],
    mantle: ["height", "river_flux", "river_next", "mantle_heat"],
});

export const CLIMATE_FIELD_KIND_BY_METRIC = Object.freeze({
    temperature: "temperature",
    precipitation: "precipitation",
});
