// Package orchestrator is the central startup coordinator. It loads config,
// resolves the dependency graph, allocates ports, resolves template variables,
// and supervises all services.
//
// Multi-phase startup order:
//
//	Phase 0   — Parse config, validate, load previous state
//	Phase 0.5 — Dashboard + OTel collector
//	Phase 0.6 — Reserve OIDC provider port
//	Phase 1   — Docker network (stub until Docker Phase 3)
//	Phase 2   — Compose services (stub until Docker Phase 3)
//	Phase 3   — Infrastructure containers (stub until Docker Phase 3)
//	Phase 3.5 — k3d cluster
//	Phase 4   — Resolve ports, template vars, DEVRIG_* env
//	Phase 4.5 — Bind OIDC provider
//	Phase 4.9 — Save state, register instance
//	Phase 5   — Spawn service supervisors
package orchestrator

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/steveyackey/devrig/internal/cluster"
	"github.com/steveyackey/devrig/internal/compose"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/dashboard"
	"github.com/steveyackey/devrig/internal/docker"
	"github.com/steveyackey/devrig/internal/events"
	"github.com/steveyackey/devrig/internal/graph"
	"github.com/steveyackey/devrig/internal/identity"
	"github.com/steveyackey/devrig/internal/oidc"
	"github.com/steveyackey/devrig/internal/otel"
	"github.com/steveyackey/devrig/internal/ports"
	"github.com/steveyackey/devrig/internal/registry"
	"github.com/steveyackey/devrig/internal/state"
	"github.com/steveyackey/devrig/internal/supervisor"
)

// Orchestrator manages the full lifecycle of a devrig project.
type Orchestrator struct {
	cfg        *config.Config
	id         *identity.ProjectIdentity
	cfgPath    string
	stateDir   string
	logBroadcast   *events.Broadcaster
	eventBroadcast *events.Broadcaster
}

// New loads and validates a config file, computes the project identity, and
// returns a ready-to-use Orchestrator.
func New(cfgPath string) (*Orchestrator, error) {
	abs, err := filepath.Abs(cfgPath)
	if err != nil {
		return nil, fmt.Errorf("resolving config path: %w", err)
	}

	cfg, _, err := config.Load(abs)
	if err != nil {
		return nil, fmt.Errorf("loading config %s: %w", abs, err)
	}
	if err := config.Validate(cfg); err != nil {
		return nil, fmt.Errorf("config errors:\n%w", err)
	}

	id, err := identity.New(cfg.Project.Name, abs)
	if err != nil {
		return nil, fmt.Errorf("computing project identity: %w", err)
	}

	// State dir is .devrig/ next to the config file (matches Rust behaviour).
	stateDir := filepath.Join(filepath.Dir(abs), ".devrig")

	return &Orchestrator{
		cfg:            cfg,
		id:             id,
		cfgPath:        abs,
		stateDir:       stateDir,
		logBroadcast:   &events.Broadcaster{},
		eventBroadcast: &events.Broadcaster{},
	}, nil
}

