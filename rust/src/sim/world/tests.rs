use super::{
    CellId, EntityState, EraKind, FeedbackQueue, GeologyState, PolityComponent, PolityId,
    RegionComponent, RegionId, SettlementComponent, SettlementId, World, WorldMesh,
};
use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::{exec_world, exec_world_with_feedback_and_hydrology};
use crate::GeologyParams;
use crate::PlateId;

const EPSILON: f32 = 1e-5;

fn build_world() -> World {
    World::new(
        WorldMesh {
            positions: vec![[0.0, 0.0, 1.0]; 4],
            nbr_offsets: vec![0, 1, 2, 3, 4],
            nbrs: vec![1, 2, 3, 0],
        },
        GeologyState {
            height: vec![0.2, -0.1, 0.1, -0.2],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
        },
    )
}

fn build_generated_world(seed: &str, params: GeologyParams) -> World {
    let level = params.level;
    let terrain = crate::sim::build_geology(seed, params);
    let (positions, indices) = generate_icosphere(level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let plate_id = terrain.plate_id;
    let mut world = World::new(
        WorldMesh {
            positions,
            nbr_offsets,
            nbrs,
        },
        GeologyState {
            height: terrain.height,
            lake_depth: terrain.lake_depth,
            plate_id,
            erosion_rate: vec![0.0; terrain.river_flux.len()],
            deposition_rate: vec![0.0; terrain.river_flux.len()],
            volcanism: vec![0.0; terrain.river_flux.len()],
            vertex_buoyancy: vec![0.0; terrain.river_flux.len()],
            geology_internal: vec![
                crate::sim::geology_types::GeologyInternal::default();
                terrain.river_flux.len()
            ],
            boundary_condition: vec![0.0; terrain.river_flux.len()],
        },
    );
    world.state.hydrology.river_flow = terrain.river_flux;
    world.state.hydrology.river_next = terrain.river_next;
    world
}

fn assert_vec_f32_close(name: &str, lhs: &[f32], rhs: &[f32], eps: f32) {
    assert_eq!(lhs.len(), rhs.len(), "{name} length mismatch");
    for (i, (&a, &b)) in lhs.iter().zip(rhs.iter()).enumerate() {
        assert!(
            (a - b).abs() <= eps,
            "{name}[{i}] mismatch: lhs={a}, rhs={b}, eps={eps}"
        );
    }
}

fn assert_geology_runtime_close(lhs: &World, rhs: &World, eps: f32) {
    assert_eq!(lhs.state.geology.plate_id, rhs.state.geology.plate_id);
    assert_eq!(
        lhs.state.hydrology.river_next,
        rhs.state.hydrology.river_next
    );
    assert_vec_f32_close(
        "state.geology.height",
        &lhs.state.geology.height,
        &rhs.state.geology.height,
        eps,
    );
    assert_vec_f32_close(
        "state.hydrology.river_flow",
        &lhs.state.hydrology.river_flow,
        &rhs.state.hydrology.river_flow,
        eps,
    );
    assert_vec_f32_close(
        "state.geology.volcanism",
        &lhs.state.geology.volcanism,
        &rhs.state.geology.volcanism,
        eps,
    );
    assert_vec_f32_close(
        "state.geology.vertex_buoyancy",
        &lhs.state.geology.vertex_buoyancy,
        &rhs.state.geology.vertex_buoyancy,
        eps,
    );

    let lhs_runtime = lhs
        .matched_geology_dynamics()
        .expect("lhs geology runtime is missing");
    let rhs_runtime = rhs
        .matched_geology_dynamics()
        .expect("rhs geology runtime is missing");

    assert_eq!(
        lhs_runtime.vertex_states.len(),
        rhs_runtime.vertex_states.len()
    );
    assert_eq!(
        lhs_runtime.boundary_state.dominant_type,
        rhs_runtime.boundary_state.dominant_type
    );
    assert_eq!(
        lhs_runtime.boundary_state.edge_pairs,
        rhs_runtime.boundary_state.edge_pairs
    );

    for (i, (a, b)) in lhs_runtime
        .vertex_states
        .iter()
        .zip(rhs_runtime.vertex_states.iter())
        .enumerate()
    {
        assert_eq!(
            a.crust_type, b.crust_type,
            "vertex_states[{i}].crust_type mismatch"
        );
        assert_vec_f32_close(
            &format!("vertex_states[{i}]"),
            &[
                a.thickness,
                a.density,
                a.age,
                a.stress,
                a.temperature,
                a.rigidity,
                a.arc_volcanism,
                a.ridge_volcanism,
                a.hotspot_volcanism,
                a.backarc_volcanism,
                a.stress_tensor.xx,
                a.stress_tensor.yy,
                a.stress_tensor.xy,
            ],
            &[
                b.thickness,
                b.density,
                b.age,
                b.stress,
                b.temperature,
                b.rigidity,
                b.arc_volcanism,
                b.ridge_volcanism,
                b.hotspot_volcanism,
                b.backarc_volcanism,
                b.stress_tensor.xx,
                b.stress_tensor.yy,
                b.stress_tensor.xy,
            ],
            eps,
        );
    }

    assert_vec_f32_close(
        "runtime.mantle_heat",
        &lhs_runtime.mantle_heat,
        &rhs_runtime.mantle_heat,
        eps,
    );
    assert_vec_f32_close(
        "runtime.boundary_state.activity",
        &lhs_runtime.boundary_state.activity,
        &rhs_runtime.boundary_state.activity,
        eps,
    );
    assert_vec_f32_close(
        "runtime.boundary_state.rollback_fraction",
        &lhs_runtime.boundary_state.rollback_fraction,
        &rhs_runtime.boundary_state.rollback_fraction,
        eps,
    );
    assert_vec_f32_close(
        "runtime.boundary_state.slab_convergence_component",
        &lhs_runtime.boundary_state.slab_convergence_component,
        &rhs_runtime.boundary_state.slab_convergence_component,
        eps,
    );
    assert_vec_f32_close(
        "runtime.boundary_state.slab_rollback_component",
        &lhs_runtime.boundary_state.slab_rollback_component,
        &rhs_runtime.boundary_state.slab_rollback_component,
        eps,
    );
    assert_eq!(
        lhs_runtime.boundary_state.edge_internal.len(),
        rhs_runtime.boundary_state.edge_internal.len()
    );
    for (i, (a, b)) in lhs_runtime
        .boundary_state
        .edge_internal
        .iter()
        .zip(rhs_runtime.boundary_state.edge_internal.iter())
        .enumerate()
    {
        assert!(
            (a.convergence_memory - b.convergence_memory).abs() <= eps,
            "edge_internal[{i}].convergence_memory mismatch: lhs={}, rhs={}",
            a.convergence_memory,
            b.convergence_memory
        );
    }
}

#[test]
fn world_initializes_exec_state() {
    let world = build_world();
    assert_eq!(world.clock.epoch, EraKind::Crust);
    assert_eq!(
        world.clock.real_years_per_tick,
        EraKind::Crust.real_years_per_tick()
    );
    assert_eq!(world.clock.budgets, EraKind::Crust.budgets());
    assert_eq!(world.clock.transition.last_land_ratio, 0.5);
    assert!(world.matched_geology_dynamics().is_none());
    assert!(world.polity_relations.is_empty());
}

#[test]
fn entity_state_round_trips_through_serde() {
    let entities = EntityState::from_components(
        vec![
            PolityComponent {
                polity_id: PolityId(1),
                capital_cell: CellId(3),
                legitimacy: 0.5,
                centralization: 0.4,
                military_tech: 0.2,
                cells_cache: vec![CellId(3)],
            },
            PolityComponent {
                polity_id: PolityId(2),
                capital_cell: CellId(8),
                legitimacy: 0.8,
                centralization: 0.6,
                military_tech: 0.4,
                cells_cache: vec![CellId(8), CellId(9)],
            },
        ],
        vec![SettlementComponent {
            settlement_id: SettlementId(4),
            cell: CellId(5),
        }],
        vec![RegionComponent {
            region_id: RegionId(7),
            cells: vec![CellId(1), CellId(2)],
        }],
    );

    let json = serde_json::to_string(&entities).expect("serialize entity store");
    let restored: EntityState = serde_json::from_str(&json).expect("deserialize entity store");
    assert_eq!(restored, entities);
    assert!(restored.validate().is_ok());
}

#[test]
fn world_initializes_land_ratio_independently_from_sea_ratio() {
    let world = World::new(
        WorldMesh {
            positions: vec![[0.0, 0.0, 1.0]; 4],
            nbr_offsets: vec![0, 1, 2, 3, 4],
            nbrs: vec![1, 2, 3, 0],
        },
        GeologyState {
            height: vec![0.3, 0.1, 0.2, -0.4],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
        },
    );

    assert_eq!(world.control.target_sea_ratio, 0.25);
    assert_eq!(world.clock.transition.last_land_ratio, 0.75);
}

#[test]
fn refresh_terrain_state_reclassifies_cells_with_sea_level_offset() {
    let mut world = World::new(
        WorldMesh {
            positions: vec![[0.0, 0.0, 1.0]; 4],
            nbr_offsets: vec![0, 1, 2, 3, 4],
            nbrs: vec![1, 2, 3, 0],
        },
        GeologyState {
            height: vec![0.18, 0.12, 0.08, -0.2],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
        },
    );

    world.control.sea_level_offset = 0.10;
    world.refresh_terrain_state();

    assert_eq!(world.coastal_flags(), [false, true, false, true]);
    let distance_from_ocean = world.distance_from_ocean_values();
    assert!(distance_from_ocean[0].is_finite());
    assert!(distance_from_ocean[1].is_finite());
    assert_eq!(distance_from_ocean[2], 0.0);
    assert_eq!(distance_from_ocean[3], 0.0);
}

#[test]
fn feedback_queue_sizes_match_world() {
    let queue = FeedbackQueue::new(8);
    assert!(queue.entries.is_empty());
}

#[test]
fn feedback_queue_pushes_entries() {
    let mut queue = FeedbackQueue::new(3);
    queue.push(crate::sim::world::FeedbackEntry {
        source: crate::sim::world::ModuleId::Exec,
        target_module: crate::sim::world::ModuleId::Exec,
        target_ref: crate::sim::world::TargetRef::Global,
        enqueued_tick: 0,
        payload: crate::sim::world::FeedbackPayload::TriggerEpochTransition {
            to: EraKind::History,
        },
    });
    assert_eq!(queue.entries.len(), 1);
}

#[test]
fn civilization_indicators_aggregate_population_and_polity() {
    let mut world = build_world();
    world.state.population.population = vec![12.0, 5.0, 11.0, 0.0];
    world.state.polity.polity_id = vec![Some(PolityId(1)), None, Some(PolityId(2)), None];

    let indicators = world.state.civilization_state().indicators();
    assert_eq!(indicators.settled_cells, 2);
    assert!((indicators.total_population - 28.0).abs() < 1e-6);
    assert_eq!(indicators.state_cells, 2);
}

#[test]
fn cell_store_view_exposes_geo_geology_climate_hydrology() {
    let world = build_world();
    let store = world.cell_store();

    assert_eq!(store.height.len(), 4);
    assert_eq!(store.plate_id.len(), 4);
    assert_eq!(store.temperature.len(), 4);
    assert_eq!(store.river_flow.len(), 4);
    assert_eq!(store.neighbors_offsets.len(), 5);
}

#[test]
fn cell_store_mut_updates_underlying_world_state() {
    let mut world = build_world();
    {
        let store = world.cell_store_mut();
        store.height[0] = 2.5;
        store.temperature[0] = -8.0;
        store.river_flow[0] = 42.0;
    }

    assert_eq!(world.state.geology.height[0], 2.5);
    assert_eq!(world.state.climate.temperature[0], -8.0);
    assert_eq!(world.state.hydrology.river_flow[0], 42.0);
}

#[test]
fn id_newtypes_round_trip_scalar_values() {
    let cell = CellId(17);
    let plate = PlateId(9);

    assert_eq!(cell.as_usize(), 17usize);
    assert_eq!(plate.as_u32(), 9u32);
}

#[test]
fn metrics_collects_height_and_flux_stats() {
    let mut world = World::new(
        WorldMesh {
            positions: vec![[0.0, 0.0, 1.0]; 4],
            nbr_offsets: vec![0, 3, 5, 7, 8],
            nbrs: vec![1, 2, 3, 0, 2, 0, 1, 0],
        },
        GeologyState {
            height: vec![1.0, -1.0, 2.0, -2.0],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
        },
    );
    world.state.hydrology.river_flow = vec![0.5, 1.2, 3.0, 0.1];
    world.state.hydrology.river_next = vec![1, 2, -1, 0];

    let metrics = world.metrics();
    assert_eq!(metrics.cell_count, 4);
    assert_eq!(metrics.land_cells, 2);
    assert!((metrics.land_ratio - 0.5).abs() < 1e-6);
    assert!((metrics.mean_height - 0.0).abs() < 1e-6);
    assert!((metrics.height_std_dev - 1.5811388).abs() < 1e-5);
    assert!((metrics.mean_river_flux - 1.2).abs() < 1e-6);
    assert!((metrics.max_river_flux - 3.0).abs() < 1e-6);
    assert!((metrics.top10_river_flux_sum - 4.8).abs() < 1e-6);
    assert_eq!(metrics.continent_count, 1);
    assert_eq!(metrics.largest_continent_cells, 2);
}

#[test]
fn metrics_are_deterministic_for_fixed_seed() {
    let params = GeologyParams {
        level: 2,
        ..Default::default()
    };
    let seed = "metrics-regression-seed";

    let terrain_a = crate::sim::build_geology(seed, params.clone());
    let terrain_b = crate::sim::build_geology(seed, params);
    let (positions, indices) = generate_icosphere(2);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let plate_id_a = terrain_a.plate_id;
    let plate_id_b = terrain_b.plate_id;

    let mut world_a = World::new(
        WorldMesh {
            positions: positions.clone(),
            nbr_offsets: nbr_offsets.clone(),
            nbrs: nbrs.clone(),
        },
        GeologyState {
            height: terrain_a.height,
            lake_depth: terrain_a.lake_depth,
            plate_id: plate_id_a,
            erosion_rate: vec![0.0; positions.len()],
            deposition_rate: vec![0.0; positions.len()],
            volcanism: vec![0.0; positions.len()],
            vertex_buoyancy: vec![0.0; positions.len()],
            geology_internal: vec![
                crate::sim::geology_types::GeologyInternal::default();
                positions.len()
            ],
            boundary_condition: vec![0.0; positions.len()],
        },
    );
    let mut world_b = World::new(
        WorldMesh {
            positions,
            nbr_offsets,
            nbrs,
        },
        GeologyState {
            height: terrain_b.height,
            lake_depth: terrain_b.lake_depth,
            plate_id: plate_id_b,
            erosion_rate: vec![0.0; world_a.cell_count()],
            deposition_rate: vec![0.0; world_a.cell_count()],
            volcanism: vec![0.0; world_a.cell_count()],
            vertex_buoyancy: vec![0.0; world_a.cell_count()],
            geology_internal: vec![
                crate::sim::geology_types::GeologyInternal::default();
                world_a.cell_count()
            ],
            boundary_condition: vec![0.0; world_a.cell_count()],
        },
    );

    for _ in 0..8 {
        exec_world(&mut world_a);
        exec_world(&mut world_b);
    }

    let metrics_a = world_a.metrics();
    let metrics_b = world_b.metrics();

    assert_eq!(metrics_a.cell_count, metrics_b.cell_count);
    assert_eq!(metrics_a.land_cells, metrics_b.land_cells);
    assert!((metrics_a.land_ratio - metrics_b.land_ratio).abs() < 1e-6);
    assert!((metrics_a.mean_height - metrics_b.mean_height).abs() < 1e-6);
    assert!((metrics_a.height_std_dev - metrics_b.height_std_dev).abs() < 1e-6);
    assert!((metrics_a.max_river_flux - metrics_b.max_river_flux).abs() < 1e-6);
    assert!((metrics_a.top10_river_flux_sum - metrics_b.top10_river_flux_sum).abs() < 1e-6);
    assert_eq!(metrics_a.continent_count, metrics_b.continent_count);
    assert_eq!(
        metrics_a.largest_continent_cells,
        metrics_b.largest_continent_cells
    );
}

#[test]
fn river_network_persists_without_early_collapse() {
    let params = GeologyParams {
        level: 2,
        ..Default::default()
    };
    let seed = "river-network-stability-seed";

    let terrain = crate::sim::build_geology(seed, params.clone());
    let (positions, indices) = generate_icosphere(2);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let plate_id = terrain.plate_id;

    let mut world = World::new(
        WorldMesh {
            positions: positions.clone(),
            nbr_offsets: nbr_offsets.clone(),
            nbrs: nbrs.clone(),
        },
        GeologyState {
            height: terrain.height.clone(),
            lake_depth: terrain.lake_depth.clone(),
            plate_id,
            erosion_rate: vec![0.0; positions.len()],
            deposition_rate: vec![0.0; positions.len()],
            volcanism: vec![0.0; positions.len()],
            vertex_buoyancy: vec![0.0; positions.len()],
            geology_internal: vec![
                crate::sim::geology_types::GeologyInternal::default();
                positions.len()
            ],
            boundary_condition: vec![0.0; positions.len()],
        },
    );

    let erosion = ErosionAutomatonState {
        positions,
        nbr_offsets,
        nbrs,
        height: terrain.height,
        water: vec![0.0; terrain.river_flux.len()],
        sediment: vec![0.0; terrain.river_flux.len()],
        armor: vec![0.0; terrain.river_flux.len()],
        rain: vec![0.12; terrain.river_flux.len()],
        river_flux: terrain.river_flux,
        raw_river_flux: Vec::new(),
        river_next: terrain.river_next,
        active_queue: (0..world.cell_count() as u32).collect(),
        active_head: 0,
        in_queue: vec![1; world.cell_count()],
        rain_cursor: 0,
        tick: 0,
        last_rebuild_tick: 0,
        last_sink_full_rebuild_tick: 0,
        flux_scale_ema: 1.0,
        last_river_driver: 1.0,
        prev_river_next: world.state.hydrology.river_next.clone(),
        flow_heading: vec![[0.0, 0.0, 0.0]; world.cell_count()],
        groundwater_storage: vec![0.0; world.cell_count()],
        scratch_effective_runoff: vec![0.0; world.cell_count()],
        scratch_changed_mark: vec![0; world.cell_count()],
        scratch_flux_samples: Vec::with_capacity(world.cell_count() / 2),
        recent_changed: Vec::new(),
        sink_id: vec![-1; world.cell_count()],
        sink_route_next: vec![-1; world.cell_count()],
        sink_spill_cell: Vec::new(),
        sink_spill_to: Vec::new(),
        sink_capacity_total: Vec::new(),
        sink_capacity_remaining: Vec::new(),
        sink_storage_sediment: Vec::new(),
        sink_spill_level: Vec::new(),
        sink_overflow_active: Vec::new(),
        sink_dirty: vec![1; world.cell_count()],
        params,
    };
    crate::sim::hydrology::apply_hydrology_state_view(&mut world, &erosion)
        .expect("hydrology state should match world");
    let mut hydrology_state = Some(erosion);
    let mut feedback = FeedbackQueue::new(world.cell_count());

    for _ in 0..2 {
        exec_world_with_feedback_and_hydrology(&mut world, &mut feedback, &mut hydrology_state);
    }
    let metrics_t2 = world.metrics();

    for _ in 2..28 {
        exec_world_with_feedback_and_hydrology(&mut world, &mut feedback, &mut hydrology_state);
    }
    let metrics_t28 = world.metrics();

    assert!(metrics_t2.river_active_cells > 0);
    assert!(metrics_t28.river_active_cells > 0);
    assert!(metrics_t2.river_ocean_reach_ratio > 0.10);
    assert!(metrics_t28.river_ocean_reach_ratio > 0.05);
    assert!(metrics_t2.river_fragmentation_ratio < 0.95);
    assert!(metrics_t28.river_fragmentation_ratio < 0.98);
}

#[test]
fn geology_runtime_is_deterministic_for_fixed_seed_and_schedule() {
    let params = GeologyParams {
        level: 2,
        ..Default::default()
    };
    let mut world_a = build_generated_world("geology-runtime-determinism", params.clone());
    let mut world_b = build_generated_world("geology-runtime-determinism", params);

    for _ in 0..12 {
        exec_world(&mut world_a);
        exec_world(&mut world_b);
    }

    assert_geology_runtime_close(&world_a, &world_b, EPSILON);
}

#[test]
fn world_json_roundtrip_preserves_geology_snapshot_state() {
    let params = GeologyParams {
        level: 2,
        ..Default::default()
    };
    let mut world = build_generated_world("geology-runtime-snapshot", params);

    for _ in 0..6 {
        exec_world(&mut world);
    }

    let snapshot = serde_json::to_string(&world).expect("world snapshot serialize failed");
    let restored: World =
        serde_json::from_str(&snapshot).expect("world snapshot deserialize failed");

    assert_geology_runtime_close(&world, &restored, EPSILON);
}

#[test]
fn world_json_roundtrip_preserves_next_step_geology_evolution() {
    let params = GeologyParams {
        level: 2,
        ..Default::default()
    };
    let mut continuous = build_generated_world("geology-runtime-step", params);

    for _ in 0..5 {
        exec_world(&mut continuous);
    }

    let snapshot = serde_json::to_string(&continuous).expect("world snapshot serialize failed");
    let mut restored: World =
        serde_json::from_str(&snapshot).expect("world snapshot deserialize failed");

    exec_world(&mut continuous);
    exec_world(&mut restored);

    assert_geology_runtime_close(&continuous, &restored, EPSILON);
}
