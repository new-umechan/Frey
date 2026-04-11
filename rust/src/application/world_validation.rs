#![cfg(feature = "wasm_transport")]

pub(crate) fn validate_non_negative_tick(tick: f64) -> Result<u64, String> {
    if !tick.is_finite() || tick < 0.0 {
        return Err("tick must be a non-negative finite value".to_string());
    }
    Ok(tick.round() as u64)
}

pub(crate) fn validate_integer_tick(tick: f64, rounded: u64) -> Result<(), String> {
    if (tick - rounded as f64).abs() > f64::EPSILON {
        return Err("tick must be an integer value".to_string());
    }
    Ok(())
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
