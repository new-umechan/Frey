use std::collections::HashSet;

use wasm_bindgen::prelude::*;

use crate::application::world_dto::WorldDeltaQuery;
use crate::application::world_query_use_cases;

use super::super::WorldSimController;

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(js_name = get_field)]
    pub fn get_field_js(
        &self,
        world_id: String,
        field_kind: String,
        lod: u32,
    ) -> Result<JsValue, JsValue> {
        let response = world_query_use_cases::get_field(&self.service, &world_id, field_kind, lod)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize field response: {err}")))
    }

    #[wasm_bindgen(js_name = get_metrics)]
    pub fn get_metrics_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let response = world_query_use_cases::get_metrics(&self.service, world_id)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize metrics: {err}")))
    }

    #[wasm_bindgen(js_name = get_world_delta)]
    pub fn get_world_delta_js(
        &mut self,
        world_id: String,
        options_js: JsValue,
    ) -> Result<JsValue, JsValue> {
        let include_fields: Option<HashSet<String>> = if options_js.is_undefined()
            || options_js.is_null()
        {
            None
        } else {
            let query = serde_wasm_bindgen::from_value::<WorldDeltaQuery>(options_js)
                .map_err(|err| JsValue::from_str(&format!("invalid world delta query: {err}")))?;
            query
                .include_fields
                .map(|fields| fields.into_iter().collect::<HashSet<String>>())
        };

        let response =
            world_query_use_cases::get_world_delta(&mut self.service, world_id, include_fields)
                .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize world delta: {err}")))
    }

    #[wasm_bindgen(js_name = get_plate_stats)]
    pub fn get_plate_stats_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let response = world_query_use_cases::get_plate_stats(&self.service, world_id)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize plate stats: {err}")))
    }

    #[wasm_bindgen(js_name = list_history_ticks)]
    pub fn list_history_ticks_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let response = world_query_use_cases::list_history_ticks(&self.service, world_id)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize history ticks response: {err}"
            ))
        })
    }
}