// Start runs the full startup sequence. filter restricts which services are
// started (empty = all). devMode spawns the Vite dev server for --dev.
func (o *Orchestrator) Start(filter []string, devMode bool) error {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// ================================================================
	// Phase 0: Dependency graph, previous state
	// ================================================================
	resolver, err := graph.New(o.cfg)
	if err != nil {
		return fmt.Errorf("building dependency graph: %w", err)
	}
	fullOrder, err := resolver.StartOrder()
	if err != nil {
		return fmt.Errorf("resolving start order: %w", err)
	}

	prevState := state.Load(o.stateDir)

	launchOrder, err := o.applyFilter(fullOrder, filter)
	if err != nil {
		return err
	}

	dashEnabled := o.cfg.Dashboard != nil && (o.cfg.Dashboard.Enabled == nil || *o.cfg.Dashboard.Enabled)
	oidcEnabled := o.cfg.OIDC != nil

	if len(launchOrder) == 0 && !dashEnabled && !oidcEnabled {
		return fmt.Errorf("no resources to start")
	}

	// Check fixed port conflicts for resources being launched.
	if conflicts := ports.CheckFixed(o.cfg); len(conflicts) > 0 {
		var msgs []string
		for _, c := range conflicts {
			msgs = append(msgs, c.Error())
		}
		return fmt.Errorf("port conflicts:\n  %s", strings.Join(msgs, "\n  "))
	}

	if err := os.MkdirAll(o.stateDir, 0o755); err != nil {
		return fmt.Errorf("creating state dir: %w", err)
	}

	if err := supervisor.WritePIDFile(o.stateDir); err != nil {
		return fmt.Errorf("writing PID file: %w", err)
	}
	defer supervisor.RemovePIDFile(o.stateDir)

	printBanner(o.id.Slug, o.cfg, launchOrder)

	// ================================================================
	// Phase 0.5: Dashboard + OTel receiver
	// ================================================================
	allocated := make(map[uint16]bool)
	var dashboardState *state.DashboardState

	if dashEnabled {
		dc := o.cfg.Dashboard
		otelCfg := dc.OTel
		if otelCfg == nil {
			otelCfg = config.DefaultOTelConfig()
		}

		dashPort, err := ports.ResolveFixed(dc.Port, "dashboard", allocated)
		if err != nil {
			return fmt.Errorf("resolving dashboard port: %w", err)
		}
		grpcPort, err := ports.ResolveFixed(otelCfg.GRPCPort, "otel-grpc", allocated)
		if err != nil {
			return fmt.Errorf("resolving OTel gRPC port: %w", err)
		}
		httpPort, err := ports.ResolveFixed(otelCfg.HTTPPort, "otel-http", allocated)
		if err != nil {
			return fmt.Errorf("resolving OTel HTTP port: %w", err)
		}
		dashboardState = &state.DashboardState{
			DashboardPort: dashPort,
			GRPCPort:      grpcPort,
			HTTPPort:      httpPort,
		}

		retention := 1 * time.Hour
		if otelCfg.Retention != "" {
			if d, err2 := time.ParseDuration(otelCfg.Retention); err2 == nil {
				retention = d
			}
		}
		maxSpans := otelCfg.TraceBuffer
		if maxSpans == 0 {
			maxSpans = 10000
		}
		maxLogs := otelCfg.LogBuffer
		if maxLogs == 0 {
			maxLogs = 100000
		}
		maxMetrics := otelCfg.MetricBuffer
		if maxMetrics == 0 {
			maxMetrics = 50000
		}

		otelStore := otel.NewStore(maxSpans, maxLogs, maxMetrics, retention)
		eventsCh := make(chan otel.WSEvent, 2048)

		// Bridge orchestrator events (service status changes) onto the WS
		// channel so the dashboard sees supervisor lifecycle transitions.
		statusSub := o.eventBroadcast.Subscribe(256)
		go func() {
			for ev := range statusSub {
				if ev.Kind != events.KindServiceStatusChange {
					continue
				}
				wsEv := otel.MakeServiceStatusEvent(otel.ServiceStatusChangePayload{
					Service: ev.Service,
					Status:  ev.Status,
				})
				select {
				case eventsCh <- wsEv:
				default:
				}
			}
		}()

		recv := otel.NewReceiver(otelStore, eventsCh)
		if err := recv.StartHTTP(ctx, httpPort); err != nil {
			return fmt.Errorf("starting OTLP HTTP receiver: %w", err)
		}
		if err := recv.StartGRPC(ctx, grpcPort); err != nil {
			return fmt.Errorf("starting OTLP gRPC receiver: %w", err)
		}

		dashSrv := dashboard.NewServer(dashboard.ServerConfig{
			Port:       dashPort,
			ConfigPath: o.cfgPath,
			StateDir:   o.stateDir,
			Store:      otelStore,
			Events:     eventsCh,
		})
		go func() {
			if err := dashSrv.Start(ctx); err != nil && ctx.Err() == nil {
				fmt.Fprintf(os.Stderr, "dashboard: %v\n", err)
			}
		}()
	}

	// ================================================================
	// Phase 0.6: Reserve OIDC port (stub — implemented in Phase 6)
	// ================================================================
	var oidcPort uint16
	if oidcEnabled {
		p, err := ports.ResolveFixed(o.cfg.OIDC.Port, "oidc", allocated)
		if err != nil {
			return fmt.Errorf("resolving OIDC port: %w", err)
		}
		oidcPort = p
	}

	// ================================================================
	// Phase 1: Docker network
	// ================================================================
	dockerStates := make(map[string]state.DockerState)
	composeStates := make(map[string]state.ComposeServiceState)
	var networkName *string
	var clusterState *state.ClusterState
	var dockerMgr *docker.Manager

	hasDockerResources := false
	for _, node := range launchOrder {
		if node.Kind == graph.KindDocker || node.Kind == graph.KindCompose ||
			node.Kind == graph.KindClusterImage || node.Kind == graph.KindClusterDeploy {
			hasDockerResources = true
			break
		}
	}
	if o.cfg.Cluster != nil {
		hasDockerResources = true
	}

	if hasDockerResources {
		mgr, err := docker.New(o.id.Slug)
		if err != nil {
			return fmt.Errorf("connecting to Docker: %w", err)
		}
		dockerMgr = mgr
		if err := dockerMgr.EnsureNetwork(ctx); err != nil {
			return fmt.Errorf("ensuring Docker network: %w", err)
		}
		nn := dockerMgr.NetworkName()
		networkName = &nn
	}

	// ================================================================
	// Phase 2: Compose services
	// ================================================================
	if o.cfg.Compose != nil {
		composeSvcs := make([]string, 0)
		for _, node := range launchOrder {
			if node.Kind == graph.KindCompose {
				composeSvcs = append(composeSvcs, node.Name)
			}
		}
		if len(composeSvcs) > 0 {
			cfgDir := filepath.Dir(o.cfgPath)
			composeFile := filepath.Join(cfgDir, o.cfg.Compose.File)
			envFile := ""
			if o.cfg.Compose.EnvFile != nil {
				envFile = *o.cfg.Compose.EnvFile
			}
			if err := compose.Up(composeFile, o.id.Slug, composeSvcs, envFile); err != nil {
				return fmt.Errorf("compose up: %w", err)
			}
			containers, err := compose.PS(composeFile, o.id.Slug)
			if err != nil {
				return fmt.Errorf("compose ps: %w", err)
			}
			for _, c := range containers {
				if !contains(composeSvcs, c.Service) {
					continue
				}
				var port *uint16
				if len(c.Publishers) > 0 && c.Publishers[0].PublishedPort != 0 {
					p := c.Publishers[0].PublishedPort
					port = &p
					allocated[p] = true
				}
				composeStates[c.Service] = state.ComposeServiceState{
					ContainerID:   c.ID,
					ContainerName: c.Name,
					Port:          port,
				}
				// Bridge to devrig network.
				if dockerMgr != nil {
					_ = dockerMgr.ConnectContainerToNetwork(ctx, c.ID, []string{c.Service})
				}
				// Optional per-service ready check.
				if check := o.cfg.Compose.ReadyChecks[c.Service]; check != nil && dockerMgr != nil {
					if err := docker.RunReadyCheck(ctx, dockerMgr.Client(), c.ID, check, port, c.Service); err != nil {
						return fmt.Errorf("compose service %s: %w", c.Service, err)
					}
				}
				o.eventBroadcast.Send(events.TelemetryEvent{
					Kind: events.KindServiceStatusChange, Service: c.Service, Status: "running",
				})
			}
		}
	}

	// ================================================================
	// Phase 3: Infrastructure containers (in dependency order)
	// ================================================================
	if dockerMgr != nil {
		cfgDir := filepath.Dir(o.cfgPath)
		for _, node := range launchOrder {
			if node.Kind != graph.KindDocker {
				continue
			}
			dockerCfg, ok := o.cfg.Docker[node.Name]
			if !ok {
				continue
			}
			var prevDocker *state.DockerState
			if prevState != nil {
				if ds, ok := prevState.Docker[node.Name]; ok {
					prevDocker = &ds
				}
			}
			ds, err := dockerMgr.StartService(ctx, node.Name, &dockerCfg, prevDocker, allocated, cfgDir)
			if err != nil {
				return fmt.Errorf("starting docker service %s: %w", node.Name, err)
			}
			dockerStates[node.Name] = ds
			// Start log stream in background.
			go docker.StreamContainerLogs(ctx, dockerMgr.Client(), ds.ContainerID, node.Name, o.logBroadcast)
			o.eventBroadcast.Send(events.TelemetryEvent{
				Kind: events.KindServiceStatusChange, Service: node.Name, Status: "running",
			})
		}
	}

	// ================================================================
	// Phase 3.5: k3d cluster
	// ================================================================
	if o.cfg.Cluster != nil {
		net := ""
		if networkName != nil {
			net = *networkName
		}
		clusterMgr := cluster.NewManager(o.cfg.Cluster, o.id.Slug, o.stateDir, net)
		cs, err := clusterMgr.Ensure(ctx)
		if err != nil {
			return fmt.Errorf("cluster: %w", err)
		}
		if cs.RegistryPort != nil {
			if err := cluster.WaitForRegistry(ctx, *cs.RegistryPort); err != nil {
				return fmt.Errorf("cluster: wait for registry: %w", err)
			}
		}

		// Build standalone images.
		for imgName, imgCfg := range o.cfg.Cluster.Images {
			ic := imgCfg
			if _, err := cluster.BuildImage(ctx, imgName, &ic, cs); err != nil {
				return fmt.Errorf("cluster image %s: %w", imgName, err)
			}
		}

		// Install addons.
		if len(o.cfg.Cluster.Addons) > 0 {
			if err := cluster.InstallAddons(ctx, o.cfg.Cluster.Addons, cs, o.stateDir); err != nil {
				return fmt.Errorf("cluster addons: %w", err)
			}
		}

		// Inject Fluent Bit log collector if enabled.
		if o.cfg.Cluster.Logs != nil && o.cfg.Cluster.Logs.Enabled && o.cfg.Cluster.Logs.Collector {
			otlpEndpoint := fmt.Sprintf("localhost:%d", o.cfg.Dashboard.OTel.HTTPPort.AsFixed())
			manifestPath, err := cluster.WriteLogCollectorManifest(otlpEndpoint, o.cfg.Cluster.Logs, o.stateDir)
			if err != nil {
				return fmt.Errorf("cluster log collector: %w", err)
			}
			if err := cluster.KubectlApply(ctx, cs.KubeconfigPath, manifestPath); err != nil {
				return fmt.Errorf("cluster log collector apply: %w", err)
			}
		}

		// Build and deploy cluster services.
		for svcName, svcCfg := range o.cfg.Cluster.Deploy {
			sc := svcCfg
			if err := cluster.BuildAndDeploy(ctx, svcName, &sc, cs, o.stateDir); err != nil {
				return fmt.Errorf("cluster deploy %s: %w", svcName, err)
			}
		}

		clusterState = cs
	}

	// Persist partial state (docker + compose running; cluster pending).
	partial := &state.ProjectState{
		Slug:            o.id.Slug,
		ConfigPath:      o.cfgPath,
		StartedAt:       time.Now(),
		Services:        make(map[string]state.ServiceState),
		Docker:          dockerStates,
		ComposeServices: composeStates,
		NetworkName:     networkName,
		Cluster:         clusterState,
		Dashboard:       dashboardState,
		PID:             os.Getpid(),
	}
	if err := partial.Save(o.stateDir); err != nil {
		return fmt.Errorf("saving partial state: %w", err)
	}

	// ================================================================
	// Phase 4: Resolve ports, template vars, DEVRIG_* env
	// ================================================================
	resolvedPorts := make(map[string]uint16)

	if dashboardState != nil {
		resolvedPorts["dashboard"] = dashboardState.DashboardPort
		resolvedPorts["otel-grpc"] = dashboardState.GRPCPort
		resolvedPorts["otel-http"] = dashboardState.HTTPPort
	}

	// Resolve service ports (sticky auto-port support).
	for _, node := range launchOrder {
		if node.Kind != graph.KindService {
			continue
		}
		svc, ok := o.cfg.Services[node.Name]
		if !ok || svc.Port == nil {
			continue
		}
		var prevPort *uint16
		prevAuto := false
		if prevState != nil {
			if ps, ok := prevState.Services[node.Name]; ok {
				prevPort = ps.Port
				prevAuto = ps.PortAuto
			}
		}
		p, isAuto, err := ports.Resolve(svc.Port, fmt.Sprintf("service:%s", node.Name), prevPort, prevAuto, allocated)
		if err != nil {
			return fmt.Errorf("resolving port for service %s: %w", node.Name, err)
		}
		resolvedPorts[fmt.Sprintf("service:%s", node.Name)] = p
		_ = isAuto
	}

	// Also resolve fixed ports for services not being launched (for template refs).
	for name, svc := range o.cfg.Services {
		key := fmt.Sprintf("service:%s", name)
		if _, done := resolvedPorts[key]; done || svc.Port == nil {
			continue
		}
		if !svc.Port.IsAuto() {
			resolvedPorts[key] = svc.Port.AsFixed()
		} else if prevState != nil {
			if ps, ok := prevState.Services[name]; ok && ps.Port != nil {
				resolvedPorts[key] = *ps.Port
			}
		}
	}

	// Build template vars and resolve {{ }} in config.
	tvars := buildTemplateVars(o.cfg, resolvedPorts, dockerStates, composeStates, clusterState, oidcPort)
	if err := config.InterpolateConfig(o.cfg, tvars); err != nil {
		return fmt.Errorf("template resolution errors:\n%w", err)
	}

	// ================================================================
	// Phase 4.5: Bind OIDC provider
	// ================================================================
	if oidcEnabled {
		oidcSrv := oidc.New(o.cfg.OIDC, oidcPort, nil)
		go func() {
			if err := oidcSrv.Start(ctx); err != nil && ctx.Err() == nil {
				fmt.Fprintf(os.Stderr, "oidc: %v\n", err)
			}
		}()
	}

	// ================================================================
	// Phase 4.9: Save full state + register
	// ================================================================
	serviceStates := make(map[string]state.ServiceState)
	var launchNames []string
	for _, node := range launchOrder {
		if node.Kind != graph.KindService {
			continue
		}
		launchNames = append(launchNames, node.Name)
		svc := o.cfg.Services[node.Name]
		var p *uint16
		if port, ok := resolvedPorts[fmt.Sprintf("service:%s", node.Name)]; ok {
			cp := port
			p = &cp
		}
		portAuto := svc.Port != nil && svc.Port.IsAuto()
		starting := "starting"
		serviceStates[node.Name] = state.ServiceState{
			Port:     p,
			PortAuto: portAuto,
			Protocol: svc.Protocol,
			Phase:    &starting,
		}
	}

	projectState := &state.ProjectState{
		Slug:            o.id.Slug,
		ConfigPath:      o.cfgPath,
		StartedAt:       time.Now(),
		Services:        serviceStates,
		Docker:          dockerStates,
		ComposeServices: composeStates,
		NetworkName:     networkName,
		Cluster:         clusterState,
		Dashboard:       dashboardState,
		PID:             os.Getpid(),
	}
	if err := projectState.Save(o.stateDir); err != nil {
		return fmt.Errorf("saving project state: %w", err)
	}

	reg := registry.Load()
	reg.Register(registry.InstanceEntry{
		Slug:       o.id.Slug,
		ConfigPath: o.cfgPath,
		StateDir:   o.stateDir,
		StartedAt:  time.Now(),
	})
	if err := reg.Save(); err != nil {
		fmt.Fprintf(os.Stderr, "warning: failed to save instance registry: %v\n", err)
	}

	// ================================================================
	// Phase 4.6: Vite dev server (--dev mode)
	// ================================================================
	if devMode {
		webDir := filepath.Join(filepath.Dir(o.cfgPath), "web")
		if _, err := os.Stat(filepath.Join(webDir, "package.json")); err != nil {
			return fmt.Errorf("--dev requires web/ directory with package.json (at %s)", webDir)
		}
		go func() {
			runDev(ctx, webDir)
		}()
	}

	// ================================================================
	// Phase 5: Spawn service supervisors
	// ================================================================
	var supervisorWg sync.WaitGroup

	if len(launchNames) > 0 {
		logsDir := filepath.Join(o.stateDir, "logs")
		_ = os.MkdirAll(logsDir, 0o755)
		jsonlPath := filepath.Join(logsDir, "current.jsonl")
		jsonlFile, _ := os.Create(jsonlPath)

		// Single goroutine drains the log broadcaster into the JSONL file.
		logSub := o.logBroadcast.Subscribe(4096)
		go func() {
			var w *bufio.Writer
			if jsonlFile != nil {
				w = bufio.NewWriter(jsonlFile)
			}
			for ev := range logSub {
				if w != nil {
					if b, err := json.Marshal(ev); err == nil {
						_, _ = w.Write(b)
						_ = w.WriteByte('\n')
						_ = w.Flush()
					}
				}
			}
			if jsonlFile != nil {
				_ = jsonlFile.Close()
			}
		}()

		for _, name := range launchNames {
			svc := o.cfg.Services[name]
			policy := supervisor.PolicyFromConfig(svc.Restart)
			workingDir := resolveWorkingDir(svc.Path, o.cfgPath)
			env := buildServiceEnv(name, o.cfg, resolvedPorts, composeStates, dashboardState)

			sup := supervisor.New(
				name,
				svc.Command,
				workingDir,
				env,
				policy,
				o.logBroadcast,
				o.eventBroadcast,
				o.stateDir,
			)

			supervisorWg.Add(1)
			go func(s *supervisor.Supervisor) {
				defer supervisorWg.Done()
				if err := s.Run(ctx); err != nil && ctx.Err() == nil {
					fmt.Fprintf(os.Stderr, "service %s: %v\n", s.Name, err)
					state.UpdateServicePhase(o.stateDir, s.Name, "failed")
				} else {
					state.UpdateServicePhase(o.stateDir, s.Name, "stopped")
				}
				o.eventBroadcast.Send(events.TelemetryEvent{
					Kind:    events.KindServiceStatusChange,
					Service: s.Name,
					Status:  "stopped",
				})
			}(sup)
		}
	}

	printStartupSummary(o.id.Slug, projectState, o.cfg, resolvedPorts)

	// ================================================================
	// Wait for SIGINT/SIGTERM or all services to exit
	// ================================================================
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, shutdownSignals...)

	// "All services exited" only applies when services were supervised. With a
	// dashboard-only config (no services) we stay up until a signal arrives —
	// the dashboard/OTel/OIDC servers are the workload.
	allDone := make(chan struct{})
	if len(launchNames) > 0 {
		go func() {
			supervisorWg.Wait()
			close(allDone)
		}()
	}

	select {
	case <-sigCh:
		fmt.Fprintln(os.Stderr, "\nShutting down...")
	case <-allDone:
		fmt.Fprintln(os.Stderr, "All services exited.")
	}

	// Cancel supervisors and wait up to 10s for clean exit.
	cancel()
	shutdownDone := make(chan struct{})
	go func() {
		supervisorWg.Wait()
		close(shutdownDone)
	}()

	timer := time.NewTimer(10 * time.Second)
	defer timer.Stop()
	select {
	case <-shutdownDone:
	case <-timer.C:
		fmt.Fprintln(os.Stderr, "Shutdown timed out — some processes may still be running.")
	case <-sigCh:
		fmt.Fprintln(os.Stderr, "\nForce shutdown.")
		os.Exit(130)
	}

	// Drain subscribers and close.
	o.logBroadcast.Close()
	o.eventBroadcast.Close()

	return nil
}

