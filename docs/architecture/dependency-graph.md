# Dependency Graph

## Overview

devrig models dependencies as a directed acyclic graph (DAG) using a small
custom resolver (no external graph library). The graph spans not just services
but also docker containers, compose services, and cluster resources, and it
determines the order in which everything is started while validating that no
circular dependencies exist.

The implementation lives in `internal/graph/graph.go` in the `Resolver`
struct.

## Graph construction

The graph is built from the parsed `Config`:

### Pass 1: Add nodes

Every service in `[services.*]`, every `[docker.*]` container, each compose
service, and each cluster resource becomes a `Node` (tagged with a
`ResourceKind`). Nodes are appended to a slice, with an `index` map from name to
slice position for O(1) lookup.

### Pass 2: Add edges

For each node, iterate over its `depends_on` list and record each dependency in
the resolver's `deps` map (name → names it must come after).

If a dependency references a resource name that does not exist, graph
construction fails with an error message naming both the referencing resource
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

The `StartOrder()` method runs Kahn's algorithm over the graph: it computes the
in-degree of every node, seeds a queue with the nodes that have no dependencies,
and repeatedly pops a node and decrements the in-degree of its dependents. This
returns nodes in an order where every dependency appears before the resources
that depend on it.

For the example above, the start order would be: `db`, `api`, `web`.

## Cycle detection

If Kahn's algorithm cannot emit every node (some nodes still have a non-zero
in-degree once the queue drains), the graph contains a cycle. `StartOrder()`
returns an error naming one of the nodes still involved in the cycle:

```
dependency cycle detected involving "api"
```

Cycle detection also happens during validation (in
`internal/config/validate.go`) so that cycle errors are reported alongside other
validation errors (missing deps, duplicate ports) and users see all problems at
once.

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
command = "go run ./cmd/api"
depends_on = ["db", "cache"]

[services.web]
command = "npm run dev"
depends_on = ["api"]

[services.worker]
command = "go run ./cmd/worker"
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
