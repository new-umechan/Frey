use wasm_bindgen::prelude::*;

use crate::application::world_dto::InitWorldConfig;
use crate::application::world_use_cases;
use crate::sim::{module_doc_records, module_graph_record};

use super::super::WorldSimController;

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(js_name = exec_modules)]
    pub fn exec_modules_js(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&module_doc_records())
            .map_err(|err| JsValue::from_str(&format!("failed to serialize exec modules: {err}")))
    }

    #[wasm_bindgen(js_name = exec_module_graph)]
    pub fn exec_module_graph_js(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&module_graph_record()).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize exec module graph: {err}"))
        })
    }

    #[wasm_bindgen(js_name = init_world)]
    pub fn init_world_js(
        &mut self,
        seed: String,
        mesh_level: u32,
        config_js: JsValue,
    ) -> Result<JsValue, JsValue> {
        let config = if config_js.is_undefined() || config_js.is_null() {
            InitWorldConfig {
                geology_params: None,
                simulation_rate: None,
                verification_mode: None,
                timeline: None,
            }
        } else {
            serde_wasm_bindgen::from_value::<InitWorldConfig>(config_js)
                .map_err(|err| JsValue::from_str(&format!("invalid init config: {err}")))?
        };

        let output = world_use_cases::init_world(&mut self.service, seed, mesh_level, config)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&output)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize init result: {err}")))
    }

    #[wasm_bindgen(js_name = init_world_from_snapshot)]
    pub fn init_world_from_snapshot_js(
        &mut self,
        seed: String,
        mesh_level: u32,
        config_js: JsValue,
        snapshot_bytes: Vec<u8>,
    ) -> Result<JsValue, JsValue> {
        let config = if config_js.is_undefined() || config_js.is_null() {
            InitWorldConfig {
                geology_params: None,
                simulation_rate: None,
                verification_mode: None,
                timeline: None,
            }
        } else {
            serde_wasm_bindgen::from_value::<InitWorldConfig>(config_js)
                .map_err(|err| JsValue::from_str(&format!("invalid init config: {err}")))?
        };

        let output = world_use_cases::init_world_from_snapshot_bytes(
            &mut self.service,
            seed,
            mesh_level,
            config,
            &snapshot_bytes,
        )
        .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&output).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize init snapshot result: {err}"))
        })
    }

    #[wasm_bindgen(js_name = exec_world)]
    pub fn exec_world_js(&mut self, world_id: String, tick_count: u32) -> Result<(), JsValue> {
        world_use_cases::exec_world(&mut self.service, &world_id, tick_count)
            .map_err(|err| JsValue::from_str(&err))
    }

    #[wasm_bindgen(js_name = advance_timeline)]
    pub fn advance_timeline_js(
        &mut self,
        world_id: String,
        tick_count: u32,
    ) -> Result<JsValue, JsValue> {
        let response = world_use_cases::advance_timeline(&mut self.service, world_id, tick_count)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize advance_timeline: {err}"))
        })
    }

    #[wasm_bindgen(js_name = exec_world_profiled)]
    pub fn exec_world_profiled_js(
        &mut self,
        world_id: String,
        tick_count: u32,
    ) -> Result<JsValue, JsValue> {
        let response =
            world_use_cases::exec_world_profiled(&mut self.service, world_id, tick_count)
                .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize exec_world_profiled: {err}"))
        })
    }

    #[wasm_bindgen(js_name = exec_world_profiled_detail)]
    pub fn exec_world_profiled_detail_js(
        &mut self,
        world_id: String,
        tick_count: u32,
    ) -> Result<JsValue, JsValue> {
        let response =
            world_use_cases::exec_world_profiled_detail(&mut self.service, world_id, tick_count)
                .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize exec_world_profiled_detail: {err}"
            ))
        })
    }

    #[wasm_bindgen(js_name = exec_world_slice)]
    pub fn exec_world_slice_js(
        &mut self,
        world_id: String,
        work_budget: u32,
    ) -> Result<JsValue, JsValue> {
        let response = world_use_cases::exec_world_slice(&mut self.service, world_id, work_budget)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize exec_world_slice: {err}"))
        })
    }

    #[wasm_bindgen(js_name = set_simulation_rate)]
    pub fn set_simulation_rate_js(&mut self, world_id: String, rate: f32) -> Result<(), JsValue> {
        world_use_cases::set_simulation_rate(&mut self.service, &world_id, rate)
            .map_err(|err| JsValue::from_str(&err))
    }
}
