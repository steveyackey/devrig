package cluster

import (
	"context"
	"fmt"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/state"
	"github.com/steveyackey/devrig/internal/tools"
	"github.com/steveyackey/devrig/internal/verbose"
)

// BuildAndDeploy builds an image, pushes it to the registry, and applies
// the service manifests. It updates cs with the new image tag. r resolves the
// kubectl binary. configDir anchors the build context and manifest paths.
func BuildAndDeploy(
	ctx context.Context,
	r *tools.Resolver,
	name string,
	cfg *config.ClusterDeployConfig,
	cs *state.ClusterState,
	stateDir string,
	configDir string,
) error {
	tag, err := buildImage(ctx, name, cfg, cs, configDir)
	if err != nil {
		return fmt.Errorf("deploy %s: build: %w", name, err)
	}

	if cfg.Manifests != "" {
		manifests := cfg.Manifests
		if !filepath.IsAbs(manifests) {
			manifests = filepath.Join(configDir, manifests)
		}
		if err := KubectlApply(ctx, r, cs.KubeconfigPath, manifests); err != nil {
			return fmt.Errorf("deploy %s: apply manifests: %w", name, err)
		}
		// Attempt a rollout restart to pick up the new image.
		_ = KubectlRollout(ctx, r, cs.KubeconfigPath, "default", name)
	}

	cs.DeployedServices[name] = state.ClusterDeployState{
		ImageTag:     tag,
		LastDeployed: time.Now(),
	}
	return nil
}

// BuildImage builds and pushes an image for a standalone image config (no
// deploy). configDir is the directory of the devrig config file; the build
// context and dockerfile are resolved relative to it. build_args support
// {{ cluster.image.<name>.tag }} references to images already built (read from
// cs.DeployedServices). The resulting image tag is recorded in cs.
func BuildImage(
	ctx context.Context,
	name string,
	cfg *config.ClusterImageConfig,
	cs *state.ClusterState,
	configDir string,
) (string, error) {
	var tag string
	if cs.RegistryName != nil && cs.RegistryPort != nil {
		tag = fmt.Sprintf("localhost:%d/%s:%d", *cs.RegistryPort, name, time.Now().Unix())
	} else {
		tag = fmt.Sprintf("devrig-%s:latest", name)
	}

	dockerfile := cfg.Dockerfile
	if dockerfile == "" {
		dockerfile = "Dockerfile"
	}
	args := []string{"build",
		"-t", tag,
		"-f", dockerfile,
	}
	for k, v := range cfg.BuildArgs {
		args = append(args, "--build-arg", fmt.Sprintf("%s=%s", k, interpolateImageRefs(v, cs)))
	}
	for k, v := range cfg.BuildSecrets {
		args = append(args, "--secret", fmt.Sprintf("id=%s,src=%s", k, v))
	}
	args = append(args, ".")

	cmd := exec.CommandContext(ctx, "docker", args...)
	cmd.Dir = filepath.Join(configDir, cfg.Context)
	if out, err := verbose.Run(cmd); err != nil {
		return "", fmt.Errorf("docker build: %w\n%s", err, out)
	}

	if cs.RegistryPort != nil {
		latest := fmt.Sprintf("localhost:%d/%s:latest", *cs.RegistryPort, name)
		if out, err := verbose.Run(exec.CommandContext(ctx, "docker", "tag", tag, latest)); err != nil {
			return "", fmt.Errorf("docker tag: %w\n%s", err, out)
		}
		if out, err := verbose.Run(exec.CommandContext(ctx, "docker", "push", tag)); err != nil {
			return "", fmt.Errorf("docker push: %w\n%s", err, out)
		}
		if out, err := verbose.Run(exec.CommandContext(ctx, "docker", "push", latest)); err != nil {
			return "", fmt.Errorf("docker push latest: %w\n%s", err, out)
		}
	}

	cs.DeployedServices[name] = state.ClusterDeployState{
		ImageTag:     tag,
		LastDeployed: time.Now(),
	}
	return tag, nil
}

