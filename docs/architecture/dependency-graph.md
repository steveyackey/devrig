# Dependency Graph

## Overview

devrig uses the `petgraph` crate to model service dependencies as a directed
acyclic graph (DAG). The graph determines the order in which services are
started and validates that no circular dependencies exist.

The implementation lives in `orchestrator/graph.rs` in the `DependencyResolver`
struct.

## Graph construction

The graph is built in two passes from the `DevrigConfig`:

### Pass 1: Add nodes

Every service defined in `[services.*]` becomes a node in the graph. Nodes
are stored in a `BTreeMap<String, NodeIndex>` for O(log n) lookup by name.

### Pass 2: Add edges

For each service, iterate over its `depends_on` list. For each dependency,
look up the dependency's `NodeIndex` and add a directed edge.

If a dependency references a service name that does not exist, graph
construction fails with an error message naming both the referencing service
and the missing dependency.

## Edge direction convention

Edges point **from dependency to dependent**:

```
  db -----> api -----> web
  (dependency)  (dependency)
```

If service `web` depends on `api`, and `api` depends on `db`, the edges are:

- `db -> api`
- `api -> web`

This convention means that a topological sort naturally yields the startup
order: nodes with no incoming edges (leaf dependencies) come first.

## Topological sort for start order

The `start_order()` method calls `petgraph::algo::toposort()` on the graph.
This returns nodes in an order where every dependency appears before the
services that depend on it.

For the example above, the start order would be: `db`, `api`, `web`.

When services have no dependencies on each other, they appear in alphabetical
order due to the use of `BTreeMap` for node insertion.

## Cycle detection

`petgraph::algo::toposort()` returns `Err(Cycle)` if the graph contains a
cycle. The error includes the `NodeIndex` of one node involved in the cycle,
which is used to produce an error message like:

```
dependency cycle detected involving service 'api'
```

Cycle detection also happens during validation (in `config/validate.rs`) using
an independent iterative DFS with visited/in-stack tracking. This provides
cycle errors alongside other validation errors (missing deps, duplicate ports)
so users see all problems at once.

## Service filtering

When `devrig start <service1> <service2>` is invoked with specific services,
the orchestrator computes the transitive closure of their dependencies:

1. Start with the set of requested services.
2. For each service in the set, add all of its `depends_on` entries.
3. Repeat until the set stops growing.
4. Filter the topological order to include only services in this set.

This ensures that starting `web` automatically starts `api` and `db` if they
are transitive dependencies.

## Example

Given this configuration:

```toml
[services.db]
command = "docker compose up postgres"

[services.cache]
command = "docker compose up redis"

[services.api]
command = "cargo watch -x run"
depends_on = ["db", "cache"]

[services.web]
command = "npm run dev"
depends_on = ["api"]

[services.worker]
command = "cargo run --bin worker"
depends_on = ["db"]
```

The graph looks like:

```
  db -------> api -------> web
  |            ^
  |            |
  +---> worker cache
```

A valid start order: `cache`, `db`, `api`, `worker`, `web` (or `db`, `cache`,
`api`, `worker`, `web` -- both are valid topological orderings).
