#[cfg(target_arch = "wasm32")]
pub(super) type ProfileClock = f64;
#[cfg(not(target_arch = "wasm32"))]
pub(super) type ProfileClock = std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_now() -> ProfileClock {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_now() -> ProfileClock {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    js_sys::Date::now() - start
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