// ImageBuildOrder returns cluster image names ordered so that an image is built
// after every image it depends on. Dependencies come from explicit depends_on
// and from {{ cluster.image.<name>.tag }} references in build_args (e.g. a
// runtime image whose FROM is a base image built by devrig). Returns an error
// on a dependency cycle.
func ImageBuildOrder(images map[string]config.ClusterImageConfig) ([]string, error) {
	deps := make(map[string]map[string]bool, len(images))
	for name, cfg := range images {
		d := make(map[string]bool)
		for _, dep := range cfg.DependsOn {
			if _, ok := images[dep]; ok {
				d[dep] = true
			}
		}
		for other := range images {
			if other == name {
				continue
			}
			ref := fmt.Sprintf("{{ cluster.image.%s.tag }}", other)
			for _, v := range cfg.BuildArgs {
				if strings.Contains(v, ref) {
					d[other] = true
				}
			}
		}
		deps[name] = d
	}

	// Deterministic order: process names alphabetically when unblocked.
	names := make([]string, 0, len(images))
	for name := range images {
		names = append(names, name)
	}
	sort.Strings(names)

	var order []string
	done := make(map[string]bool, len(images))
	for len(order) < len(images) {
		progressed := false
		for _, name := range names {
			if done[name] {
				continue
			}
			ready := true
			for dep := range deps[name] {
				if !done[dep] {
					ready = false
					break
				}
			}
			if ready {
				order = append(order, name)
				done[name] = true
				progressed = true
			}
		}
		if !progressed {
			return nil, fmt.Errorf("cluster image dependency cycle detected")
		}
	}
	return order, nil
}

// interpolateImageRefs replaces {{ cluster.image.<name>.tag }} references with
// the full image tag of an already-built image recorded in cs.DeployedServices.
// References to images not yet built are left unchanged.
func interpolateImageRefs(value string, cs *state.ClusterState) string {
	for name, ds := range cs.DeployedServices {
		value = strings.ReplaceAll(value,
			fmt.Sprintf("{{ cluster.image.%s.tag }}", name), ds.ImageTag)
	}
	return value
}

func buildImage(ctx context.Context, name string, cfg *config.ClusterDeployConfig, cs *state.ClusterState, configDir string) (string, error) {
	var tag string
	if cs.RegistryName != nil && cs.RegistryPort != nil {
		tag = fmt.Sprintf("localhost:%d/%s:%d", *cs.RegistryPort, name, time.Now().Unix())
	} else {
		tag = fmt.Sprintf("devrig-%s:latest", name)
	}

	dockerfile := cfg.Dockerfile
	if dockerfile == "" {
		dockerfile = "Dockerfile"
	}
	args := []string{"build",
		"-t", tag,
		"-f", dockerfile,
	}
	for k, v := range cfg.BuildSecrets {
		args = append(args, "--secret", fmt.Sprintf("id=%s,src=%s", k, v))
	}
	args = append(args, ".")

	cmd := exec.CommandContext(ctx, "docker", args...)
	cmd.Dir = filepath.Join(configDir, cfg.Context)
	if out, err := verbose.Run(cmd); err != nil {
		return "", fmt.Errorf("docker build: %w\n%s", err, out)
	}

	if cs.RegistryPort != nil {
		latest := fmt.Sprintf("localhost:%d/%s:latest", *cs.RegistryPort, name)
		if out, err := verbose.Run(exec.CommandContext(ctx, "docker", "tag", tag, latest)); err != nil {
			return "", fmt.Errorf("docker tag: %w\n%s", err, out)
		}
		if out, err := verbose.Run(exec.CommandContext(ctx, "docker", "push", tag)); err != nil {
			return "", fmt.Errorf("docker push: %w\n%s", err, out)
		}
		if out, err := verbose.Run(exec.CommandContext(ctx, "docker", "push", latest)); err != nil {
			return "", fmt.Errorf("docker push latest: %w\n%s", err, out)
		}
	}
	return tag, nil
}
