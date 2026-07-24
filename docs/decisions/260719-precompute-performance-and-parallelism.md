# Precompute performance and parallelism

## Status

Draft

## Context

`precompute_world` regenerates a level-6, 1600-tick store serially. A failed long-running
simulation currently reports its aggregate timing only after the final manifest is written, so a
panic late in the run loses the information needed to distinguish simulation, view-delta,
compression, and durable-write costs. The default developer command consequently has an
unbounded feedback cycle as the model and stored fields grow.

## Proposal

First add periodic, cumulative phase timing to precompute. The report must expose elapsed time,
completed ticks, and the time spent in world advancement, delta construction, delta encoding and
write, and keyframe write. The interval is configurable and defaults to 16 ticks.

An opt-in module profile records the geology, climate, glaciology, hydrology, ecology, society,
and transition portions of every precompute tick. It is diagnostic-only and may add profiling
overhead, so it is not enabled in normal store generation.

Persistent material has a separate opt-in phase profile because it has distinct advection,
projection, boundary-reaction, and rasterization stages. It remains confined to that module to
avoid coupling measurement fields into the serialized world state.

After collecting a level-6 baseline, evaluate optimizations in this order:

1. Preserve atomic store publication while batching durability barriers rather than syncing every
   delta file.
2. Pipeline immutable delta encoding and file writes behind the sequential world advancement
   loop, with bounded memory and deterministic frame ordering.
3. Parallelize independent per-cell and per-element phases inside a tick. Global topology,
   boundary reactions, and floating-point reductions remain ordered reduction phases so that a
   fixed seed remains reproducible.

Before introducing concurrency, eliminate duplicate projection passes when no intervening stage
mutates persistent material. This is a semantics-preserving local optimization and establishes a
baseline for subsequent parallel work.

The 1600-tick level-6 generation remains the release validation workload. Shorter and/or lower
resolution stores are developer feedback workloads, not replacements for that validation.

## Consequences

- The first change adds small logging overhead but makes late failures diagnosable.
- I/O pipelining and data-parallel phases require measured speedups and deterministic regression
  checks before adoption.
- Changing fragment removal thresholds is a model decision and must be documented separately; it
  is not a performance optimization.
