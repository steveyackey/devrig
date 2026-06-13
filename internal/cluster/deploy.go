package cluster

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
	"time"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/state"
)

// BuildAndDeploy builds an image, pushes it to the registry, and applies
// the service manifests. It updates cs with the new image tag.
func BuildAndDeploy(
	ctx context.Context,
	name string,
	cfg *config.ClusterDeployConfig,
	cs *state.ClusterState,
	stateDir string,
) error {
	tag, err := buildImage(ctx, name, cfg, cs)
	if err != nil {
		return fmt.Errorf("deploy %s: build: %w", name, err)
	}

	if cfg.Manifests != "" {
		if err := KubectlApply(ctx, cs.KubeconfigPath, cfg.Manifests); err != nil {
			return fmt.Errorf("deploy %s: apply manifests: %w", name, err)
		}
		// Attempt a rollout restart to pick up the new image.
		_ = KubectlRollout(ctx, cs.KubeconfigPath, "default", name)
	}

	cs.DeployedServices[name] = state.ClusterDeployState{
		ImageTag:     tag,
		LastDeployed: time.Now(),
	}
	return nil
}

// BuildImage builds and pushes an image for a standalone image config (no deploy).
func BuildImage(
	ctx context.Context,
	name string,
	cfg *config.ClusterImageConfig,
	cs *state.ClusterState,
) (string, error) {
	var tag string
	if cs.RegistryName != nil && cs.RegistryPort != nil {
		tag = fmt.Sprintf("localhost:%d/%s:%d", *cs.RegistryPort, name, time.Now().Unix())
	} else {
		tag = fmt.Sprintf("devrig-%s:latest", name)
	}

	args := []string{"build",
		"-t", tag,
		"-f", cfg.Dockerfile,
	}
	for k, v := range cfg.BuildArgs {
		args = append(args, "--build-arg", fmt.Sprintf("%s=%s", k, v))
	}
	for k, v := range cfg.BuildSecrets {
		args = append(args, "--secret", fmt.Sprintf("id=%s,src=%s", k, v))
	}
	args = append(args, cfg.Context)

	cmd := exec.CommandContext(ctx, "docker", args...)
	if out, err := cmd.CombinedOutput(); err != nil {
		return "", fmt.Errorf("docker build: %w\n%s", err, out)
	}

	if cs.RegistryPort != nil {
		latest := fmt.Sprintf("localhost:%d/%s:latest", *cs.RegistryPort, name)
		if out, err := exec.CommandContext(ctx, "docker", "tag", tag, latest).CombinedOutput(); err != nil {
			return "", fmt.Errorf("docker tag: %w\n%s", err, out)
		}
		if out, err := exec.CommandContext(ctx, "docker", "push", tag).CombinedOutput(); err != nil {
			return "", fmt.Errorf("docker push: %w\n%s", err, out)
		}
		if out, err := exec.CommandContext(ctx, "docker", "push", latest).CombinedOutput(); err != nil {
			return "", fmt.Errorf("docker push latest: %w\n%s", err, out)
		}
	}

	cs.DeployedServices[name] = state.ClusterDeployState{
		ImageTag:     tag,
		LastDeployed: time.Now(),
	}
	return tag, nil
}

func buildImage(ctx context.Context, name string, cfg *config.ClusterDeployConfig, cs *state.ClusterState) (string, error) {
	var tag string
	if cs.RegistryName != nil && cs.RegistryPort != nil {
		tag = fmt.Sprintf("localhost:%d/%s:%d", *cs.RegistryPort, name, time.Now().Unix())
	} else {
		tag = fmt.Sprintf("devrig-%s:latest", name)
	}

	args := []string{"build",
		"-t", tag,
		"-f", cfg.Dockerfile,
	}
	for k, v := range cfg.BuildSecrets {
		args = append(args, "--secret", fmt.Sprintf("id=%s,src=%s", k, v))
	}
	if cfg.Context != "" {
		args = append(args, cfg.Context)
	} else {
		args = append(args, ".")
	}

	cmd := exec.CommandContext(ctx, "docker", args...)
	if out, err := cmd.CombinedOutput(); err != nil {
		return "", fmt.Errorf("docker build: %w\n%s", err, strings.TrimSpace(string(out)))
	}

	if cs.RegistryPort != nil {
		latest := fmt.Sprintf("localhost:%d/%s:latest", *cs.RegistryPort, name)
		if out, err := exec.CommandContext(ctx, "docker", "tag", tag, latest).CombinedOutput(); err != nil {
			return "", fmt.Errorf("docker tag: %w\n%s", err, out)
		}
		if out, err := exec.CommandContext(ctx, "docker", "push", tag).CombinedOutput(); err != nil {
			return "", fmt.Errorf("docker push: %w\n%s", err, out)
		}
		if out, err := exec.CommandContext(ctx, "docker", "push", latest).CombinedOutput(); err != nil {
			return "", fmt.Errorf("docker push latest: %w\n%s", err, out)
		}
	}
	return tag, nil
}
