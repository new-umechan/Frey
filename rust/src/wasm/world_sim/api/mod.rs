mod commands;
mod queries;
mod worlds;

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use serde::Deserialize;
    use wasm_bindgen::JsValue;

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

    #[test]
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

    #[test]
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
}