// Stop sends SIGTERM to the running devrig process via the PID file.
func (o *Orchestrator) Stop() error {
	st := state.Load(o.stateDir)
	if st == nil {
		return fmt.Errorf("no running project state found — is the project running?")
	}

	pidPath := filepath.Join(o.stateDir, "devrig.pid")
	data, err := os.ReadFile(pidPath)
	if err != nil {
		return fmt.Errorf("no PID file found — project may have been started by an older version")
	}

	var pid int
	if _, err := fmt.Sscanf(strings.TrimSpace(string(data)), "%d", &pid); err != nil {
		return fmt.Errorf("invalid PID file: %w", err)
	}

	proc, err := os.FindProcess(pid)
	if err != nil {
		return fmt.Errorf("finding process %d: %w", pid, err)
	}

	if err := sendStop(proc); err != nil {
		_ = os.Remove(pidPath)
		return fmt.Errorf("signalling process %d: %w", pid, err)
	}

	fmt.Printf("Sent stop signal to devrig (pid %d)\n", pid)

	// Poll for exit (up to 10s).
	for range 100 {
		time.Sleep(100 * time.Millisecond)
		if !isAlive(proc) {
			return nil // Process exited
		}
	}
	return nil
}

// Delete stops the project, removes all managed resources, and unregisters it.
func (o *Orchestrator) Delete() error {
	_ = o.Stop() // best-effort

	// TODO Phase 3: stop docker containers
	if o.cfg != nil && o.cfg.Cluster != nil {
		mgr := cluster.NewManager(o.cfg.Cluster, o.id.Slug, o.stateDir, "")
		_ = mgr.Delete(context.Background())
	}

	if err := state.Remove(o.stateDir); err != nil {
		fmt.Fprintf(os.Stderr, "warning: removing state: %v\n", err)
	}
	if o.stateDir != "" {
		_ = os.RemoveAll(o.stateDir)
	}

	reg := registry.Load()
	reg.Unregister(o.id.Slug)
	if err := reg.Save(); err != nil {
		fmt.Fprintf(os.Stderr, "warning: saving registry: %v\n", err)
	}
	return nil
}

