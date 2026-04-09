# Module Declaration DAG (Generated)

この文書は `rust/src/sim/exec/modules.rs` の宣言から自動生成される。

## Modules

| phase | module | inbox | profile | display | execution | tick_boundary | reads | writes | feedback | depends_on | description |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| prepare | exec | none | none | feedback | plain | no | clock | clock |  |  | tick budget and epoch preparation |
| exec_feedback | exec | exec_inbox | feedback | feedback | plain | no | clock, entities | domesticates_cells, entities, clock |  | prepare | apply global exec-targeted feedback queued before this tick |
| geology | geology | module_inbox | geology_terrain | geology | plain | no | clock, control, terrain_projection, geology_cells, climate_cells, glaciology_cells, hydrology_cells | geology_cells, hydrology_cells | hydrology | prepare, exec_feedback | advance tectonics and terrain-coupled geology state |
| climate | climate | module_inbox | climate | climate | plain | no | clock, control, terrain_projection, geology_cells, climate_cells | climate_cells | ecology | prepare, exec_feedback, geology | update climate transport and surface fields |
| glaciology | glaciology | module_inbox | glaciology | glaciology | plain | no | clock, control, terrain_projection, geology_cells, climate_cells, glaciology_cells | terrain_projection, geology_cells, glaciology_cells | geology, hydrology | prepare, exec_feedback, geology, climate | update ice state and glaciology forcing |
| hydrology | hydrology | module_inbox | hydrology | hydrology | hydrology_coupled | no | clock, control, terrain_projection, geology_cells, climate_cells, hydrology_cells, glaciology_cells | terrain_projection, geology_cells, hydrology_cells | ecology | prepare, exec_feedback, geology, climate, glaciology | update runoff, routing, and erosion coupling |
| ecology | ecology | module_inbox | ecology | ecology | plain | no | clock, terrain_projection, climate_cells, hydrology_cells, ecology_cells | ecology_cells | domesticates, subsistence | prepare, exec_feedback, geology, climate, glaciology, hydrology | update biome and ecosystem state |
| domesticates | domesticates | module_inbox | society | society | plain | no | clock, terrain_projection, climate_cells, hydrology_cells, ecology_cells, domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | population, settlement, polity | prepare, exec_feedback, geology, climate, glaciology, hydrology, ecology | update crop and livestock adoption |
| subsistence | subsistence | module_inbox | society | society | plain | no | clock, terrain_projection, climate_cells, hydrology_cells, ecology_cells, domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | population | prepare, exec_feedback, geology, climate, glaciology, hydrology, ecology, domesticates | update food production and extraction |
| population | population | module_inbox | society | society | plain | no | clock, terrain_projection, climate_cells, hydrology_cells, ecology_cells, domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | settlement, polity, conflict | prepare, exec_feedback, geology, climate, glaciology, hydrology, ecology, domesticates, subsistence | update population growth and movement |
| settlement | settlement | module_inbox | society | society | plain | no | clock, terrain_projection, climate_cells, hydrology_cells, ecology_cells, domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | polity, conflict | prepare, exec_feedback, geology, climate, glaciology, hydrology, ecology, domesticates, subsistence, population | update settlements and local urban structure |
| polity | polity | module_inbox | society | society | plain | no | clock, terrain_projection, climate_cells, hydrology_cells, ecology_cells, domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | conflict | prepare, exec_feedback, geology, climate, glaciology, hydrology, ecology, domesticates, subsistence, population, settlement | update polity organization and territorial control |
| conflict | conflict | module_inbox | society | society | plain | no | clock, terrain_projection, climate_cells, hydrology_cells, ecology_cells, domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations | domesticates_cells, subsistence_cells, population_cells, settlement_cells, polity_cells, conflict_cells, entities, polity_relations |  | prepare, exec_feedback, geology, climate, glaciology, hydrology, ecology, domesticates, subsistence, population, settlement, polity | update conflict and inter-polity pressure |
| transition | exec | none | transition | transition | plain | no | clock, geology_cells, climate_cells, ecology_cells | clock |  | prepare, exec_feedback, geology, climate, glaciology, hydrology, ecology | advance era transition rules |
| finalize | exec | none | none | post_step | plain | yes | clock | clock |  | prepare, exec_feedback, transition | close the tick and advance the clock |

