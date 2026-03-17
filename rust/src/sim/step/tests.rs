    use crate::TerrainParams;

    use super::*;
    use crate::sim::world::{GeologyState, World, WorldMesh};

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                normalize3([0.0, 0.8, 0.6]),
                normalize3([0.7, 0.2, 0.6]),
                normalize3([0.4, -0.7, 0.6]),
                normalize3([-0.6, -0.1, 0.8]),
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let geology = GeologyState {
            height: vec![0.45, 0.15, -0.25, 0.05],
            plate_id: vec![0, 0, 1, 1],
            river_flux: vec![0.1, 0.2, 0.3, 0.1],
            river_next: vec![1, 2, -1, 2],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            boundary_condition: vec![0.0; 4],
        };
        World::new(mesh, geology)
    }

    #[test]
    fn step_world_advances_tick_and_sets_budget_to_one() {
        let mut world = build_test_world();
        world.exec.era = EraKind::History;
        step_world(&mut world);
        assert_eq!(world.exec.tick, 1);
        assert_eq!(world.exec.budgets.geology, 1);
        assert_eq!(world.exec.budgets.climate, 1);
        assert_eq!(world.exec.budgets.ecology, 1);
        assert_eq!(world.exec.budgets.civilization, 4);
    }

    #[test]
    fn river_fallback_routes_flux_downhill() {
        let mut world = build_test_world();
        world.exec.river_erosion_state = None;
        river::run_river_step(&mut world, 1);
        assert_eq!(world.state.geology.river_next.len(), 4);
        assert!(world
            .state
            .geology
            .river_flux
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0));
    }

    #[test]
    fn terrain_step_initializes_dynamics_and_updates_boundary_signal() {
        let mut world = build_test_world();
        let params = TerrainParams::default();
        world.exec.river_erosion_state = Some(crate::ErosionAutomatonState {
            positions: world.mesh.positions.clone(),
            nbr_offsets: world.mesh.nbr_offsets.clone(),
            nbrs: world.mesh.nbrs.clone(),
            height: world.state.geology.height.clone(),
            water: vec![0.0; 4],
            sediment: vec![0.0; 4],
            armor: vec![0.0; 4],
            rain: vec![0.1; 4],
            river_flux: world.state.geology.river_flux.clone(),
            river_next: world.state.geology.river_next.clone(),
            active_queue: vec![0, 1, 2, 3],
            active_head: 0,
            in_queue: vec![1; 4],
            rain_cursor: 0,
            tick: 0,
            last_rebuild_tick: 0,
            last_sink_full_rebuild_tick: 0,
            flux_scale_ema: 1.0,
            last_river_driver: 1.0,
            prev_river_next: world.state.geology.river_next.clone(),
            flow_heading: vec![[0.0, 0.0, 0.0]; 4],
            groundwater_storage: vec![0.0; 4],
            scratch_effective_runoff: vec![0.0; 4],
            scratch_changed_mark: vec![0; 4],
            scratch_flux_samples: Vec::with_capacity(2),
            recent_changed: Vec::new(),
            sink_id: vec![-1; 4],
            sink_route_next: vec![-1; 4],
            sink_spill_cell: Vec::new(),
            sink_spill_to: Vec::new(),
            sink_capacity_total: Vec::new(),
            sink_capacity_remaining: Vec::new(),
            sink_storage_sediment: Vec::new(),
            sink_spill_level: Vec::new(),
            sink_overflow_active: Vec::new(),
            sink_dirty: vec![1; 4],
            params,
        });

        terrain::run_terrain_step(&mut world);

        assert!(world.exec.terrain_dynamics.is_some());
        assert_eq!(world.state.geology.boundary_condition.len(), 4);
    }

    #[test]
    fn route_river_flux_emphasizes_upstream_accumulation() {
        let height = vec![0.6, 0.4, 0.2];
        let river_next = vec![1, 2, -1];
        let rain = vec![0.2, 0.2, 0.2];
        let flux = river::route_river_flux(&height, &river_next, &rain);
        assert_eq!(flux.len(), 3);
        assert!(flux[2] > flux[1]);
        assert_eq!(flux[0], 0.0);
    }

    #[test]
    fn river_fallback_applies_threshold_and_clears_ocean_next() {
        let mut world = build_test_world();
        world.exec.era = EraKind::Environment;
        world.exec.river_erosion_state = None;
        world.state.climate.runoff = vec![10.0; 4];

        river::run_river_step(&mut world, 1);

        assert_eq!(world.state.geology.river_next[2], -1);
        assert_eq!(world.state.geology.river_flux[0], 0.0);
        assert_eq!(world.state.geology.river_flux[1], 0.0);
        assert_eq!(world.state.geology.river_flux[3], 0.0);
    }