// applyFilter returns the subset of nodes that are in filter + transitive deps.
// An empty filter returns all nodes.
func (o *Orchestrator) applyFilter(order []graph.Node, filter []string) ([]graph.Node, error) {
	if len(filter) == 0 {
		return order, nil
	}

	for _, name := range filter {
		if _, ok := o.cfg.Services[name]; !ok {
			avail := make([]string, 0, len(o.cfg.Services))
			for k := range o.cfg.Services {
				avail = append(avail, k)
			}
			return nil, fmt.Errorf("unknown service %q (available: %v)", name, avail)
		}
	}

	needed := make(map[string]bool)
	for _, n := range filter {
		needed[n] = true
	}
	// Expand transitive dependencies.
	changed := true
	for changed {
		changed = false
		for name := range needed {
			if svc, ok := o.cfg.Services[name]; ok {
				for _, dep := range svc.DependsOn {
					if !needed[dep] {
						needed[dep] = true
						changed = true
					}
				}
			}
			if d, ok := o.cfg.Docker[name]; ok {
				for _, dep := range d.DependsOn {
					if !needed[dep] {
						needed[dep] = true
						changed = true
					}
				}
			}
		}
	}

	var out []graph.Node
	for _, node := range order {
		if needed[node.Name] {
			out = append(out, node)
		}
	}
	return out, nil
}

