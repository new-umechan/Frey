mod commands;
mod common;
mod queries;
mod worlds;

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use serde::Deserialize;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::super::WorldSimController;

    #[derive(Deserialize)]
    struct InitResponse {
        world_id: String,
    }

    #[derive(Deserialize)]
    struct BudgetSummary {
        geology: u32,
        climate: u32,
        ecology: u32,
        civilization: u32,
    }

    #[derive(Deserialize)]
    struct MetricsResponse {
        tick: f64,
        budgets: BudgetSummary,
    }

    #[derive(Deserialize)]
    struct StepWorldProfiledResponse {
        steps: u32,
        step_feedback_ms: f64,
        step_geology_terrain_ms: f64,
        step_climate_ms: f64,
        step_geology_river_ms: f64,
        step_ecology_ms: f64,
        step_civilization_ms: f64,
        step_transition_ms: f64,
        step_sync_erosion_ms: f64,
        step_observe_world_change_ms: f64,
        step_history_snapshot_ms: f64,
    }

    #[derive(Deserialize)]
    struct HistoryTicksResponse {
        interval: u32,
        ticks: Vec<f64>,
    }

    #[derive(Deserialize)]
    struct RestoreWorldResponse {
        tick: f64,
    }

    #[derive(Deserialize)]
    struct CheckpointResult {
        snapshot_id: String,
    }

    #[derive(Deserialize)]
    struct CheckpointListEntry {
        snapshot_id: String,
        tick: f64,
    }

    #[derive(Deserialize)]
    struct CheckpointListResponse {
        checkpoints: Vec<CheckpointListEntry>,
    }

    #[wasm_bindgen_test]
    fn init_step_and_metrics_work() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-a".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");
        let world_id = init_data.world_id;

        controller
            .step_world_js(world_id.clone(), 3)
            .expect("step world");
        let metrics = controller.get_metrics_js(world_id).expect("get metrics");
        let metrics_data: MetricsResponse =
            serde_wasm_bindgen::from_value(metrics).expect("parse metrics");
        assert!(metrics_data.tick >= 1.0);
    }

    #[wasm_bindgen_test]
    fn step_world_profiled_returns_breakdown() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-profile".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");

        let profiled = controller
            .step_world_profiled_js(init_data.world_id, 1)
            .expect("step world profiled");
        let profiled_data: StepWorldProfiledResponse =
            serde_wasm_bindgen::from_value(profiled).expect("parse profiled");

        assert_eq!(profiled_data.steps, 1);
        assert!(profiled_data.step_feedback_ms >= 0.0);
        assert!(profiled_data.step_geology_terrain_ms >= 0.0);
        assert!(profiled_data.step_climate_ms >= 0.0);
        assert!(profiled_data.step_geology_river_ms >= 0.0);
        assert!(profiled_data.step_ecology_ms >= 0.0);
        assert!(profiled_data.step_civilization_ms >= 0.0);
        assert!(profiled_data.step_transition_ms >= 0.0);
        assert!(profiled_data.step_sync_erosion_ms >= 0.0);
        assert!(profiled_data.step_observe_world_change_ms >= 0.0);
        assert!(profiled_data.step_history_snapshot_ms >= 0.0);
    }

    #[wasm_bindgen_test]
    fn init_metrics_expose_crust_budgets_before_first_tick() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-b".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");

        let metrics = controller
            .get_metrics_js(init_data.world_id)
            .expect("get metrics");
        let metrics_data: MetricsResponse =
            serde_wasm_bindgen::from_value(metrics).expect("parse metrics");

        assert_eq!(metrics_data.tick, 0.0);
        assert_eq!(metrics_data.budgets.geology, 4);
        assert_eq!(metrics_data.budgets.climate, 0);
        assert_eq!(metrics_data.budgets.ecology, 0);
        assert_eq!(metrics_data.budgets.civilization, 0);
    }

    #[wasm_bindgen_test]
    fn history_ticks_and_restore_work() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-c".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");
        let world_id = init_data.world_id;

        controller
            .step_world_js(world_id.clone(), 80)
            .expect("step world");

        let history_ticks = controller
            .list_history_ticks_js(world_id.clone())
            .expect("list history ticks");
        let history_data: HistoryTicksResponse =
            serde_wasm_bindgen::from_value(history_ticks).expect("parse history ticks");
        assert_eq!(history_data.interval, 32);
        assert!(history_data.ticks.contains(&0.0));
        assert!(history_data.ticks.contains(&32.0));
        assert!(history_data.ticks.contains(&64.0));

        let restored = controller
            .restore_world_to_tick_js(world_id.clone(), 32.0)
            .expect("restore world");
        let restored_data: RestoreWorldResponse =
            serde_wasm_bindgen::from_value(restored).expect("parse restored world");
        assert_eq!(restored_data.tick, 32.0);

        let history_after_restore = controller
            .list_history_ticks_js(world_id.clone())
            .expect("list history ticks after restore");
        let history_after_restore_data: HistoryTicksResponse =
            serde_wasm_bindgen::from_value(history_after_restore)
                .expect("parse history ticks after restore");
        assert!(history_after_restore_data.ticks.contains(&64.0));

        let metrics = controller
            .get_metrics_js(world_id)
            .expect("get metrics after restore");
        let metrics_data: MetricsResponse =
            serde_wasm_bindgen::from_value(metrics).expect("parse metrics");
        assert_eq!(metrics_data.tick, 32.0);
    }

    #[wasm_bindgen_test]
    fn list_checkpoints_returns_saved_entries() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-d".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");

        controller
            .step_world_js(init_data.world_id.clone(), 8)
            .expect("step world");
        let saved = controller
            .save_checkpoint_js(init_data.world_id)
            .expect("save checkpoint");
        let saved_data: CheckpointResult =
            serde_wasm_bindgen::from_value(saved).expect("parse checkpoint result");

        let listed = controller.list_checkpoints_js().expect("list checkpoints");
        let listed_data: CheckpointListResponse =
            serde_wasm_bindgen::from_value(listed).expect("parse checkpoint list");

        assert!(listed_data
            .checkpoints
            .iter()
            .any(|entry| entry.snapshot_id == saved_data.snapshot_id && entry.tick >= 0.0));
    }
}
