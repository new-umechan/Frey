use serde::Serialize;

use crate::sim::world::{FeedbackQueue, ModuleId, World};

use super::pipeline::{
    finalize_tick, prepare_step, run_climate_stage, run_conflict_stage, run_domesticates_stage,
    run_ecology_stage, run_feedback_stage, run_geology_stage_with_geology,
    run_glaciology_stage_with_hydrology, run_hydrology_stage_with_hydrology, run_polity_stage,
    run_population_stage, run_settlement_stage, run_subsistence_stage, run_transition_stage,
    ExecWorldPhase,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldResource {
    Clock,
    Control,
    TerrainProjection,
    GeologyCells,
    ClimateCells,
    GlaciologyCells,
    HydrologyCells,
    EcologyCells,
    DomesticatesCells,
    SubsistenceCells,
    PopulationCells,
    SettlementCells,
    PolityCells,
    ConflictCells,
    Entities,
    PolityRelations,
    PlateRelations,
}

pub struct ModuleExecContext<'a> {
    pub feedback: &'a mut FeedbackQueue,
    pub geology_state: &'a mut super::GeologyExecState,
    pub hydrology_state: &'a mut super::HydrologyExecState,
}

pub struct ModuleDeclaration {
    pub phase: ExecWorldPhase,
    pub module_id: ModuleId,
    pub reads: &'static [WorldResource],
    pub writes: &'static [WorldResource],
    pub feedback: &'static [ModuleId],
    pub feedback_mode: FeedbackMode,
    pub profile_category: ProfileCategory,
    pub display_group: DisplayGroup,
    pub execution_kind: ExecutionKind,
    pub completes_tick: bool,
    pub step: for<'a> fn(&mut World, &mut ModuleExecContext<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleDependency {
    pub from: ExecWorldPhase,
    pub to: ExecWorldPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackMode {
    None,
    ModuleInbox,
    ExecInbox,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileCategory {
    None,
    Feedback,
    GeologyTerrain,
    Climate,
    Glaciology,
    Hydrology,
    Ecology,
    Society,
    Transition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionKind {
    Plain,
    HydrologyCoupled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayGroup {
    Feedback,
    Geology,
    Climate,
    Glaciology,
    Hydrology,
    Ecology,
    Society,
    Transition,
    PostStep,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleManifest {
    pub phase: ExecWorldPhase,
    pub module_id: ModuleId,
    pub phase_key: &'static str,
    pub module_key: &'static str,
    pub description: &'static str,
    pub feedback_mode: FeedbackMode,
    pub profile_category: ProfileCategory,
    pub display_group: DisplayGroup,
    pub execution_kind: ExecutionKind,
    pub completes_tick: bool,
    pub reads: Vec<WorldResource>,
    pub writes: Vec<WorldResource>,
    pub feedback: Vec<ModuleId>,
    pub depends_on: Vec<ExecWorldPhase>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModuleDocRecord {
    pub phase: &'static str,
    pub module: &'static str,
    pub description: &'static str,
    pub inbox: &'static str,
    pub profile: &'static str,
    pub display: &'static str,
    pub execution: &'static str,
    pub tick_boundary: bool,
    pub reads: Vec<&'static str>,
    pub writes: Vec<&'static str>,
    pub feedback_targets: Vec<&'static str>,
    pub depends_on: Vec<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModuleGraphEdgeRecord {
    pub from_phase: &'static str,
    pub from_module: &'static str,
    pub to_phase: &'static str,
    pub to_module: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModuleGraphRecord {
    pub modules: Vec<ModuleDocRecord>,
    pub edges: Vec<ModuleGraphEdgeRecord>,
}

const EXEC_PREPARE_READS: &[WorldResource] = &[WorldResource::Clock];
const EXEC_PREPARE_WRITES: &[WorldResource] = &[WorldResource::Clock];

const EXEC_FEEDBACK_READS: &[WorldResource] = &[WorldResource::Clock, WorldResource::Entities];
const EXEC_FEEDBACK_WRITES: &[WorldResource] = &[
    WorldResource::DomesticatesCells,
    WorldResource::Entities,
    WorldResource::Clock,
];

const GEOLOGY_READS: &[WorldResource] = &[
    WorldResource::Clock,
    WorldResource::Control,
    WorldResource::TerrainProjection,
    WorldResource::GeologyCells,
    WorldResource::ClimateCells,
    WorldResource::GlaciologyCells,
    WorldResource::HydrologyCells,
];
const GEOLOGY_WRITES: &[WorldResource] =
    &[WorldResource::GeologyCells, WorldResource::HydrologyCells];

const CLIMATE_READS: &[WorldResource] = &[
    WorldResource::Clock,
    WorldResource::Control,
    WorldResource::TerrainProjection,
    WorldResource::GeologyCells,
    WorldResource::ClimateCells,
];
const CLIMATE_WRITES: &[WorldResource] = &[WorldResource::ClimateCells];

const GLACIOLOGY_READS: &[WorldResource] = &[
    WorldResource::Clock,
    WorldResource::Control,
    WorldResource::TerrainProjection,
    WorldResource::GeologyCells,
    WorldResource::ClimateCells,
    WorldResource::GlaciologyCells,
];
const GLACIOLOGY_WRITES: &[WorldResource] =
    &[WorldResource::GeologyCells, WorldResource::GlaciologyCells];

const HYDROLOGY_READS: &[WorldResource] = &[
    WorldResource::Clock,
    WorldResource::Control,
    WorldResource::TerrainProjection,
    WorldResource::GeologyCells,
    WorldResource::ClimateCells,
    WorldResource::HydrologyCells,
    WorldResource::GlaciologyCells,
];
const HYDROLOGY_WRITES: &[WorldResource] = &[
    WorldResource::TerrainProjection,
    WorldResource::GeologyCells,
    WorldResource::HydrologyCells,
];

const ECOLOGY_READS: &[WorldResource] = &[
    WorldResource::Clock,
    WorldResource::TerrainProjection,
    WorldResource::ClimateCells,
    WorldResource::HydrologyCells,
    WorldResource::EcologyCells,
];
const ECOLOGY_WRITES: &[WorldResource] = &[WorldResource::EcologyCells];

const SOCIETY_READS: &[WorldResource] = &[
    WorldResource::Clock,
    WorldResource::TerrainProjection,
    WorldResource::ClimateCells,
    WorldResource::HydrologyCells,
    WorldResource::EcologyCells,
    WorldResource::DomesticatesCells,
    WorldResource::SubsistenceCells,
    WorldResource::PopulationCells,
    WorldResource::SettlementCells,
    WorldResource::PolityCells,
    WorldResource::ConflictCells,
    WorldResource::Entities,
    WorldResource::PolityRelations,
];
const SOCIETY_WRITES: &[WorldResource] = &[
    WorldResource::DomesticatesCells,
    WorldResource::SubsistenceCells,
    WorldResource::PopulationCells,
    WorldResource::SettlementCells,
    WorldResource::PolityCells,
    WorldResource::ConflictCells,
    WorldResource::Entities,
    WorldResource::PolityRelations,
];

const TRANSITION_READS: &[WorldResource] = &[
    WorldResource::Clock,
    WorldResource::GeologyCells,
    WorldResource::ClimateCells,
    WorldResource::EcologyCells,
];
const TRANSITION_WRITES: &[WorldResource] = &[WorldResource::Clock];

const FINALIZE_READS: &[WorldResource] = &[WorldResource::Clock];
const FINALIZE_WRITES: &[WorldResource] = &[WorldResource::Clock];

fn step_prepare(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    prepare_step(world);
}

fn step_feedback(world: &mut World, ctx: &mut ModuleExecContext<'_>) {
    run_feedback_stage(world, ctx.feedback);
}

fn step_geology(world: &mut World, ctx: &mut ModuleExecContext<'_>) {
    run_geology_stage_with_geology(world, ctx.geology_state);
}

fn step_climate(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    run_climate_stage(world);
}

fn step_glaciology(world: &mut World, ctx: &mut ModuleExecContext<'_>) {
    run_glaciology_stage_with_hydrology(world, ctx.hydrology_state);
}

fn step_hydrology(world: &mut World, ctx: &mut ModuleExecContext<'_>) {
    run_hydrology_stage_with_hydrology(world, ctx.geology_state, ctx.hydrology_state);
}

fn step_ecology(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    run_ecology_stage(world);
}

fn step_domesticates(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    run_domesticates_stage(world);
}

fn step_subsistence(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    run_subsistence_stage(world);
}

fn step_population(world: &mut World, ctx: &mut ModuleExecContext<'_>) {
    run_population_stage(world, ctx.feedback);
}

fn step_settlement(world: &mut World, ctx: &mut ModuleExecContext<'_>) {
    run_settlement_stage(world, ctx.feedback);
}

fn step_polity(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    run_polity_stage(world);
}

fn step_conflict(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    run_conflict_stage(world);
}

fn step_transition(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    run_transition_stage(world);
}

fn step_finalize(world: &mut World, _ctx: &mut ModuleExecContext<'_>) {
    finalize_tick(world);
}

pub const MODULE_DECLARATIONS: &[ModuleDeclaration] = &[
    ModuleDeclaration {
        phase: ExecWorldPhase::Prepare,
        module_id: ModuleId::Exec,
        reads: EXEC_PREPARE_READS,
        writes: EXEC_PREPARE_WRITES,
        feedback: &[],
        feedback_mode: FeedbackMode::None,
        profile_category: ProfileCategory::None,
        display_group: DisplayGroup::Feedback,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_prepare,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::ExecFeedback,
        module_id: ModuleId::Exec,
        reads: EXEC_FEEDBACK_READS,
        writes: EXEC_FEEDBACK_WRITES,
        feedback: &[],
        feedback_mode: FeedbackMode::ExecInbox,
        profile_category: ProfileCategory::Feedback,
        display_group: DisplayGroup::Feedback,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_feedback,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Geology,
        module_id: ModuleId::Geology,
        reads: GEOLOGY_READS,
        writes: GEOLOGY_WRITES,
        feedback: &[ModuleId::Hydrology],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::GeologyTerrain,
        display_group: DisplayGroup::Geology,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_geology,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Climate,
        module_id: ModuleId::Climate,
        reads: CLIMATE_READS,
        writes: CLIMATE_WRITES,
        feedback: &[ModuleId::Ecology],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Climate,
        display_group: DisplayGroup::Climate,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_climate,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Glaciology,
        module_id: ModuleId::Glaciology,
        reads: GLACIOLOGY_READS,
        writes: GLACIOLOGY_WRITES,
        feedback: &[ModuleId::Geology, ModuleId::Hydrology],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Glaciology,
        display_group: DisplayGroup::Glaciology,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_glaciology,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Hydrology,
        module_id: ModuleId::Hydrology,
        reads: HYDROLOGY_READS,
        writes: HYDROLOGY_WRITES,
        feedback: &[ModuleId::Ecology],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Hydrology,
        display_group: DisplayGroup::Hydrology,
        execution_kind: ExecutionKind::HydrologyCoupled,
        completes_tick: false,
        step: step_hydrology,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Ecology,
        module_id: ModuleId::Ecology,
        reads: ECOLOGY_READS,
        writes: ECOLOGY_WRITES,
        feedback: &[ModuleId::Domesticates, ModuleId::Subsistence],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Ecology,
        display_group: DisplayGroup::Ecology,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_ecology,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Domesticates,
        module_id: ModuleId::Domesticates,
        reads: SOCIETY_READS,
        writes: SOCIETY_WRITES,
        feedback: &[ModuleId::Population, ModuleId::Settlement, ModuleId::Polity],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Society,
        display_group: DisplayGroup::Society,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_domesticates,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Subsistence,
        module_id: ModuleId::Subsistence,
        reads: SOCIETY_READS,
        writes: SOCIETY_WRITES,
        feedback: &[ModuleId::Population],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Society,
        display_group: DisplayGroup::Society,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_subsistence,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Population,
        module_id: ModuleId::Population,
        reads: SOCIETY_READS,
        writes: SOCIETY_WRITES,
        feedback: &[ModuleId::Settlement, ModuleId::Polity, ModuleId::Conflict],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Society,
        display_group: DisplayGroup::Society,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_population,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Settlement,
        module_id: ModuleId::Settlement,
        reads: SOCIETY_READS,
        writes: SOCIETY_WRITES,
        feedback: &[ModuleId::Polity, ModuleId::Conflict],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Society,
        display_group: DisplayGroup::Society,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_settlement,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Polity,
        module_id: ModuleId::Polity,
        reads: SOCIETY_READS,
        writes: SOCIETY_WRITES,
        feedback: &[ModuleId::Conflict],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Society,
        display_group: DisplayGroup::Society,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_polity,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Conflict,
        module_id: ModuleId::Conflict,
        reads: SOCIETY_READS,
        writes: SOCIETY_WRITES,
        feedback: &[],
        feedback_mode: FeedbackMode::ModuleInbox,
        profile_category: ProfileCategory::Society,
        display_group: DisplayGroup::Society,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_conflict,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Transition,
        module_id: ModuleId::Exec,
        reads: TRANSITION_READS,
        writes: TRANSITION_WRITES,
        feedback: &[],
        feedback_mode: FeedbackMode::None,
        profile_category: ProfileCategory::Transition,
        display_group: DisplayGroup::Transition,
        execution_kind: ExecutionKind::Plain,
        completes_tick: false,
        step: step_transition,
    },
    ModuleDeclaration {
        phase: ExecWorldPhase::Finalize,
        module_id: ModuleId::Exec,
        reads: FINALIZE_READS,
        writes: FINALIZE_WRITES,
        feedback: &[],
        feedback_mode: FeedbackMode::None,
        profile_category: ProfileCategory::None,
        display_group: DisplayGroup::PostStep,
        execution_kind: ExecutionKind::Plain,
        completes_tick: true,
        step: step_finalize,
    },
];

pub fn declaration_for_phase(phase: ExecWorldPhase) -> &'static ModuleDeclaration {
    MODULE_DECLARATIONS
        .iter()
        .find(|declaration| declaration.phase == phase)
        .expect("module declaration is missing for execution phase")
}

pub fn phase_accepts_module_feedback(phase: ExecWorldPhase) -> bool {
    declaration_for_phase(phase).feedback_mode == FeedbackMode::ModuleInbox
}

pub fn phase_accepts_exec_feedback(phase: ExecWorldPhase) -> bool {
    declaration_for_phase(phase).feedback_mode == FeedbackMode::ExecInbox
}

pub fn phase_profile_category(phase: ExecWorldPhase) -> ProfileCategory {
    declaration_for_phase(phase).profile_category
}

pub fn phase_execution_kind(phase: ExecWorldPhase) -> ExecutionKind {
    declaration_for_phase(phase).execution_kind
}

pub fn phase_completes_tick(phase: ExecWorldPhase) -> bool {
    declaration_for_phase(phase).completes_tick
}

pub fn phase_display_group(phase: ExecWorldPhase) -> DisplayGroup {
    declaration_for_phase(phase).display_group
}

pub fn first_phase() -> ExecWorldPhase {
    declared_phase_order()
        .into_iter()
        .next()
        .expect("module declarations must not be empty")
}

pub fn declared_phase_order() -> Vec<ExecWorldPhase> {
    topologically_sorted_phases()
}

pub fn module_manifests() -> Vec<ModuleManifest> {
    let mut depends_on = std::collections::HashMap::<ExecWorldPhase, Vec<ExecWorldPhase>>::new();
    for dependency in declared_dependencies() {
        depends_on
            .entry(dependency.to)
            .or_default()
            .push(dependency.from);
    }

    declared_phase_order()
        .into_iter()
        .map(|phase| {
            let declaration = declaration_for_phase(phase);
            let mut dependency_phases = depends_on.remove(&phase).unwrap_or_default();
            dependency_phases.sort_by_key(phase_declaration_index);
            ModuleManifest {
                phase,
                module_id: declaration.module_id,
                phase_key: phase_key(phase),
                module_key: module_key(declaration.module_id),
                description: module_description(phase),
                feedback_mode: declaration.feedback_mode,
                profile_category: declaration.profile_category,
                display_group: declaration.display_group,
                execution_kind: declaration.execution_kind,
                completes_tick: declaration.completes_tick,
                reads: declaration.reads.to_vec(),
                writes: declaration.writes.to_vec(),
                feedback: declaration.feedback.to_vec(),
                depends_on: dependency_phases,
            }
        })
        .collect()
}

pub fn module_manifest_lines() -> Vec<String> {
    module_manifests()
        .into_iter()
        .map(|manifest| {
            format!(
                "{phase} [{module}] inbox={inbox} profile={profile} display={display} exec={exec} tick_boundary={tick_boundary} reads={reads} writes={writes} feedback={feedback} depends_on={depends_on} desc=\"{description}\"",
                phase = manifest.phase_key,
                module = manifest.module_key,
                inbox = feedback_mode_key(manifest.feedback_mode),
                profile = profile_category_key(manifest.profile_category),
                display = display_group_key(manifest.display_group),
                exec = execution_kind_key(manifest.execution_kind),
                tick_boundary = tick_boundary_key(manifest.completes_tick),
                reads = join_world_resources(&manifest.reads),
                writes = join_world_resources(&manifest.writes),
                feedback = join_module_ids(&manifest.feedback),
                depends_on = join_exec_phases(&manifest.depends_on),
                description = manifest.description,
            )
        })
        .collect()
}

pub fn module_doc_records() -> Vec<ModuleDocRecord> {
    module_manifests()
        .into_iter()
        .map(|manifest| ModuleDocRecord {
            phase: manifest.phase_key,
            module: manifest.module_key,
            description: manifest.description,
            inbox: feedback_mode_key(manifest.feedback_mode),
            profile: profile_category_key(manifest.profile_category),
            display: display_group_key(manifest.display_group),
            execution: execution_kind_key(manifest.execution_kind),
            tick_boundary: manifest.completes_tick,
            reads: manifest.reads.into_iter().map(world_resource_key).collect(),
            writes: manifest
                .writes
                .into_iter()
                .map(world_resource_key)
                .collect(),
            feedback_targets: manifest.feedback.into_iter().map(module_key).collect(),
            depends_on: manifest.depends_on.into_iter().map(phase_key).collect(),
        })
        .collect()
}

pub fn module_graph_edge_records() -> Vec<ModuleGraphEdgeRecord> {
    declared_dependencies()
        .into_iter()
        .map(|dependency| {
            let from = declaration_for_phase(dependency.from);
            let to = declaration_for_phase(dependency.to);
            ModuleGraphEdgeRecord {
                from_phase: phase_key(dependency.from),
                from_module: module_key(from.module_id),
                to_phase: phase_key(dependency.to),
                to_module: module_key(to.module_id),
            }
        })
        .collect()
}

pub fn module_graph_record() -> ModuleGraphRecord {
    ModuleGraphRecord {
        modules: module_doc_records(),
        edges: module_graph_edge_records(),
    }
}

pub fn next_phase_after(phase: ExecWorldPhase) -> ExecWorldPhase {
    let phases = declared_phase_order();
    let index = phases
        .iter()
        .position(|candidate| *candidate == phase)
        .expect("module declaration is missing for execution phase");
    phases.get(index + 1).copied().unwrap_or_else(first_phase)
}

pub fn declared_dependencies() -> Vec<ModuleDependency> {
    let mut dependencies = Vec::new();
    for (from_index, from) in MODULE_DECLARATIONS.iter().enumerate() {
        for to in MODULE_DECLARATIONS.iter().skip(from_index + 1) {
            if modules_require_dependency(from, to) {
                dependencies.push(ModuleDependency {
                    from: from.phase,
                    to: to.phase,
                });
            }
        }
    }
    dependencies
}

pub fn validate_module_declarations() -> Result<(), String> {
    let phases = declared_phase_order();
    if phases.is_empty() {
        return Err("module declarations must not be empty".to_string());
    }
    if phases.first().copied() != Some(first_phase()) {
        return Err("module declarations must start with Prepare".to_string());
    }
    if phases.last().copied()
        != MODULE_DECLARATIONS
            .iter()
            .find(|declaration| declaration.completes_tick)
            .map(|declaration| declaration.phase)
    {
        return Err("module declarations must end with Finalize".to_string());
    }
    for expected in MODULE_DECLARATIONS
        .iter()
        .map(|declaration| declaration.phase)
    {
        let count = phases.iter().filter(|phase| **phase == expected).count();
        if count != 1 {
            return Err(format!(
                "module declaration count mismatch for {expected:?}: expected 1, got {count}"
            ));
        }
    }
    for declaration in MODULE_DECLARATIONS {
        let mut duplicated_reads = declaration.reads.to_vec();
        duplicated_reads.sort_unstable_by_key(|resource| *resource as u8);
        duplicated_reads.dedup();
        if duplicated_reads.len() != declaration.reads.len() {
            return Err(format!(
                "duplicate reads in declaration for phase {:?}",
                declaration.phase
            ));
        }
        let mut duplicated_writes = declaration.writes.to_vec();
        duplicated_writes.sort_unstable_by_key(|resource| *resource as u8);
        duplicated_writes.dedup();
        if duplicated_writes.len() != declaration.writes.len() {
            return Err(format!(
                "duplicate writes in declaration for phase {:?}",
                declaration.phase
            ));
        }
    }
    Ok(())
}

fn topologically_sorted_phases() -> Vec<ExecWorldPhase> {
    let declarations = MODULE_DECLARATIONS;
    let dependencies = declared_dependencies();
    let mut incoming_counts = declarations
        .iter()
        .map(|declaration| (declaration.phase, 0usize))
        .collect::<std::collections::HashMap<_, _>>();
    let mut outgoing = declarations
        .iter()
        .map(|declaration| (declaration.phase, Vec::new()))
        .collect::<std::collections::HashMap<_, Vec<ExecWorldPhase>>>();

    for dependency in dependencies {
        *incoming_counts
            .get_mut(&dependency.to)
            .expect("dependency target must be declared") += 1;
        outgoing
            .get_mut(&dependency.from)
            .expect("dependency source must be declared")
            .push(dependency.to);
    }

    let declaration_index = declarations
        .iter()
        .enumerate()
        .map(|(index, declaration)| (declaration.phase, index))
        .collect::<std::collections::HashMap<_, _>>();

    let mut ready = declarations
        .iter()
        .filter_map(|declaration| {
            (incoming_counts
                .get(&declaration.phase)
                .copied()
                .unwrap_or_default()
                == 0)
                .then_some(declaration.phase)
        })
        .collect::<Vec<_>>();
    ready.sort_by_key(|phase| declaration_index[phase]);

    let mut ordered = Vec::with_capacity(declarations.len());
    while let Some(phase) = ready.first().copied() {
        ready.remove(0);
        ordered.push(phase);
        if let Some(targets) = outgoing.get(&phase) {
            for target in targets {
                let count = incoming_counts
                    .get_mut(target)
                    .expect("dependency target must be declared");
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ready.push(*target);
                }
            }
            ready.sort_by_key(|candidate| declaration_index[candidate]);
        }
    }

    assert_eq!(
        ordered.len(),
        declarations.len(),
        "module declarations must form an acyclic graph"
    );
    ordered
}

fn modules_require_dependency(from: &ModuleDeclaration, to: &ModuleDeclaration) -> bool {
    resource_overlap(from.writes, to.reads)
        || resource_overlap(from.writes, to.writes)
        || from.feedback.contains(&to.module_id)
}

fn resource_overlap(lhs: &[WorldResource], rhs: &[WorldResource]) -> bool {
    lhs.iter().any(|resource| rhs.contains(resource))
}

fn phase_declaration_index(phase: &ExecWorldPhase) -> usize {
    MODULE_DECLARATIONS
        .iter()
        .position(|declaration| declaration.phase == *phase)
        .expect("module declaration is missing for execution phase")
}

pub fn phase_key(phase: ExecWorldPhase) -> &'static str {
    match phase {
        ExecWorldPhase::Prepare => "prepare",
        ExecWorldPhase::ExecFeedback => "exec_feedback",
        ExecWorldPhase::Geology => "geology",
        ExecWorldPhase::Climate => "climate",
        ExecWorldPhase::Glaciology => "glaciology",
        ExecWorldPhase::Hydrology => "hydrology",
        ExecWorldPhase::Ecology => "ecology",
        ExecWorldPhase::Domesticates => "domesticates",
        ExecWorldPhase::Subsistence => "subsistence",
        ExecWorldPhase::Population => "population",
        ExecWorldPhase::Settlement => "settlement",
        ExecWorldPhase::Polity => "polity",
        ExecWorldPhase::Conflict => "conflict",
        ExecWorldPhase::Transition => "transition",
        ExecWorldPhase::Finalize => "finalize",
    }
}

pub fn module_key(module_id: ModuleId) -> &'static str {
    match module_id {
        ModuleId::Exec => "exec",
        ModuleId::Geology => "geology",
        ModuleId::Climate => "climate",
        ModuleId::Glaciology => "glaciology",
        ModuleId::Hydrology => "hydrology",
        ModuleId::Ecology => "ecology",
        ModuleId::Domesticates => "domesticates",
        ModuleId::Subsistence => "subsistence",
        ModuleId::Population => "population",
        ModuleId::Settlement => "settlement",
        ModuleId::Polity => "polity",
        ModuleId::Conflict => "conflict",
    }
}

pub fn world_resource_key(resource: WorldResource) -> &'static str {
    match resource {
        WorldResource::Clock => "clock",
        WorldResource::Control => "control",
        WorldResource::TerrainProjection => "terrain_projection",
        WorldResource::GeologyCells => "geology_cells",
        WorldResource::ClimateCells => "climate_cells",
        WorldResource::GlaciologyCells => "glaciology_cells",
        WorldResource::HydrologyCells => "hydrology_cells",
        WorldResource::EcologyCells => "ecology_cells",
        WorldResource::DomesticatesCells => "domesticates_cells",
        WorldResource::SubsistenceCells => "subsistence_cells",
        WorldResource::PopulationCells => "population_cells",
        WorldResource::SettlementCells => "settlement_cells",
        WorldResource::PolityCells => "polity_cells",
        WorldResource::ConflictCells => "conflict_cells",
        WorldResource::Entities => "entities",
        WorldResource::PolityRelations => "polity_relations",
        WorldResource::PlateRelations => "plate_relations",
    }
}

pub fn module_description(phase: ExecWorldPhase) -> &'static str {
    match phase {
        ExecWorldPhase::Prepare => "tick budget and epoch preparation",
        ExecWorldPhase::ExecFeedback => {
            "apply global exec-targeted feedback queued before this tick"
        }
        ExecWorldPhase::Geology => "advance tectonics and terrain-coupled geology state",
        ExecWorldPhase::Climate => "update climate transport and surface fields",
        ExecWorldPhase::Glaciology => "update ice state and glaciology forcing",
        ExecWorldPhase::Hydrology => "update runoff, routing, and erosion coupling",
        ExecWorldPhase::Ecology => "update biome and ecosystem state",
        ExecWorldPhase::Domesticates => "update crop and livestock adoption",
        ExecWorldPhase::Subsistence => "update food production and extraction",
        ExecWorldPhase::Population => "update population growth and movement",
        ExecWorldPhase::Settlement => "update settlements and local urban structure",
        ExecWorldPhase::Polity => "update polity organization and territorial control",
        ExecWorldPhase::Conflict => "update conflict and inter-polity pressure",
        ExecWorldPhase::Transition => "advance era transition rules",
        ExecWorldPhase::Finalize => "close the tick and advance the clock",
    }
}

pub fn feedback_mode_key(mode: FeedbackMode) -> &'static str {
    match mode {
        FeedbackMode::None => "none",
        FeedbackMode::ModuleInbox => "module_inbox",
        FeedbackMode::ExecInbox => "exec_inbox",
    }
}

pub fn profile_category_key(category: ProfileCategory) -> &'static str {
    match category {
        ProfileCategory::None => "none",
        ProfileCategory::Feedback => "feedback",
        ProfileCategory::GeologyTerrain => "geology_terrain",
        ProfileCategory::Climate => "climate",
        ProfileCategory::Glaciology => "glaciology",
        ProfileCategory::Hydrology => "hydrology",
        ProfileCategory::Ecology => "ecology",
        ProfileCategory::Society => "society",
        ProfileCategory::Transition => "transition",
    }
}

