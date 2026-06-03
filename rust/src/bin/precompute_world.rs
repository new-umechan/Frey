#[cfg(feature = "precompute_server")]
fn main() {
    if let Err(err) = frey_wasm::precompute_server::run_precompute_world_from_env() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "precompute_server"))]
fn main() {
    eprintln!("precompute_world requires --features precompute_server");
    std::process::exit(1);
}
