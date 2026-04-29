use std::collections::HashSet;

use js_sys::{Float32Array, Int32Array, Object, Uint32Array};
use wasm_bindgen::prelude::*;

use crate::application::world_dto::WorldDeltaQuery;
use crate::application::{world_causal_exploration_use_cases, world_query_use_cases};

use super::super::WorldSimController;

fn vec_f32_to_js(values: &[f32]) -> JsValue {
    Float32Array::from(values).into()
}

fn vec_u32_to_js(values: &[u32]) -> JsValue {
    Uint32Array::from(values).into()
}

fn vec_i32_to_js(values: &[i32]) -> JsValue {
    Int32Array::from(values).into()
}

fn serialize_field_delta_to_js(
    field_kind: &str,
    mode: &str,
    ranges: &[(u32, u32)],
    dirty_bitmap: &Option<Vec<u32>>,
    f32_data: &Option<Vec<f32>>,
    u32_data: &Option<Vec<u32>>,
    i32_data: &Option<Vec<i32>>,
) -> JsValue {
    let result = Object::new();
    let _ = js_sys::Reflect::set(
        &result,
        &JsValue::from_str("field_kind"),
        &JsValue::from_str(field_kind),
    );
    let _ = js_sys::Reflect::set(
        &result,
        &JsValue::from_str("mode"),
        &JsValue::from_str(mode),
    );

    let ranges_js = ranges
        .iter()
        .map(|&(start, end)| {
            let obj = Object::new();
            let _ = js_sys::Reflect::set(
                &obj,
                &JsValue::from_str("start"),
                &JsValue::from_f64(start as f64),
            );
            let _ = js_sys::Reflect::set(
                &obj,
                &JsValue::from_str("end"),
                &JsValue::from_f64(end as f64),
            );
            JsValue::from(obj)
        })
        .collect::<js_sys::Array>();
    let _ = js_sys::Reflect::set(&result, &JsValue::from_str("ranges"), &ranges_js);

    if let Some(bitmap) = dirty_bitmap {
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("dirty_bitmap"),
            &vec_u32_to_js(bitmap),
        );
    } else {
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("dirty_bitmap"),
            &JsValue::null(),
        );
    }

    if let Some(data) = f32_data {
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("f32_data"),
            &vec_f32_to_js(data),
        );
    } else {
        let _ = js_sys::Reflect::set(&result, &JsValue::from_str("f32_data"), &JsValue::null());
    }

    if let Some(data) = u32_data {
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("u32_data"),
            &vec_u32_to_js(data),
        );
    } else {
        let _ = js_sys::Reflect::set(&result, &JsValue::from_str("u32_data"), &JsValue::null());
    }

    if let Some(data) = i32_data {
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("i32_data"),
            &vec_i32_to_js(data),
        );
    } else {
        let _ = js_sys::Reflect::set(&result, &JsValue::from_str("i32_data"), &JsValue::null());
    }

    result.into()
}

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

        let result = Object::new();
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("field_kind"),
            &JsValue::from_str(&response.field_kind),
        );
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("stride"),
            &JsValue::from_f64(response.stride as f64),
        );
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("cell_count"),
            &JsValue::from_f64(response.cell_count as f64),
        );
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("sampled_count"),
            &JsValue::from_f64(response.sampled_count as f64),
        );

        if let Some(data) = response.f32_data {
            let _ = js_sys::Reflect::set(
                &result,
                &JsValue::from_str("f32_data"),
                &vec_f32_to_js(&data),
            );
        } else {
            let _ = js_sys::Reflect::set(&result, &JsValue::from_str("f32_data"), &JsValue::null());
        }

        if let Some(data) = response.u32_data {
            let _ = js_sys::Reflect::set(
                &result,
                &JsValue::from_str("u32_data"),
                &vec_u32_to_js(&data),
            );
        } else {
            let _ = js_sys::Reflect::set(&result, &JsValue::from_str("u32_data"), &JsValue::null());
        }

        if let Some(data) = response.i32_data {
            let _ = js_sys::Reflect::set(
                &result,
                &JsValue::from_str("i32_data"),
                &vec_i32_to_js(&data),
            );
        } else {
            let _ = js_sys::Reflect::set(&result, &JsValue::from_str("i32_data"), &JsValue::null());
        }

        Ok(result.into())
    }

    #[wasm_bindgen(js_name = get_metrics)]
    pub fn get_metrics_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let response = world_query_use_cases::get_metrics(&self.service, world_id)
            .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize metrics: {err}")))
    }

    #[wasm_bindgen(js_name = get_scientific_benchmark_samples)]
    pub fn get_scientific_benchmark_samples_js(
        &self,
        world_id: String,
    ) -> Result<JsValue, JsValue> {
        let response =
            world_query_use_cases::get_scientific_benchmark_samples(&self.service, world_id)
                .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize scientific benchmark samples: {err}"
            ))
        })
    }

    #[wasm_bindgen(js_name = get_causal_exploration_demo)]
    pub fn get_causal_exploration_demo_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let response = world_causal_exploration_use_cases::get_causal_exploration_demo(
            &self.service,
            &world_id,
        )
        .map_err(|err| JsValue::from_str(&err))?;
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize causal exploration demo response: {err}"
            ))
        })
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

        let deltas_js = response
            .deltas
            .iter()
            .map(|delta| {
                serialize_field_delta_to_js(
                    &delta.field_kind,
                    &delta.mode,
                    &delta
                        .ranges
                        .iter()
                        .map(|r| (r.start, r.end))
                        .collect::<Vec<_>>(),
                    &delta.dirty_bitmap,
                    &delta.f32_data,
                    &delta.u32_data,
                    &delta.i32_data,
                )
            })
            .collect::<js_sys::Array>();

        let budgets_js = serde_wasm_bindgen::to_value(&response.budgets).unwrap_or(JsValue::null());

        let result = Object::new();
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("world_id"),
            &JsValue::from_str(&response.world_id),
        );
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("tick"),
            &JsValue::from_f64(response.tick),
        );
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("era"),
            &JsValue::from_str(&response.era),
        );
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("real_years_per_tick"),
            &JsValue::from_f64(response.real_years_per_tick as f64),
        );
        let _ = js_sys::Reflect::set(
            &result,
            &JsValue::from_str("runtime_tick_ms"),
            &JsValue::from_f64(response.runtime_tick_ms as f64),
        );
        let _ = js_sys::Reflect::set(&result, &JsValue::from_str("budgets"), &budgets_js);
        let _ = js_sys::Reflect::set(&result, &JsValue::from_str("deltas"), &deltas_js);

        Ok(result.into())
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
