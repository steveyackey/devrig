# ADR 005: Traefik over Nginx for ingress routing

## Status

Accepted

## Context

devrig needs an ingress routing solution for local development when services
are running behind a reverse proxy (especially in k3d Kubernetes clusters).
The two primary candidates are:

- **Nginx Ingress Controller**: the most widely deployed Kubernetes ingress
  controller. Requires explicit ingress resource definitions and a reload
  cycle when routes change.
- **Traefik**: a dynamic reverse proxy that supports auto-discovery via Docker
  labels and Kubernetes CRDs. Routes are configured declaratively alongside
  the services they front.

In a local development context, services start, stop, and restart frequently.
Routes need to update in real time without manual intervention.

## Decision

Use Traefik for ingress routing. Services declare their routing rules via
Docker labels (for container services) or Kubernetes IngressRoute CRDs (for
k3d-deployed services). Traefik watches for changes and updates its routing
table automatically.

Example Docker label configuration:

```toml
[infra.api]
image = "myapp-api:dev"
labels = { "traefik.http.routers.api.rule" = "PathPrefix(`/api`)" }
```

## Consequences

**Positive:**

- Zero-config routing: adding a label to a service is all that is needed.
  No separate ingress manifest to write or maintain.
- Dynamic routing: Traefik detects new and removed services automatically,
  which matches the development workflow of frequently restarting services.
- Built-in dashboard at port 8080 for debugging routing issues.
- Good support for both Docker and Kubernetes backends, which aligns with
  devrig's goal of supporting both container and cluster modes.

**Negative:**

- Teams with existing Nginx ingress configurations for production may need
  to maintain two sets of routing rules. This is acceptable because devrig
  targets local development only.
- Traefik's configuration model (labels and dynamic config) differs from
  the static Nginx config model, which may require learning.

**Neutral:**

- Traefik is a Go binary distributed as a single Docker image. It adds one
  container to the local environment but has minimal resource overhead.
