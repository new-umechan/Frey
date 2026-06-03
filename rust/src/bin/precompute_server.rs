#[cfg(feature = "precompute_server")]
#[tokio::main]
async fn main() {
    if let Err(err) = frey_wasm::precompute_server::run_from_env().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "precompute_server"))]
fn main() {
    eprintln!("precompute_server requires --features precompute_server");
    std::process::exit(1);
}