## Edges

| from_phase | from_module | to_phase | to_module |
| --- | --- | --- | --- |
| prepare | exec | exec_feedback | exec |
| prepare | exec | geology | geology |
| prepare | exec | climate | climate |
| prepare | exec | glaciology | glaciology |
| prepare | exec | hydrology | hydrology |
| prepare | exec | ecology | ecology |
| prepare | exec | domesticates | domesticates |
| prepare | exec | subsistence | subsistence |
| prepare | exec | population | population |
| prepare | exec | settlement | settlement |
| prepare | exec | polity | polity |
| prepare | exec | conflict | conflict |
| prepare | exec | transition | exec |
| prepare | exec | finalize | exec |
| exec_feedback | exec | geology | geology |
| exec_feedback | exec | climate | climate |
| exec_feedback | exec | glaciology | glaciology |
| exec_feedback | exec | hydrology | hydrology |
| exec_feedback | exec | ecology | ecology |
| exec_feedback | exec | domesticates | domesticates |
| exec_feedback | exec | subsistence | subsistence |
| exec_feedback | exec | population | population |
| exec_feedback | exec | settlement | settlement |
| exec_feedback | exec | polity | polity |
| exec_feedback | exec | conflict | conflict |
| exec_feedback | exec | transition | exec |
| exec_feedback | exec | finalize | exec |
| geology | geology | climate | climate |
| geology | geology | glaciology | glaciology |
| geology | geology | hydrology | hydrology |
| geology | geology | ecology | ecology |
| geology | geology | domesticates | domesticates |
| geology | geology | subsistence | subsistence |
| geology | geology | population | population |
| geology | geology | settlement | settlement |
| geology | geology | polity | polity |
| geology | geology | conflict | conflict |
| geology | geology | transition | exec |
| climate | climate | glaciology | glaciology |
| climate | climate | hydrology | hydrology |
| climate | climate | ecology | ecology |
| climate | climate | domesticates | domesticates |
| climate | climate | subsistence | subsistence |
| climate | climate | population | population |
| climate | climate | settlement | settlement |
| climate | climate | polity | polity |
| climate | climate | conflict | conflict |
| climate | climate | transition | exec |
| glaciology | glaciology | hydrology | hydrology |
| glaciology | glaciology | ecology | ecology |
| glaciology | glaciology | domesticates | domesticates |
| glaciology | glaciology | subsistence | subsistence |
| glaciology | glaciology | population | population |
| glaciology | glaciology | settlement | settlement |
| glaciology | glaciology | polity | polity |
| glaciology | glaciology | conflict | conflict |
| glaciology | glaciology | transition | exec |
| hydrology | hydrology | ecology | ecology |
| hydrology | hydrology | domesticates | domesticates |
| hydrology | hydrology | subsistence | subsistence |
| hydrology | hydrology | population | population |
| hydrology | hydrology | settlement | settlement |
| hydrology | hydrology | polity | polity |
| hydrology | hydrology | conflict | conflict |
| hydrology | hydrology | transition | exec |
| ecology | ecology | domesticates | domesticates |
| ecology | ecology | subsistence | subsistence |
| ecology | ecology | population | population |
| ecology | ecology | settlement | settlement |
| ecology | ecology | polity | polity |
| ecology | ecology | conflict | conflict |
| ecology | ecology | transition | exec |
| domesticates | domesticates | subsistence | subsistence |
| domesticates | domesticates | population | population |
| domesticates | domesticates | settlement | settlement |
| domesticates | domesticates | polity | polity |
| domesticates | domesticates | conflict | conflict |
| subsistence | subsistence | population | population |
| subsistence | subsistence | settlement | settlement |
| subsistence | subsistence | polity | polity |
| subsistence | subsistence | conflict | conflict |
| population | population | settlement | settlement |
| population | population | polity | polity |
| population | population | conflict | conflict |
| settlement | settlement | polity | polity |
| settlement | settlement | conflict | conflict |
| polity | polity | conflict | conflict |
| transition | exec | finalize | exec |

module_count: 15
edge_count: 88

