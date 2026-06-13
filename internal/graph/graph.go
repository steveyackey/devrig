// Package graph implements Kahn's topological sort over a unified dependency
// graph that spans services, docker containers, compose services, and cluster
// resources.
package graph

import (
	"fmt"

	"github.com/steveyackey/devrig/internal/config"
)

// ResourceKind identifies what kind of resource a node represents.
type ResourceKind int

const (
	KindService ResourceKind = iota
	KindDocker
	KindCompose
	KindClusterImage
	KindClusterDeploy
)

func (k ResourceKind) String() string {
	switch k {
	case KindService:
		return "service"
	case KindDocker:
		return "docker"
	case KindCompose:
		return "compose"
	case KindClusterImage:
		return "cluster.image"
	case KindClusterDeploy:
		return "cluster.deploy"
	default:
		return "unknown"
	}
}

// Node is a vertex in the dependency graph.
type Node struct {
	Name string
	Kind ResourceKind
}

// Resolver builds and resolves a unified dependency graph.
type Resolver struct {
	nodes []Node
	// adj maps node name → names it must come after (its dependencies)
	deps map[string][]string
	// index maps name → node index
	index map[string]int
}

// New builds a Resolver from a parsed config. Returns an error if any
// depends_on reference names a resource that doesn't exist.
func New(cfg *config.Config) (*Resolver, error) {
	r := &Resolver{
		deps:  make(map[string][]string),
		index: make(map[string]int),
	}

	add := func(name string, kind ResourceKind) {
		if _, exists := r.index[name]; exists {
			return // already added (e.g. compose service listed twice)
		}
		r.index[name] = len(r.nodes)
		r.nodes = append(r.nodes, Node{Name: name, Kind: kind})
		r.deps[name] = nil
	}

	for name := range cfg.Docker {
		add(name, KindDocker)
	}
	if cfg.Compose != nil {
		for _, svc := range cfg.Compose.Services {
			add(svc, KindCompose)
		}
	}
	if cfg.Cluster != nil {
		for name := range cfg.Cluster.Images {
			add(name, KindClusterImage)
		}
		for name := range cfg.Cluster.Deploy {
			add(name, KindClusterDeploy)
		}
	}
	for name := range cfg.Services {
		add(name, KindService)
	}

	// Wire dependency edges.
	addEdges := func(name string, deps []string) error {
		for _, dep := range deps {
			if _, ok := r.index[dep]; !ok {
				return fmt.Errorf("%s %q depends on %q which is not defined", r.nodeKind(name), name, dep)
			}
			r.deps[name] = append(r.deps[name], dep)
		}
		return nil
	}

	for name, svc := range cfg.Services {
		if err := addEdges(name, svc.DependsOn); err != nil {
			return nil, err
		}
	}
	for name, d := range cfg.Docker {
		if err := addEdges(name, d.DependsOn); err != nil {
			return nil, err
		}
	}
	if cfg.Cluster != nil {
		for name, img := range cfg.Cluster.Images {
			if err := addEdges(name, img.DependsOn); err != nil {
				return nil, err
			}
		}
		for name, dep := range cfg.Cluster.Deploy {
			if err := addEdges(name, dep.DependsOn); err != nil {
				return nil, err
			}
		}
	}

	return r, nil
}

// StartOrder returns nodes in topological order (dependencies first).
// Returns an error if the graph contains a cycle.
func (r *Resolver) StartOrder() ([]Node, error) {
	// Kahn's algorithm
	inDegree := make(map[string]int, len(r.nodes))
	// dependents[dep] = list of nodes that depend on dep
	dependents := make(map[string][]string, len(r.nodes))

	for name := range r.deps {
		if _, ok := inDegree[name]; !ok {
			inDegree[name] = 0
		}
		for _, dep := range r.deps[name] {
			dependents[dep] = append(dependents[dep], name)
			inDegree[name]++
		}
	}

	// Seed queue with nodes that have no dependencies.
	var queue []string
	for name := range inDegree {
		if inDegree[name] == 0 {
			queue = append(queue, name)
		}
	}

	var order []Node
	for len(queue) > 0 {
		// Pop first element (deterministic: sort would be ideal but isn't required).
		cur := queue[0]
		queue = queue[1:]
		order = append(order, r.nodes[r.index[cur]])

		for _, dependent := range dependents[cur] {
			inDegree[dependent]--
			if inDegree[dependent] == 0 {
				queue = append(queue, dependent)
			}
		}
	}

	if len(order) != len(r.nodes) {
		// Find a node still in a cycle for the error message.
		for name, deg := range inDegree {
			if deg > 0 {
				return nil, fmt.Errorf("dependency cycle detected involving %q", name)
			}
		}
		return nil, fmt.Errorf("dependency cycle detected")
	}

	return order, nil
}

// Node returns the Node for a given resource name.
func (r *Resolver) Node(name string) (Node, bool) {
	idx, ok := r.index[name]
	if !ok {
		return Node{}, false
	}
	return r.nodes[idx], true
}

func (r *Resolver) nodeKind(name string) string {
	if idx, ok := r.index[name]; ok {
		return r.nodes[idx].Kind.String()
	}
	return "resource"
}
