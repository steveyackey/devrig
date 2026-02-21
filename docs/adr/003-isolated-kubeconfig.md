# ADR 003: Isolated kubeconfig at .devrig/kubeconfig

## Status

Accepted

## Context

When devrig creates a k3d cluster for local Kubernetes development, k3d
generates a kubeconfig file that contains credentials and API server addresses
for the new cluster.

By default, k3d merges this kubeconfig into `~/.kube/config`, which is the
standard location for kubectl configuration. This merging behavior creates
several problems:

- It modifies a file that may be carefully managed by the user or by other
  tools (e.g. cloud provider CLIs, corporate VPN tooling).
- It can change the user's current-context, causing `kubectl` commands in
  other terminals to unexpectedly target the devrig cluster.
- Cleanup is error-prone: removing the cluster does not always clean up the
  merged entries from `~/.kube/config`.

## Decision

Store the devrig-managed kubeconfig at `.devrig/kubeconfig` within the project
directory. Never read from or write to `~/.kube/config`.

When invoking `kubectl` or other Kubernetes tools on behalf of the user, devrig
sets the `KUBECONFIG` environment variable to point at the project-local file.
The `devrig start` output includes instructions for how to use this kubeconfig
manually:

```bash
export KUBECONFIG=.devrig/kubeconfig
kubectl get pods
```

## Consequences

**Positive:**

- No interference with the user's existing kubectl setup.
- Multiple devrig projects can run simultaneously without kubeconfig conflicts,
  since each project has its own `.devrig/kubeconfig`.
- `devrig delete` can cleanly remove the kubeconfig along with the rest of the
  `.devrig/` directory.
- Deterministic: devrig always knows exactly where the kubeconfig is.

**Negative:**

- Users must set `KUBECONFIG` or pass `--kubeconfig` when using kubectl
  outside of devrig. This is intentional -- it prevents accidental operations
  against the devrig cluster.
- Some Kubernetes GUI tools may not automatically discover the project-local
  kubeconfig.
