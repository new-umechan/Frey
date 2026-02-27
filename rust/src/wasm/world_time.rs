use crate::sim::world;
use serde::Serialize;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WorldTimeController {
    inner: world::WorldTime,
    checkpoints: Vec<CheckpointState>,
    next_checkpoint_seq: u64,
    layers: HashMap<ControllerLayerKind, ControllerLayerData>,
}

#[derive(Clone)]
struct CheckpointState {
    id: String,
    time: world::WorldTime,
    layers: HashMap<ControllerLayerKind, ControllerLayerData>,
}

#[derive(Serialize)]
struct CheckpointSummary {
    id: String,
    tick: f64,
    era: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ControllerLayerKind {
    Climate,
    Ecology,
    Civilization,
}

#[derive(Clone)]
enum ControllerLayerData {
    Climate { temp: Vec<f32>, rain: Vec<f32> },
    Ecology {
        habitability: Vec<f32>,
        productivity: Vec<f32>,
    },
    Civilization { population: Vec<f32> },
}

impl ControllerLayerKind {
    fn for_era(era: world::EraKind) -> &'static [ControllerLayerKind] {
        const NONE: &[ControllerLayerKind] = &[];
        const CLIMATE: &[ControllerLayerKind] = &[ControllerLayerKind::Climate];
        const CLIMATE_ECOLOGY: &[ControllerLayerKind] =
            &[ControllerLayerKind::Climate, ControllerLayerKind::Ecology];
        const ALL: &[ControllerLayerKind] = &[
            ControllerLayerKind::Climate,
            ControllerLayerKind::Ecology,
            ControllerLayerKind::Civilization,
        ];
        match era {
            world::EraKind::Crust => NONE,
            world::EraKind::Environment => CLIMATE,
            world::EraKind::Life => CLIMATE_ECOLOGY,
            world::EraKind::Civilization => ALL,
            world::EraKind::History => ALL,
        }
    }
}

impl ControllerLayerData {
    fn for_kind(kind: ControllerLayerKind, time: &world::WorldTime) -> Self {
        match kind {
            ControllerLayerKind::Climate => Self::Climate {
                temp: vec![time.ema_climate_activity],
                rain: vec![time.ema_river_activity],
            },
            ControllerLayerKind::Ecology => Self::Ecology {
                habitability: vec![time.ema_ecology_activity],
                productivity: vec![
                    (time.ema_ecology_activity * 0.6 + time.ema_climate_activity * 0.4)
                        .clamp(0.0, 1.0),
                ],
            },
            ControllerLayerKind::Civilization => Self::Civilization {
                population: vec![time.ema_civilization_activity],
            },
        }
    }
}

#[wasm_bindgen]
impl WorldTimeController {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WorldTimeController {
        let mut controller = WorldTimeController {
            inner: world::WorldTime::new(),
            checkpoints: Vec::new(),
            next_checkpoint_seq: 1,
            layers: HashMap::new(),
        };
        controller.ensure_layers_for_current_era();
        controller
    }

    #[wasm_bindgen(js_name = reset)]
    pub fn reset_js(&mut self) {
        self.inner.reset();
        self.layers.clear();
        self.ensure_layers_for_current_era();
    }

    #[wasm_bindgen(js_name = step)]
    pub fn step_js(&mut self, ticks: u32) {
        self.inner.step(ticks);
    }

    #[wasm_bindgen(js_name = observeActivity)]
    pub fn observe_activity_js(
        &mut self,
        terrain: f32,
        river: f32,
        climate: f32,
        ecology: f32,
        civilization: f32,
    ) {
        self.inner
            .observe_activity(terrain, river, climate, ecology, civilization);
        self.ensure_layers_for_current_era();
    }

    #[wasm_bindgen(js_name = tick)]
    pub fn tick_js(&self) -> f64 {
        self.inner.tick as f64
    }

    #[wasm_bindgen(js_name = eraKey)]
    pub fn era_key_js(&self) -> String {
        self.inner.era.as_key().to_string()
    }

    #[wasm_bindgen(js_name = save_checkpoint)]
    pub fn save_checkpoint_js(&mut self) -> String {
        let id = format!("cp-{:06}", self.next_checkpoint_seq);
        self.next_checkpoint_seq = self.next_checkpoint_seq.saturating_add(1);
        self.checkpoints.push(CheckpointState {
            id: id.clone(),
            time: self.inner,
            layers: self.layers.clone(),
        });
        id
    }

    #[wasm_bindgen(js_name = load_checkpoint)]
    pub fn load_checkpoint_js(&mut self, id: String) -> Result<(), JsValue> {
        let Some(checkpoint) = self.checkpoints.iter().find(|c| c.id == id) else {
            return Err(JsValue::from_str(&format!("checkpoint not found: {id}")));
        };
        self.inner = checkpoint.time;
        self.layers = checkpoint.layers.clone();
        self.ensure_layers_for_current_era();
        Ok(())
    }

    #[wasm_bindgen(js_name = list_checkpoints)]
    pub fn list_checkpoints_js(&self) -> Result<JsValue, JsValue> {
        let summaries: Vec<CheckpointSummary> = self
            .checkpoints
            .iter()
            .map(|checkpoint| CheckpointSummary {
                id: checkpoint.id.clone(),
                tick: checkpoint.time.tick as f64,
                era: checkpoint.time.era.as_key().to_string(),
            })
            .collect();

        serde_wasm_bindgen::to_value(&summaries).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize checkpoint list: {err}"))
        })
    }

    #[wasm_bindgen(js_name = get_layer)]
    pub fn get_layer_js(&self, kind: String) -> Result<Vec<f32>, JsValue> {
        match kind.as_str() {
            "climate.temp" => {
                let Some(ControllerLayerData::Climate { temp, .. }) =
                    self.layers.get(&ControllerLayerKind::Climate)
                else {
                    return Err(JsValue::from_str("layer is not generated yet: climate.temp"));
                };
                Ok(temp.clone())
            }
            "climate.rain" => {
                let Some(ControllerLayerData::Climate { rain, .. }) =
                    self.layers.get(&ControllerLayerKind::Climate)
                else {
                    return Err(JsValue::from_str("layer is not generated yet: climate.rain"));
                };
                Ok(rain.clone())
            }
            "ecology.habitability" => {
                let Some(ControllerLayerData::Ecology { habitability, .. }) =
                    self.layers.get(&ControllerLayerKind::Ecology)
                else {
                    return Err(JsValue::from_str(
                        "layer is not generated yet: ecology.habitability",
                    ));
                };
                Ok(habitability.clone())
            }
            "ecology.productivity" => {
                let Some(ControllerLayerData::Ecology { productivity, .. }) =
                    self.layers.get(&ControllerLayerKind::Ecology)
                else {
                    return Err(JsValue::from_str(
                        "layer is not generated yet: ecology.productivity",
                    ));
                };
                Ok(productivity.clone())
            }
            "civilization.population" => {
                let Some(ControllerLayerData::Civilization { population }) =
                    self.layers.get(&ControllerLayerKind::Civilization)
                else {
                    return Err(JsValue::from_str(
                        "layer is not generated yet: civilization.population",
                    ));
                };
                Ok(population.clone())
            }
            _ => Err(JsValue::from_str(&format!("invalid layer kind: {kind}"))),
        }
    }
}

impl WorldTimeController {
    fn ensure_layers_for_current_era(&mut self) {
        for kind in ControllerLayerKind::for_era(self.inner.era) {
            self.layers
                .entry(*kind)
                .or_insert_with(|| ControllerLayerData::for_kind(*kind, &self.inner));
        }
    }
}
