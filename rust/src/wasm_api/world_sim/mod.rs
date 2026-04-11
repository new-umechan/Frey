use wasm_bindgen::prelude::*;

mod api;

#[wasm_bindgen]
pub struct WorldSimController {
    service: crate::application::world_service::WorldService,
}

impl Default for WorldSimController {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WorldSimController {
        WorldSimController {
            service: crate::application::world_service::WorldService::new(),
        }
    }
}
