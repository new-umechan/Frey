use wasm_bindgen::prelude::JsValue;

pub use crate::wasm_api::world_sim::WorldSimController;

pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    let output = crate::core_api::generate_mesh(level).map_err(|err| JsValue::from_str(&err))?;
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize mesh output: {err}")))
}

pub fn generate_geology(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    let geology_params = if params_js.is_undefined() || params_js.is_null() {
        crate::sim::geology_types::GeologyParams::default()
    } else {
        serde_wasm_bindgen::from_value::<crate::sim::geology_types::GeologyParams>(params_js)
            .map_err(|err| JsValue::from_str(&format!("invalid terrain params: {err}")))?
    };

    let output = crate::core_api::generate_geology(&seed, geology_params);
    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize terrain output: {err}")))
}

pub fn build_render_positions(input_js: JsValue) -> Result<JsValue, JsValue> {
    let positions = crate::wasm_api::visuals::build_render_positions_from_js(input_js)
        .map_err(|err| JsValue::from_str(&format!("failed to build render positions: {err}")))?;
    serde_wasm_bindgen::to_value(&positions)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize render positions: {err}")))
}
