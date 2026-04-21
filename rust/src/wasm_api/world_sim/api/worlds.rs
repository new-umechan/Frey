use wasm_bindgen::prelude::*;

use crate::application::world_dto::InitWorldConfig;
use crate::application::world_use_cases;
use crate::application::world_validation::{validate_integer_tick, validate_non_negative_tick};
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
                target_sea_ratio: None,
                simulation_rate: None,
                verification_mode: None,
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

    #[wasm_bindgen(js_name = exec_world)]
    pub fn exec_world_js(&mut self, world_id: String, tick_count: u32) -> Result<(), JsValue> {
        world_use_cases::exec_world(&mut self.service, &world_id, tick_count)
            .map_err(|err| JsValue::from_str(&err))
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

    #[wasm_bindgen(js_name = set_target_sea_ratio)]
    pub fn set_target_sea_ratio_js(
        &mut self,
        world_id: String,
        target_sea_ratio: f32,
    ) -> Result<(), JsValue> {
        world_use_cases::set_target_sea_ratio(&mut self.service, &world_id, target_sea_ratio)
            .map_err(|err| JsValue::from_str(&err))
    }

    #[wasm_bindgen(js_name = fork_world)]
    pub fn fork_world_js(&mut self, world_id: String, tick: f64) -> Result<JsValue, JsValue> {
        let tick_u64 = validate_non_negative_tick(tick).map_err(|err| JsValue::from_str(&err))?;
        validate_integer_tick(tick, tick_u64).map_err(|err| JsValue::from_str(&err))?;

        let response = world_use_cases::fork_world(&mut self.service, world_id, tick_u64)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize fork_world: {err}")))
    }
}
