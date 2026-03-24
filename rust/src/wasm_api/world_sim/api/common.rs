use wasm_bindgen::JsValue;

use super::super::state::HISTORY_SNAPSHOT_INTERVAL;

pub(super) fn world_not_found_error(world_id: &str) -> JsValue {
    JsValue::from_str(&format!("world not found: {world_id}"))
}

pub(super) fn validate_non_negative_tick(tick: f64) -> Result<u64, JsValue> {
    if !tick.is_finite() || tick < 0.0 {
        return Err(JsValue::from_str(
            "tick must be a non-negative finite value",
        ));
    }
    Ok(tick.round() as u64)
}

pub(super) fn validate_integer_tick(tick: f64, rounded: u64) -> Result<(), JsValue> {
    if (tick - rounded as f64).abs() > f64::EPSILON {
        return Err(JsValue::from_str("tick must be an integer value"));
    }
    Ok(())
}

pub(super) fn validate_checkpoint_tick(tick: u64) -> Result<(), JsValue> {
    if tick % HISTORY_SNAPSHOT_INTERVAL != 0 {
        return Err(JsValue::from_str(&format!(
            "tick {tick} is not checkpointed; available ticks are saved every {HISTORY_SNAPSHOT_INTERVAL} ticks"
        )));
    }
    Ok(())
}

pub(super) fn history_tick_not_available_error(tick: u64) -> JsValue {
    JsValue::from_str(&format!("tick {tick} is not available in history"))
}

#[cfg(test)]
mod tests {
    use super::{validate_integer_tick, validate_non_negative_tick};

    #[test]
    fn validate_integer_tick_accepts_exact_integer() {
        let rounded = validate_non_negative_tick(32.0).expect("tick should be valid");
        validate_integer_tick(32.0, rounded).expect("integer tick should pass");
    }
}