// buildTemplateVars assembles the TemplateVars used for {{ }} interpolation.
func buildTemplateVars(
	cfg *config.Config,
	resolvedPorts map[string]uint16,
	dockerStates map[string]state.DockerState,
	composeStates map[string]state.ComposeServiceState,
	clusterState *state.ClusterState,
	oidcPort uint16,
) *config.TemplateVars {
	tv := &config.TemplateVars{
		ProjectName: cfg.Project.Name,
	}

	if p, ok := resolvedPorts["dashboard"]; ok {
		cp := p
		tv.DashboardPort = &cp
	}
	if p, ok := resolvedPorts["otel-grpc"]; ok {
		cp := p
		tv.OTelGRPCPort = &cp
	}
	if p, ok := resolvedPorts["otel-http"]; ok {
		cp := p
		tv.OTelHTTPPort = &cp
	}

	if oidcPort != 0 {
		cp := oidcPort
		tv.OIDCPort = &cp
		issuer := fmt.Sprintf("http://localhost:%d", oidcPort)
		tv.OIDCIssuer = &issuer
	}

	tv.ServicePorts = make(map[string]uint16)
	for name := range cfg.Services {
		if p, ok := resolvedPorts[fmt.Sprintf("service:%s", name)]; ok {
			tv.ServicePorts[name] = p
		}
	}

	tv.DockerPorts = make(map[string]uint16)
	tv.DockerNamedPorts = make(map[string]map[string]uint16)
	for name, ds := range dockerStates {
		if ds.Port != nil {
			tv.DockerPorts[name] = *ds.Port
		}
		if len(ds.NamedPorts) > 0 {
			tv.DockerNamedPorts[name] = ds.NamedPorts
		}
	}

	tv.ComposePorts = make(map[string]uint16)
	for name, cs := range composeStates {
		if cs.Port != nil {
			tv.ComposePorts[name] = *cs.Port
		}
	}

	if clusterState != nil {
		name := clusterState.ClusterName
		tv.ClusterName = &name
		kc := clusterState.KubeconfigPath
		tv.ClusterKubeconfig = &kc
		if clusterState.RegistryName != nil {
			reg := fmt.Sprintf("%s:5000", *clusterState.RegistryName)
			tv.ClusterRegistry = &reg
		}
		if clusterState.RegistryPort != nil {
			rh := fmt.Sprintf("localhost:%d", *clusterState.RegistryPort)
			tv.ClusterRegistryHost = &rh
		}
	}

	return tv
}

