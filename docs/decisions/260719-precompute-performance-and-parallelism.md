# Precompute performance and parallelism

## Status

Accepted

## Context

Level-6, 1600-tick store generation was dominated by persistent-material projection. Late failures
also lacked enough periodic timing to identify whether simulation or store output was responsible.
Optimization must preserve deterministic fixed-seed output and the scientific model.

## Decision

- Emit periodic cumulative timing for world advancement, delta construction/write, and keyframes.
  Keep module and persistent-material phase profiles opt-in.
- Reuse projections when no intervening stage mutates persistent material.
- On native builds, parallelize independent ridge-gap reconstruction and persistent-element
  projection. WebAssembly remains sequential.
- Workers may update only their assigned element hosts and produce private overlap records. Deposit
  records on the main thread in original element, candidate-cell, and overlap order, preserving the
  floating-point accumulation order.
- Process projection in batches of at most 131,072 elements, merging each batch before starting the
  next. This bounds temporary overlap storage without changing result order.
- Keep global topology and cross-cell reactions ordered.
- Retain level-6, 1600-tick generation as the release validation workload.

## Validation

On a 32-logical-CPU host, each late projection fell from about 510-560 ms to 35-53 ms. The
`alpha`, level-6, 120-tick output exactly matched the sequential result after removing `run_id` and
was repeatable across parallel runs. Its wall time fell from about 130 to 29-30 seconds.

The 1600-tick release run completed in 550.43 seconds with 338.40 ms/tick world advancement and
about 1.74 GiB peak RSS. It passed the former failure points at ticks 986 and 1244. Commands and
validation gates are recorded in `docs/operations/bench/geology/validation.md`.

## Consequences

- Native precompute uses all available logical CPUs and increases host contention.
- Batching bounds projection intermediates, but world state and allocator retention still grow.
- Fragment thresholds remain model decisions, not performance tuning.
