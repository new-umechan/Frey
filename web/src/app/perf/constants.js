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
    "erosion_rate",
    "deposition_rate",
    "temperature",
    "precipitation",
    "evapotranspiration",
    "aridity",
    "runoff",
    "ocean_temperature",
    "river_transport_cost",
]);

export const DELTA_FIELD_KIND_BY_VIEW = Object.freeze({
    normal: ["height", "river_flux", "river_next"],
    metric: ["height", "river_flux", "river_next"],
});

export const FIELD_KIND_BY_CELL_METRIC = Object.freeze({
    height: "height",
    mantle_heat: "mantle_heat",
    erosion_rate: "erosion_rate",
    deposition_rate: "deposition_rate",
    temperature: "temperature",
    precipitation: "precipitation",
    evapotranspiration: "evapotranspiration",
    aridity: "aridity",
    ocean_temperature: "ocean_temperature",
    river_flux: "river_flux",
    runoff: "runoff",
    river_transport_cost: "river_transport_cost",
});