// buildServiceEnv builds the env map for a single service, injecting
// DEVRIG_* discovery vars and OTel endpoint vars.
func buildServiceEnv(
	name string,
	cfg *config.Config,
	resolvedPorts map[string]uint16,
	composeStates map[string]state.ComposeServiceState,
	dashState *state.DashboardState,
) map[string]string {
	env := make(map[string]string)

	// Service-defined env vars.
	svc := cfg.Services[name]
	for k, v := range svc.Env {
		env[k] = v
	}

	// DEVRIG_* discovery for other services (fixed or already-resolved ports).
	for svcName := range cfg.Services {
		if svcName == name {
			continue
		}
		upper := strings.ToUpper(strings.ReplaceAll(svcName, "-", "_"))
		env[fmt.Sprintf("DEVRIG_%s_HOST", upper)] = "localhost"
		if p, ok := resolvedPorts[fmt.Sprintf("service:%s", svcName)]; ok {
			env[fmt.Sprintf("DEVRIG_%s_PORT", upper)] = fmt.Sprint(p)
			env[fmt.Sprintf("DEVRIG_%s_URL", upper)] = fmt.Sprintf("http://localhost:%d", p)
		}
	}

	// DEVRIG_* for docker services (stub — ports will be set in Phase 3).

	// DEVRIG_* for compose services.
	for csName, cs := range composeStates {
		upper := strings.ToUpper(strings.ReplaceAll(csName, "-", "_"))
		env[fmt.Sprintf("DEVRIG_%s_HOST", upper)] = "localhost"
		if cs.Port != nil {
			env[fmt.Sprintf("DEVRIG_%s_PORT", upper)] = fmt.Sprint(*cs.Port)
			env[fmt.Sprintf("DEVRIG_%s_URL", upper)] = fmt.Sprintf("http://localhost:%d", *cs.Port)
		}
	}

	// OTel + dashboard env vars.
	if dashState != nil {
		env["OTEL_EXPORTER_OTLP_ENDPOINT"] = fmt.Sprintf("http://localhost:%d", dashState.HTTPPort)
		env["OTEL_SERVICE_NAME"] = name
		env["DEVRIG_DASHBOARD_URL"] = fmt.Sprintf("http://localhost:%d", dashState.DashboardPort)
	}

	// Self port.
	if p, ok := resolvedPorts[fmt.Sprintf("service:%s", name)]; ok {
		env["PORT"] = fmt.Sprint(p)
	}

	return env
}

