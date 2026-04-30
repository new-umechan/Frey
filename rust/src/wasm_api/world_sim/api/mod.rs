mod commands;
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
        simulation_rate: Option<f32>,
        budgets: BudgetSummary,
    }

    #[derive(Deserialize)]
    struct FieldResponse {
        field_kind: String,
        f32_data: Option<Vec<f32>>,
    }

    #[derive(Deserialize)]
    struct StepWorldProfiledResponse {
        steps: u32,
        exec_feedback_ms: f64,
        exec_geology_terrain_ms: f64,
        exec_climate_ms: f64,
        exec_glaciology_ms: f64,
        exec_hydrology_ms: f64,
        exec_ecology_ms: f64,
        exec_society_ms: f64,
        exec_transition_ms: f64,
        step_sync_erosion_ms: f64,
        step_observe_world_change_ms: f64,
        step_history_snapshot_ms: f64,
    }

    #[derive(Deserialize)]
    struct ExecWorldSliceResponse {
        processed_ticks: u32,
        busy: bool,
        phase: String,
        tick: f64,
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
    struct ScientificBenchmarkSamplesResponse {
        sample_count: u32,
        samples: Vec<ScientificBenchmarkSample>,
    }

    #[derive(Deserialize)]
    struct ScientificBenchmarkSample {
        tick: f64,
        era: String,
    }

    #[derive(Deserialize)]
    struct ForkWorldResponse {
        world_id: String,
        tick: f64,
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
            .exec_world_js(world_id.clone(), 3)
            .expect("step world");
        let metrics = controller.get_metrics_js(world_id).expect("get metrics");
        let metrics_data: MetricsResponse =
            serde_wasm_bindgen::from_value(metrics).expect("parse metrics");
        assert!(metrics_data.tick >= 1.0);
    }

    #[wasm_bindgen_test]
    fn get_field_lake_depth_is_available() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-lake-depth".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");

        let field = controller
            .get_field_js(init_data.world_id, "lake_depth".to_string(), 1)
            .expect("get lake_depth field");
        let field_data: FieldResponse =
            serde_wasm_bindgen::from_value(field).expect("parse field response");

        assert_eq!(field_data.field_kind, "lake_depth");
        assert!(field_data
            .f32_data
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false));
    }

    #[wasm_bindgen_test]
    fn exec_world_profiled_returns_breakdown() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-profile".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");

        let profiled = controller
            .exec_world_profiled_js(init_data.world_id, 1)
            .expect("step world profiled");
        let profiled_data: StepWorldProfiledResponse =
            serde_wasm_bindgen::from_value(profiled).expect("parse profiled");

        assert_eq!(profiled_data.steps, 1);
        assert!(profiled_data.exec_feedback_ms >= 0.0);
        assert!(profiled_data.exec_geology_terrain_ms >= 0.0);
        assert!(profiled_data.exec_climate_ms >= 0.0);
        assert!(profiled_data.exec_glaciology_ms >= 0.0);
        assert!(profiled_data.exec_hydrology_ms >= 0.0);
        assert!(profiled_data.exec_ecology_ms >= 0.0);
        assert!(profiled_data.exec_society_ms >= 0.0);
        assert!(profiled_data.exec_transition_ms >= 0.0);
        assert!(profiled_data.step_sync_erosion_ms >= 0.0);
        assert!(profiled_data.step_observe_world_change_ms >= 0.0);
        assert!(profiled_data.step_history_snapshot_ms >= 0.0);
    }

    #[wasm_bindgen_test]
    fn exec_world_slice_completes_tick_without_exposing_partial_tick_count() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-slice".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");

        let first = controller
            .exec_world_slice_js(init_data.world_id.clone(), 1)
            .expect("run slice");
        let first_data: ExecWorldSliceResponse =
            serde_wasm_bindgen::from_value(first).expect("parse slice response");
        assert_eq!(first_data.processed_ticks, 0);
        assert!(first_data.busy);
        assert!(first_data.tick >= 0.0);

        let mut last = first_data;
        while last.busy {
            let next = controller
                .exec_world_slice_js(init_data.world_id.clone(), 1)
                .expect("run next slice");
            last = serde_wasm_bindgen::from_value(next).expect("parse next slice");
        }

        assert_eq!(last.processed_ticks, 1);
        assert_eq!(last.tick, 1.0);
        assert!(!last.phase.is_empty());
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
            .exec_world_js(world_id.clone(), 80)
            .expect("step world");

        let history_ticks = controller
            .list_history_ticks_js(world_id.clone())
            .expect("list history ticks");
        let history_data: HistoryTicksResponse =
            serde_wasm_bindgen::from_value(history_ticks).expect("parse history ticks");
        assert_eq!(history_data.interval, 64);
        assert!(history_data.ticks.contains(&0.0));
        assert!(history_data.ticks.contains(&64.0));

        let restored = controller
            .restore_world_to_tick_js(world_id.clone(), 65.0)
            .expect("restore world");
        let restored_data: RestoreWorldResponse =
            serde_wasm_bindgen::from_value(restored).expect("parse restored world");
        assert_eq!(restored_data.tick, 65.0);

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
        assert_eq!(metrics_data.tick, 65.0);
    }

    #[wasm_bindgen_test]
    fn long_exec_keeps_world_observable() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-long".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");
        let world_id = init_data.world_id;

        controller
            .exec_world_js(world_id.clone(), 96)
            .expect("long step world");

        let metrics = controller
            .get_metrics_js(world_id.clone())
            .expect("get metrics after long exec");
        let metrics_data: MetricsResponse =
            serde_wasm_bindgen::from_value(metrics).expect("parse metrics after long exec");
        assert!(metrics_data.tick.is_finite());
        assert!(metrics_data.tick >= 96.0);

        let history_ticks = controller
            .list_history_ticks_js(world_id)
            .expect("list history ticks");
        let history_data: HistoryTicksResponse =
            serde_wasm_bindgen::from_value(history_ticks).expect("parse history ticks");
        assert!(history_data.ticks.contains(&64.0));
        assert!(history_data.ticks.iter().all(|tick| tick.is_finite()));
    }

    #[wasm_bindgen_test]
    fn interventions_and_fork_replay_from_checkpoint() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-intervention".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");
        let world_id = init_data.world_id;

        controller
            .set_simulation_rate_js(world_id.clone(), 4.0)
            .expect("set simulation rate intervention");
        controller
            .exec_world_js(world_id.clone(), 80)
            .expect("step world to create checkpoints");
        controller
            .restore_world_to_tick_js(world_id.clone(), 0.0)
            .expect("restore to tick 0");

        let metrics = controller
            .get_metrics_js(world_id.clone())
            .expect("metrics after restore");
        let metrics_data: MetricsResponse =
            serde_wasm_bindgen::from_value(metrics).expect("parse metrics");
        assert_eq!(metrics_data.tick, 0.0);
        assert_eq!(metrics_data.simulation_rate.unwrap_or(0.0), 4.0);

        let forked = controller
            .fork_world_js(world_id, 0.0)
            .expect("fork world at tick 0");
        let forked_data: ForkWorldResponse =
            serde_wasm_bindgen::from_value(forked).expect("parse fork world");
        assert_eq!(forked_data.tick, 0.0);
        assert!(!forked_data.world_id.is_empty());
    }

    #[wasm_bindgen_test]
    fn scientific_benchmark_samples_are_queryable() {
        let mut controller = WorldSimController::new();
        let config = serde_wasm_bindgen::to_value(&serde_json::json!({
            "verification_mode": "scientific_benchmark"
        }))
        .expect("serialize init config");
        let init = controller
            .init_world_js("seed-science-query".to_string(), 1, config)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");
        let world_id = init_data.world_id;

        controller
            .exec_world_js(world_id.clone(), 4)
            .expect("exec world");
        let samples = controller
            .get_scientific_benchmark_samples_js(world_id)
            .expect("get scientific benchmark samples");
        let data: ScientificBenchmarkSamplesResponse =
            serde_wasm_bindgen::from_value(samples).expect("parse samples");
        assert!(data.sample_count >= 1);
        assert!(!data.samples.is_empty());
        assert!(data.samples[0].tick >= 1.0);
        assert!(!data.samples[0].era.is_empty());
    }
}
