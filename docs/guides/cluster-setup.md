# Cluster Setup Guide

This guide covers setting up a local Kubernetes cluster with devrig, deploying
services into it, and using file watching for automatic rebuilds.

## Prerequisites

- **Docker** -- Running and accessible without sudo.
- **k3d** v5.x -- Lightweight k3s wrapper. Install from https://k3d.io.
- **kubectl** -- Kubernetes CLI. Install from https://kubernetes.io/docs/tasks/tools/.

Run `devrig doctor` to verify all tools are available:

```bash
devrig doctor
```

## Quick start

Add a `[cluster]` section to your `devrig.toml`:

```toml
[project]
name = "myapp"

[infra.postgres]
image = "postgres:16-alpine"
port = 5432
[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
[infra.postgres.ready_check]
type = "pg_isready"

[cluster]
name = "myapp-dev"
agents = 1
ports = ["8080:80"]

[cluster.deploy.api]
context = "./services/api"
dockerfile = "Dockerfile"
manifests = ["k8s/deployment.yaml", "k8s/service.yaml"]
watch = true
depends_on = ["postgres"]
```

Create the cluster and deploy:

```bash
devrig cluster create
```

This creates a k3d cluster with a local registry, builds all container images,
applies all Kubernetes manifests, and starts file watchers.

## Configuration reference

### `[cluster]` section

| Field    | Type            | Required | Default             | Description                                      |
|----------|-----------------|----------|---------------------|--------------------------------------------------|
| `name`   | string          | No       | `devrig-{slug}`     | k3d cluster name.                                |
| `agents` | integer         | No       | `1`                 | Number of k3d agent nodes.                       |
| `ports`  | list of strings | No       | `[]`                | Port mappings from host to cluster load balancer. |
| `registry` | boolean       | No       | `true`              | Whether to create a local container registry.    |

Port mappings follow the format `"hostPort:containerPort"` and are forwarded
through the k3d load balancer:

```toml
ports = ["8080:80", "8443:443"]
```

### `[cluster.deploy.*]` section

Each `[cluster.deploy.<name>]` block defines a service to build and deploy
into the cluster.

| Field        | Type            | Required | Default      | Description                                         |
|--------------|-----------------|----------|--------------|-----------------------------------------------------|
| `context`    | string          | Yes      | --           | Docker build context directory, relative to config. |
| `dockerfile` | string          | No       | `Dockerfile` | Dockerfile path, relative to context.               |
| `manifests`  | list of strings | Yes      | --           | Kubernetes manifest files to apply, relative to config. |
| `watch`      | boolean         | No       | `false`      | Enable file watching for automatic rebuild and redeploy. |
| `depends_on` | list of strings | No       | `[]`         | Infra or other deploy services to start before this.|

Manifests support the `{{ cluster.name }}` template variable, which resolves
to the cluster name for use in image references and labels.

## Registry usage

When `registry = true` (the default), devrig creates a k3d-managed container
registry alongside the cluster.

### Image naming conventions

The registry uses two addresses depending on context:

| Context               | Registry address                      | Example                                    |
|-----------------------|---------------------------------------|--------------------------------------------|
| Inside the cluster    | `k3d-devrig-{slug}-reg:5000`         | `k3d-devrig-myapp-a1b2c3d4-reg:5000/api`  |
| Host (docker push)    | `localhost:{port}`                    | `localhost:5100/api`                        |

devrig handles the naming automatically. When you define a deploy service,
devrig builds the image, tags it for the local registry, pushes it, and
references it correctly in your manifests.

### Pushing images manually

If you need to push an image outside of devrig:

```bash
docker build -t localhost:5100/myimage:latest .
docker push localhost:5100/myimage:latest
```

Inside your Kubernetes manifests, reference it as:

```yaml
image: k3d-devrig-myapp-a1b2c3d4-reg:5000/myimage:latest
```

## File watching

When `watch = true` on a deploy service, devrig monitors the build context
directory for changes and triggers automatic rebuilds and redeploys.

### Behavior

- **Debounce** -- Changes are debounced with a 500ms window. Multiple rapid
  file saves trigger a single rebuild.
- **Ignored directories** -- The following directories are ignored by default:
  `.git`, `node_modules`, `target`, `__pycache__`, `.devrig`.
- **Rebuild cycle** -- On change: docker build, docker push to registry,
  kubectl rollout restart.

### Example workflow

1. Edit source code in `./services/api/src/main.rs`.
2. devrig detects the change after 500ms of no further edits.
3. Docker image is rebuilt using the deploy context and Dockerfile.
4. Image is pushed to the local registry.
5. A rollout restart is triggered so pods pull the new image.

## Network connectivity

devrig connects the k3d cluster to the same Docker network used by infra
containers. This allows pods to reach infrastructure services by Docker
container name.

### Connectivity table

| From       | To         | How                                           | Example                                  |
|------------|------------|-----------------------------------------------|------------------------------------------|
| Pod        | Infra      | Docker container name on shared network       | `postgres://devrig:devrig@devrig-myapp-a1b2c3d4-postgres:5432` |
| Infra      | Pod        | Via cluster load balancer on Docker network   | `http://k3d-myapp-dev-serverlb:80`       |
| Host       | Pod        | Via port mappings in `cluster.ports`          | `http://localhost:8080`                  |
| Host       | Infra      | Via infra port mappings                       | `postgres://localhost:5432`              |

Pods should use the Docker container name (not `localhost`) when connecting
to infra services, since `localhost` inside a pod refers to the pod itself.

## CLI commands

### `devrig cluster create`

Create the k3d cluster, registry, build and deploy all services:

```bash
devrig cluster create
```

### `devrig cluster delete`

Tear down the cluster, registry, and all associated resources:

```bash
devrig cluster delete
```

### `devrig cluster kubeconfig`

Print the path to the isolated kubeconfig file:

```bash
devrig cluster kubeconfig
# Output: /home/user/myproject/.devrig/kubeconfig
```

Export it for use with kubectl directly:

```bash
export KUBECONFIG=$(devrig cluster kubeconfig)
kubectl get pods
```

### `devrig kubectl` / `devrig k`

Run kubectl commands against the devrig cluster with the correct kubeconfig
automatically set:

```bash
devrig kubectl get pods
devrig k get pods
devrig k logs -f deployment/api
devrig k exec -it deployment/api -- sh
devrig k apply -f extra-manifest.yaml
```

These commands are equivalent to setting `KUBECONFIG` and running kubectl
directly.

## Troubleshooting

### k3d not found

```
[!!] k3d    not found
```

Install k3d v5.x:

```bash
curl -s https://raw.githubusercontent.com/k3d-io/k3d/main/install.sh | bash
```

Verify with `k3d version`. devrig requires v5.x.

### Port conflicts

```
Error: port 8080 is already in use
```

Another process is using a port listed in `cluster.ports`. Find it with:

```bash
lsof -i :8080
```

Either stop the conflicting process or change the host port in your config:

```toml
ports = ["9080:80"]
```

### Slow cluster startup

k3d cluster creation can take 30-60 seconds on first run as it pulls the
k3s image. Subsequent creates reuse the cached image. If startup consistently
takes more than 2 minutes, check Docker resource limits (CPU and memory
allocation).

### Registry push failures

```
Error: push to localhost:5100 failed
```

Verify the registry container is running:

```bash
docker ps | grep registry
```

If the registry is missing, delete and recreate the cluster:

```bash
devrig cluster delete
devrig cluster create
```

Also confirm no firewall rules block localhost ports in the 5000-5200 range.
