mod common;
mod domains;
pub mod sim;
#[path = "generated/terrain_params_defaults.rs"]
mod terrain_params_defaults;
mod types;
mod wasm;
pub use sim::world;

pub use crate::types::{ErosionAutomatonState, MeshOutput, TerrainOutput, TerrainParams};
pub use crate::wasm::world_time::WorldTimeController;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct CrustTerrainAutomaton {
    inner: Option<domains::CrustTerrainUpdateState>,
}

#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    let output = domains::build_mesh(level).map_err(|err| JsValue::from_str(&err))?;
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize mesh output: {err}")))
}

#[wasm_bindgen]
pub fn generate_terrain(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    let terrain_params = if params_js.is_undefined() || params_js.is_null() {
        TerrainParams::default()
    } else {
        serde_wasm_bindgen::from_value::<TerrainParams>(params_js)
            .map_err(|err| JsValue::from_str(&format!("invalid terrain params: {err}")))?
    };

    let output = domains::build_terrain(&seed, terrain_params);
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize terrain output: {err}")))
}

#[wasm_bindgen]
impl CrustTerrainAutomaton {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: String, params_js: JsValue) -> Result<CrustTerrainAutomaton, JsValue> {
        let terrain_params = if params_js.is_undefined() || params_js.is_null() {
            TerrainParams::default()
        } else {
            serde_wasm_bindgen::from_value::<TerrainParams>(params_js)
                .map_err(|err| JsValue::from_str(&format!("invalid terrain params: {err}")))?
        };

        Ok(Self {
            inner: Some(domains::init_crust_terrain_update(&seed, terrain_params)),
        })
    }

    #[wasm_bindgen(js_name = step)]
    pub fn step_js(&mut self, budget_ticks: u32) -> bool {
        if let Some(state) = self.inner.as_mut() {
            domains::step_crust_terrain_update(state, budget_ticks);
            return domains::crust_terrain_update_is_done(state);
        }
        true
    }

    #[wasm_bindgen(js_name = isDone)]
    pub fn is_done_js(&self) -> bool {
        self.inner
            .as_ref()
            .map(domains::crust_terrain_update_is_done)
            .unwrap_or(true)
    }

    #[wasm_bindgen(js_name = phaseName)]
    pub fn phase_name_js(&self) -> String {
        self.inner
            .as_ref()
            .map(domains::crust_terrain_update_phase_name)
            .unwrap_or("finished")
            .to_string()
    }

    #[wasm_bindgen(js_name = finish)]
    pub fn finish_js(&mut self) -> Result<JsValue, JsValue> {
        let Some(state) = self.inner.take() else {
            return Err(JsValue::from_str(
                "crust terrain automaton already finished",
            ));
        };
        if !domains::crust_terrain_update_is_done(&state) {
            self.inner = Some(state);
            return Err(JsValue::from_str("crust terrain automaton is not done yet"));
        }
        let output = domains::finish_crust_terrain_update(state);
        serde_wasm_bindgen::to_value(&output).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize crust terrain automaton output: {err}"
            ))
        })
    }
}

#[wasm_bindgen]
pub fn init_erosion_automaton(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    let terrain_params = if params_js.is_undefined() || params_js.is_null() {
        TerrainParams::default()
    } else {
        serde_wasm_bindgen::from_value::<TerrainParams>(params_js)
            .map_err(|err| JsValue::from_str(&format!("invalid terrain params: {err}")))?
    };

    let state = domains::build_erosion_automaton(&seed, terrain_params);
    serde_wasm_bindgen::to_value(&state)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize erosion automaton: {err}")))
}

#[wasm_bindgen]
pub fn step_erosion_automaton(state_js: JsValue, budget_cells: u32) -> Result<JsValue, JsValue> {
    let mut state = serde_wasm_bindgen::from_value::<ErosionAutomatonState>(state_js)
        .map_err(|err| JsValue::from_str(&format!("invalid erosion automaton state: {err}")))?;
    domains::step_erosion_automaton(&mut state, budget_cells);
    serde_wasm_bindgen::to_value(&state).map_err(|err| {
        JsValue::from_str(&format!(
            "failed to serialize stepped erosion automaton state: {err}"
        ))
    })
}

#[wasm_bindgen]
pub fn build_render_positions(input_js: JsValue) -> Result<JsValue, JsValue> {
    let positions = wasm::visuals::build_render_positions_from_js(input_js)
        .map_err(|err| JsValue::from_str(&format!("failed to build render positions: {err}")))?;
    serde_wasm_bindgen::to_value(&positions)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize render positions: {err}")))
}

#[wasm_bindgen]
pub fn build_vertex_colors(input_js: JsValue) -> Result<JsValue, JsValue> {
    let colors = wasm::visuals::build_vertex_colors_from_js(input_js)
        .map_err(|err| JsValue::from_str(&format!("failed to build vertex colors: {err}")))?;
    serde_wasm_bindgen::to_value(&colors)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize vertex colors: {err}")))
}