func contains(slice []string, s string) bool {
	for _, v := range slice {
		if v == s {
			return true
		}
	}
	return false
}

func resolveWorkingDir(path *string, cfgPath string) string {
	if path == nil {
		return filepath.Dir(cfgPath)
	}
	p := expandHome(*path)
	if filepath.IsAbs(p) {
		return p
	}
	return filepath.Join(filepath.Dir(cfgPath), p)
}

func expandHome(p string) string {
	if strings.HasPrefix(p, "~/") {
		home, _ := os.UserHomeDir()
		return filepath.Join(home, p[2:])
	}
	return p
}

func runDev(ctx context.Context, webDir string) {
	// Import cycle risk: keep this inline to avoid importing dashboard pkg.
	// The Vite dev server will be started properly once the dashboard pkg exists.
	cmd := buildDevCmd(ctx, webDir)
	if cmd == nil {
		return
	}
	_ = cmd.Run()
}

// printBanner outputs the startup banner to stdout.
func printBanner(slug string, cfg *config.Config, order []graph.Node) {
	fmt.Printf("\n  devrig  ·  %s\n\n", slug)
	for _, node := range order {
		switch node.Kind {
		case graph.KindService:
			fmt.Printf("  ◦ %s\n", node.Name)
		case graph.KindDocker:
			fmt.Printf("  ◦ [docker] %s\n", node.Name)
		case graph.KindCompose:
			fmt.Printf("  ◦ [compose] %s\n", node.Name)
		case graph.KindClusterImage:
			fmt.Printf("  ◦ [image] %s\n", node.Name)
		case graph.KindClusterDeploy:
			fmt.Printf("  ◦ [cluster] %s\n", node.Name)
		}
	}
	if cfg.Dashboard != nil {
		fmt.Printf("  ◦ [dashboard]\n")
	}
	fmt.Println()
}

// printStartupSummary outputs the post-startup summary table.
func printStartupSummary(slug string, ps *state.ProjectState, cfg *config.Config, resolvedPorts map[string]uint16) {
	fmt.Printf("\n  ╭─ %s\n", slug)

	for name, svc := range ps.Services {
		port := ""
		if p, ok := resolvedPorts[fmt.Sprintf("service:%s", name)]; ok {
			proto := "http"
			if svcCfg, ok2 := cfg.Services[name]; ok2 && svcCfg.Protocol != nil {
				proto = *svcCfg.Protocol
			}
			port = fmt.Sprintf("  %s://localhost:%d", proto, p)
		}
		phase := "starting"
		if svc.Phase != nil {
			phase = *svc.Phase
		}
		fmt.Printf("  │  %-20s  %s%s\n", name, phase, port)
	}

	if ps.Dashboard != nil {
		fmt.Printf("  │  %-20s  http://localhost:%d\n", "[dashboard]", ps.Dashboard.DashboardPort)
	}

	fmt.Println("  ╰─")
	fmt.Println()
}