pub fn execution_kind_key(kind: ExecutionKind) -> &'static str {
    match kind {
        ExecutionKind::Plain => "plain",
        ExecutionKind::HydrologyCoupled => "hydrology_coupled",
    }
}

pub fn tick_boundary_key(completes_tick: bool) -> &'static str {
    if completes_tick {
        "yes"
    } else {
        "no"
    }
}

pub fn display_group_key(group: DisplayGroup) -> &'static str {
    match group {
        DisplayGroup::Feedback => "feedback",
        DisplayGroup::Geology => "geology",
        DisplayGroup::Climate => "climate",
        DisplayGroup::Glaciology => "glaciology",
        DisplayGroup::Hydrology => "hydrology",
        DisplayGroup::Ecology => "ecology",
        DisplayGroup::Society => "society",
        DisplayGroup::Transition => "transition",
        DisplayGroup::PostStep => "post_step",
    }
}

fn join_world_resources(resources: &[WorldResource]) -> String {
    join_keys(
        resources
            .iter()
            .map(|resource| world_resource_key(*resource)),
    )
}

fn join_module_ids(module_ids: &[ModuleId]) -> String {
    join_keys(module_ids.iter().map(|module_id| module_key(*module_id)))
}

fn join_exec_phases(phases: &[ExecWorldPhase]) -> String {
    join_keys(phases.iter().map(|phase| phase_key(*phase)))
}

fn join_keys<'a>(keys: impl Iterator<Item = &'a str>) -> String {
    let collected = keys.collect::<Vec<_>>();
    if collected.is_empty() {
        "-".to_string()
    } else {
        collected.join(",")
    }
}
