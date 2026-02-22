# Kubeconfig Isolation

## Context

Most Kubernetes tools (k3d, minikube, kind) modify the user's global
`~/.kube/config` when creating clusters. They merge new cluster entries,
set the current context, and leave stale entries behind when clusters are
deleted. This creates several problems for a development tool like devrig:

- **Accidental production access** -- Switching the current context can cause
  subsequent kubectl commands to target the wrong cluster.
- **Config file corruption** -- Concurrent writes from multiple tools can
  produce malformed YAML.
- **Stale entries** -- Deleted clusters leave orphaned entries that clutter
  `kubectl config get-contexts` output.
- **Multi-project interference** -- Two devrig projects creating clusters
  simultaneously would race on the same config file.

devrig manages local Kubernetes clusters via k3d, and needs a strategy that
eliminates all of these risks.

## Decision

devrig never reads or writes `~/.kube/config`. All kubeconfig state is
isolated to the project's `.devrig/` directory.

### k3d cluster creation flags

When creating a k3d cluster, devrig passes two flags that prevent k3d from
touching the default kubeconfig:

```
k3d cluster create <name> \
    --kubeconfig-update-default=false \
    --kubeconfig-switch-context=false \
    ...
```

- `--kubeconfig-update-default=false` prevents k3d from merging the new
  cluster into `~/.kube/config`.
- `--kubeconfig-switch-context=false` prevents k3d from changing the
  current-context in `~/.kube/config`.

### Kubeconfig file location

After cluster creation, devrig writes the kubeconfig to:

```
<project-root>/.devrig/kubeconfig
```

This file is obtained by running `k3d kubeconfig get <cluster-name>` and
writing the output to the project-local path. The file contains the cluster
CA certificate, client certificate, client key, and server endpoint -- everything
needed to authenticate with the cluster.

### How devrig kubectl works

The `devrig kubectl` (and `devrig k`) commands set the `KUBECONFIG`
environment variable to point at `.devrig/kubeconfig` before exec-ing
kubectl:

```
KUBECONFIG=<project-root>/.devrig/kubeconfig kubectl <args...>
```

This ensures kubectl targets the correct cluster without inspecting or
modifying the user's global config.

### Manual kubectl access

Users who want to run kubectl directly (outside of devrig) can export the
kubeconfig path:

```bash
export KUBECONFIG=$(devrig cluster kubeconfig)
kubectl get pods
kubectl logs -f deployment/api
```

The `devrig cluster kubeconfig` command prints the absolute path to the
kubeconfig file and exits. This works well with shell substitution.

## Lifecycle

The kubeconfig file follows the cluster lifecycle:

| Event                    | Action                                          |
|--------------------------|-------------------------------------------------|
| `devrig cluster create`  | Kubeconfig written to `.devrig/kubeconfig`      |
| `devrig cluster delete`  | Kubeconfig removed along with `.devrig/` state  |
| `devrig delete`          | Full cleanup including cluster and kubeconfig    |

The kubeconfig is not preserved across cluster delete/create cycles. Each
new cluster gets a fresh kubeconfig with new certificates.

## Consequences

### Benefits

- **Zero side effects** -- Running devrig never alters `~/.kube/config`,
  regardless of how many projects or clusters exist.
- **Multi-project safety** -- Each project has its own kubeconfig in its own
  `.devrig/` directory. No file-level contention.
- **Clean teardown** -- Deleting a cluster removes its kubeconfig. No stale
  entries accumulate anywhere.
- **Predictable context** -- `devrig kubectl` always targets the project's
  cluster. There is no ambiguity about which context is active.

### Trade-offs

- Users must either use `devrig kubectl` or manually export `KUBECONFIG` to
  interact with the cluster. Standard `kubectl` without configuration will
  not see the devrig cluster.
- IDE Kubernetes plugins that read `~/.kube/config` will not automatically
  discover devrig clusters. Users must configure the plugin to use the
  project-local kubeconfig path.

### Verification

Integration tests verify kubeconfig isolation by comparing a checksum of
`~/.kube/config` before and after cluster creation. If the checksums differ,
the test fails, confirming that devrig's k3d flags are working correctly.
The test sequence is:

1. Compute SHA-256 of `~/.kube/config` (or record that it does not exist).
2. Run `devrig cluster create`.
3. Compute SHA-256 of `~/.kube/config` again.
4. Assert the checksums are identical.
5. Assert `.devrig/kubeconfig` exists and contains valid YAML.
6. Run `devrig cluster delete`.
7. Assert `.devrig/kubeconfig` no longer exists.
