use wasm_bindgen::prelude::*;

use crate::application::world_use_cases;
use crate::application::world_validation::{validate_integer_tick, validate_non_negative_tick};

use super::super::WorldSimController;

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(js_name = seek_world_to_tick)]
    pub fn seek_world_to_tick_js(
        &mut self,
        world_id: String,
        tick: f64,
    ) -> Result<JsValue, JsValue> {
        let tick_u64 = validate_non_negative_tick(tick).map_err(|err| JsValue::from_str(&err))?;
        validate_integer_tick(tick, tick_u64).map_err(|err| JsValue::from_str(&err))?;

        let result = world_use_cases::seek_world_to_tick(&mut self.service, world_id, tick_u64)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize seek world result: {err}"))
        })
    }

    #[wasm_bindgen(js_name = restore_world_to_tick)]
    pub fn restore_world_to_tick_js(
        &mut self,
        world_id: String,
        tick: f64,
    ) -> Result<JsValue, JsValue> {
        self.seek_world_to_tick_js(world_id, tick)
    }

    #[wasm_bindgen(js_name = rewind_world_by_ticks)]
    pub fn rewind_world_by_ticks_js(
        &mut self,
        world_id: String,
        tick_count: u32,
    ) -> Result<JsValue, JsValue> {
        let result =
            world_use_cases::rewind_world_by_ticks(&mut self.service, world_id, tick_count)
                .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize rewind world result: {err}"))
        })
    }
}
