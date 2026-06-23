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
	"sort"
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
	"github.com/steveyackey/devrig/internal/style"
	"github.com/steveyackey/devrig/internal/supervisor"
	"github.com/steveyackey/devrig/internal/tools"
)

// Orchestrator manages the full lifecycle of a devrig project.
type Orchestrator struct {
	cfg            *config.Config
	id             *identity.ProjectIdentity
	cfgPath        string
	stateDir       string
	logBroadcast   *events.Broadcaster
	eventBroadcast *events.Broadcaster
	clusterMgr     *cluster.Manager
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

	daemonStartMs, _ := supervisor.ProcStartTimeMs(os.Getpid())

	// Reap any service processes orphaned by a previous run whose devrig
	// process is no longer alive (crash, kill -9, power loss). Without this they
	// linger and hold ports. PID+start-time identity guards against killing an
	// unrelated process that reused a recorded PID.
	reconcileOrphans(prevState)

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
			Port:           dashPort,
			ConfigPath:     o.cfgPath,
			StateDir:       o.stateDir,
			Store:          otelStore,
			Events:         eventsCh,
			ClusterManager: o.clusterMgr,
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
			sp := style.NewSpinner("Starting " + node.Name)
			sp.Start()
			ds, err := dockerMgr.StartService(ctx, node.Name, &dockerCfg, prevDocker, allocated, cfgDir)
			if err != nil {
				sp.Fail("Starting " + node.Name + " failed")
				return fmt.Errorf("starting docker service %s: %w", node.Name, err)
			}
			sp.Done("Started " + node.Name)
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
		resolver := tools.ResolverFromConfig(o.cfg.Tools, true)
		o.clusterMgr = cluster.NewManager(o.cfg.Cluster, resolver, o.id.Slug, o.stateDir, filepath.Dir(o.cfgPath), net)
		sp := style.NewSpinner("Preparing cluster")
		sp.Start()
		cs, err := o.clusterMgr.Ensure(ctx)
		if err != nil {
			sp.Fail("Cluster setup failed")
			return fmt.Errorf("cluster: %w", err)
		}
		if cs.RegistryPort != nil {
			if err := cluster.WaitForRegistry(ctx, *cs.RegistryPort); err != nil {
				sp.Fail("Cluster registry not ready")
				return fmt.Errorf("cluster: wait for registry: %w", err)
			}
		}
		sp.Done("Cluster ready")

		// Build standalone images in dependency order so that build_args
		// referencing another image's tag resolve to an already-built image.
		cfgDir := filepath.Dir(o.cfgPath)
		imgOrder, err := cluster.ImageBuildOrder(o.cfg.Cluster.Images)
		if err != nil {
			return fmt.Errorf("cluster images: %w", err)
		}
		for _, imgName := range imgOrder {
			ic := o.cfg.Cluster.Images[imgName]
			sp := style.NewSpinner("Building image " + imgName)
			sp.Start()
			if _, err := cluster.BuildImage(ctx, imgName, &ic, cs, cfgDir); err != nil {
				sp.Fail("Building image " + imgName + " failed")
				return fmt.Errorf("cluster image %s: %w", imgName, err)
			}
			sp.Done("Built image " + imgName)
		}

		// Install addons. Addon `values` may reference cluster vars (e.g.
		// {{ cluster.registry }}, {{ cluster.image.NAME.tag }}). The main config
		// interpolation pass runs later (Phase 4), so resolve addon values here
		// using the cluster state that's now available (registry + image tags).
		if len(o.cfg.Cluster.Addons) > 0 {
			if err := config.InterpolateAddonValues(o.cfg.Cluster.Addons, addonTemplateVars(cs)); err != nil {
				return fmt.Errorf("cluster addon values:\n%w", err)
			}

			sp := style.NewSpinner("Installing addons")
			sp.Start()
			if err := cluster.InstallAddons(ctx, resolver, o.cfg.Cluster.Addons, cs, o.stateDir, cfgDir); err != nil {
				sp.Fail("Installing addons failed")
				return fmt.Errorf("cluster addons: %w", err)
			}
			sp.Done("Addons installed")
		}

		// Inject Fluent Bit log collector if enabled.
		if o.cfg.Cluster.Logs != nil && o.cfg.Cluster.Logs.Enabled && o.cfg.Cluster.Logs.Collector {
			// Use host.k3d.internal so pods can reach the host's OTEL collector.
			// k3d automatically creates this DNS entry pointing to the Docker host gateway.
			otlpEndpoint := fmt.Sprintf("host.k3d.internal:%d", o.cfg.Dashboard.OTel.HTTPPort.AsFixed())
			manifestPath, err := cluster.WriteLogCollectorManifest(otlpEndpoint, o.cfg.Cluster.Logs, o.stateDir)
			if err != nil {
				return fmt.Errorf("cluster log collector: %w", err)
			}
			if err := cluster.KubectlApply(ctx, resolver, cs.KubeconfigPath, manifestPath); err != nil {
				return fmt.Errorf("cluster log collector apply: %w", err)
			}
		}

		// Build and deploy cluster services.
		for svcName, svcCfg := range o.cfg.Cluster.Deploy {
			sc := svcCfg
			sp := style.NewSpinner("Deploying " + svcName)
			sp.Start()
			if err := cluster.BuildAndDeploy(ctx, resolver, svcName, &sc, cs, o.stateDir, cfgDir); err != nil {
				sp.Fail("Deploying " + svcName + " failed")
				return fmt.Errorf("cluster deploy %s: %w", svcName, err)
			}
			sp.Done("Deployed " + svcName)
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
		PIDStartTimeMs:  daemonStartMs,
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
		PIDStartTimeMs:  daemonStartMs,
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
	// handles are kept in launch (dependency) order so shutdown can drain them
	// in reverse — dependents stop before the dependencies they rely on.
	var handles []svcHandle

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

			// Per-service context so shutdown can stop services individually in
			// reverse dependency order.
			svcCtx, svcCancel := context.WithCancel(ctx)
			done := make(chan struct{})
			handles = append(handles, svcHandle{name: name, cancel: svcCancel, done: done})

			supervisorWg.Add(1)
			go func(s *supervisor.Supervisor) {
				defer supervisorWg.Done()
				defer close(done)
				if err := s.Run(svcCtx); err != nil && svcCtx.Err() == nil {
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

	o.liveStartupSummary(projectState, resolvedPorts)

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
		cancel() // end log-follow streams holding the Docker client before stopping containers
		stopDockerContainers(dockerMgr, dockerStates)
		o.logBroadcast.Close()
		o.eventBroadcast.Close()
		return nil
	}

	// Graceful drain in reverse dependency order: stop each service (and its
	// process tree) and wait for it before stopping the ones it depends on. A
	// per-service grace exceeds the supervisor's SIGTERM→SIGKILL window (5s).
	shutdownDone := make(chan struct{})
	go func() {
		const perServiceGrace = 8 * time.Second
		for i := len(handles) - 1; i >= 0; i-- {
			h := handles[i]
			h.cancel()
			select {
			case <-h.done:
			case <-time.After(perServiceGrace):
				fmt.Fprintf(os.Stderr, "service %s did not stop in time; continuing.\n", h.name)
			}
		}
		// Cancel the root context first: this ends the per-container log-follow
		// streams that hold a connection on the shared Docker client. Otherwise
		// the StopService calls below can deadlock waiting for a free connection
		// (the stream only releases on cancel). Then stop the containers.
		cancel()
		stopDockerContainers(dockerMgr, dockerStates)
		supervisorWg.Wait()
		close(shutdownDone)
	}()

	// Overall cap, plus a second signal forces an immediate exit.
	timer := time.NewTimer(45 * time.Second)
	defer timer.Stop()
	select {
	case <-shutdownDone:
	case <-timer.C:
		fmt.Fprintln(os.Stderr, "Shutdown timed out — some processes may still be running.")
	case <-sigCh:
		fmt.Fprintln(os.Stderr, "\nForce shutdown.")
		cancel()
		os.Exit(130)
	}

	// Drain subscribers and close.
	o.logBroadcast.Close()
	o.eventBroadcast.Close()

	return nil
}

// svcHandle lets the shutdown path stop one supervised service at a time, in
// reverse dependency order.
type svcHandle struct {
	name   string
	cancel context.CancelFunc
	done   chan struct{}
}

// stopDockerContainers stops (but does not remove) the project's Docker service
// containers on shutdown, so `docker ps` is clean afterward. They are stopped
// rather than removed so the next start is quick and named volumes (e.g.
// database data) persist; StartService recreates the container. Stops run
// concurrently under a bounded context so one slow container can't stall the
// shutdown. The k3d cluster is intentionally left running (use `devrig delete`).
func stopDockerContainers(mgr *docker.Manager, states map[string]state.DockerState) {
	if mgr == nil || len(states) == 0 {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	var wg sync.WaitGroup
	for name, ds := range states {
		wg.Add(1)
		go func(name string, ds state.DockerState) {
			defer wg.Done()
			if err := mgr.StopService(ctx, &ds); err != nil {
				fmt.Fprintf(os.Stderr, "stopping container %s: %v\n", name, err)
			}
		}(name, ds)
	}
	wg.Wait()
}

// reconcileOrphans kills service processes left running by a previous devrig
// run that is no longer alive. It is a no-op when there is no prior state or
// when the prior devrig is still running (a concurrent instance — left alone).
// PID+start-time identity ensures a recorded PID that has since been reused by
// an unrelated process is never killed.
func reconcileOrphans(prev *state.ProjectState) {
	if prev == nil {
		return
	}
	// If the previous devrig is still alive, its services aren't orphans.
	if prev.PID != 0 && prev.PID != os.Getpid() && supervisor.SameProcess(prev.PID, prev.PIDStartTimeMs) {
		fmt.Fprintf(os.Stderr, "warning: another devrig instance (pid %d) appears to be running for this project\n", prev.PID)
		return
	}
	for name, svc := range prev.Services {
		if svc.PID == 0 {
			continue
		}
		if supervisor.SameProcess(int(svc.PID), svc.StartTimeMs) {
			fmt.Fprintf(os.Stderr, "Reaping orphaned service %q (pid %d) from a previous run...\n", name, svc.PID)
			supervisor.KillTree(int(svc.PID), 3*time.Second)
		}
	}
}

// Stop signals the running devrig process to shut down (SIGTERM on Unix; a
// forceful tree kill on Windows), located via the PID file.
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

	// Guard against PID reuse: only signal if the live process is still the
	// devrig we recorded. Otherwise the PID now belongs to something unrelated.
	if !supervisor.SameProcess(pid, st.PIDStartTimeMs) {
		_ = os.Remove(pidPath)
		fmt.Printf("devrig (pid %d) is not running; cleaned up stale PID file.\n", pid)
		return nil
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

	ctx := context.Background()

	// Delete the k3d cluster first: its nodes attach to the devrig network, so
	// they must be gone before we can remove the network in CleanupAll.
	if o.cfg != nil && o.cfg.Cluster != nil {
		resolver := tools.ResolverFromConfig(o.cfg.Tools, false)
		mgr := cluster.NewManager(o.cfg.Cluster, resolver, o.id.Slug, o.stateDir, filepath.Dir(o.cfgPath), "")
		_ = mgr.Delete(ctx)
	}

	// Remove all Docker resources for this project — containers, volumes, and
	// the network — found by the devrig.project label.
	if dockerMgr, err := docker.New(o.id.Slug); err == nil {
		if err := dockerMgr.CleanupAll(ctx); err != nil {
			fmt.Fprintf(os.Stderr, "warning: cleaning up Docker resources: %v\n", err)
		}
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
		// For in-cluster services to reach the host OTEL collector
		endpoint := fmt.Sprintf("host.k3d.internal:%d", cp)
		tv.OTelEndpointGRPC = &endpoint
	}
	if p, ok := resolvedPorts["otel-http"]; ok {
		cp := p
		tv.OTelHTTPPort = &cp
		// For in-cluster services to reach the host OTEL collector
		endpoint := fmt.Sprintf("host.k3d.internal:%d", cp)
		tv.OTelEndpointHTTP = &endpoint
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
		cv := addonTemplateVars(clusterState)
		tv.ClusterName = cv.ClusterName
		tv.ClusterKubeconfig = cv.ClusterKubeconfig
		tv.ClusterRegistry = cv.ClusterRegistry
		tv.ClusterRegistryHost = cv.ClusterRegistryHost
		tv.ClusterImageTags = cv.ClusterImageTags
	}

	return tv
}

// addonTemplateVars builds a TemplateVars with just the cluster-level variables
// (name, kubeconfig, registry, per-image tags) available from the cluster
// state. Used to interpolate helm addon `values` during the cluster phase,
// before the full Phase 4 template-vars pass runs.
func addonTemplateVars(clusterState *state.ClusterState) *config.TemplateVars {
	tv := &config.TemplateVars{}
	if clusterState == nil {
		return tv
	}
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
	// cluster.image.<name>.tag — just the tag portion (e.g. "1234567890") of
	// each built image. ImageTag is the full ref ("localhost:5000/n:tag" or
	// "devrig-n:latest"); service env / addon values want only the tag.
	// (build_args interpolation uses the full ref separately.)
	tv.ClusterImageTags = make(map[string]string, len(clusterState.DeployedServices))
	for n, ds := range clusterState.DeployedServices {
		tag := ds.ImageTag
		if i := strings.LastIndex(tag, ":"); i != -1 {
			tag = tag[i+1:]
		}
		tv.ClusterImageTags[n] = tag
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
// kindBadge returns a short, colored tag for a resource kind.
func kindBadge(kind graph.ResourceKind) string {
	switch kind {
	case graph.KindService:
		return style.Cyan("service")
	case graph.KindDocker:
		return style.Blue("docker")
	case graph.KindCompose:
		return style.Blue("compose")
	case graph.KindClusterImage:
		return style.Magenta("image")
	case graph.KindClusterDeploy:
		return style.Magenta("cluster")
	default:
		return ""
	}
}

func printBanner(slug string, cfg *config.Config, order []graph.Node) {
	gl := style.G()
	fmt.Printf("\n  %s  %s  %s\n\n",
		style.BoldCyan("devrig"), style.Gray(gl.MidDot), style.Bold(slug))

	// Align the badge column.
	badgeW := 0
	for _, node := range order {
		if w := style.Width(kindBadge(node.Kind)); w > badgeW {
			badgeW = w
		}
	}
	if cfg.Dashboard != nil && style.Width(style.Green("dashboard")) > badgeW {
		badgeW = style.Width(style.Green("dashboard"))
	}

	line := func(badge, name string) {
		fmt.Printf("  %s %s  %s\n", style.Gray(gl.Bullet), style.PadRight(badge, badgeW), name)
	}
	for _, node := range order {
		line(kindBadge(node.Kind), node.Name)
	}
	if cfg.Dashboard != nil {
		line(style.Green("dashboard"), "")
	}
	fmt.Println()
}

// statusDot returns a colored status glyph + word for a service phase.
func statusDot(phase string) (glyph, label string) {
	gl := style.G()
	switch phase {
	case "running":
		return style.Green(gl.Running), style.Green("running")
	case "failed":
		return style.Red(gl.Failed), style.Red("failed")
	case "stopped":
		return style.Gray(gl.Stopped), style.Gray("stopped")
	default:
		return style.Yellow(gl.Pending), style.Yellow("starting")
	}
}

// liveStartupSummary prints the summary box and keeps it updated as services
// transition from "starting" to "running" (or "failed"). On a color TTY it
// redraws the box in place; when piped/CI it waits for services to settle, then
// prints the final box once. Returns after all services settle or a timeout.
func (o *Orchestrator) liveStartupSummary(ps *state.ProjectState, resolvedPorts map[string]uint16) {
	names := make([]string, 0, len(ps.Services))
	for name := range ps.Services {
		names = append(names, name)
	}
	sort.Strings(names)

	nameW := len("dashboard")
	for _, n := range names {
		if len(n) > nameW {
			nameW = len(n)
		}
	}

	// Current phase per service; seeded from saved state, updated from events.
	status := make(map[string]string, len(names))
	for _, n := range names {
		phase := "starting"
		if sv := ps.Services[n]; sv.Phase != nil {
			phase = *sv.Phase
		}
		status[n] = phase
	}

	render := func() string {
		rows := make([]string, 0, len(names)+1)
		for _, name := range names {
			glyph, label := statusDot(status[name])
			url := ""
			if p, ok := resolvedPorts[fmt.Sprintf("service:%s", name)]; ok {
				proto := "http"
				if svcCfg, ok2 := o.cfg.Services[name]; ok2 && svcCfg.Protocol != nil {
					proto = *svcCfg.Protocol
				}
				url = style.Link(fmt.Sprintf("%s://localhost:%d", proto, p))
			}
			rows = append(rows, fmt.Sprintf("%s  %s  %s  %s",
				glyph, style.Bold(style.PadRight(name, nameW)), style.PadRight(label, 8), url))
		}
		if ps.Dashboard != nil {
			glyph, label := statusDot("running")
			url := style.Link(fmt.Sprintf("http://localhost:%d", ps.Dashboard.DashboardPort))
			rows = append(rows, fmt.Sprintf("%s  %s  %s  %s",
				glyph, style.Bold(style.PadRight("dashboard", nameW)), style.PadRight(label, 8), url))
		}
		return style.Box(o.id.Slug, rows)
	}

	settled := func() bool {
		for _, n := range names {
			switch status[n] {
			case "running", "failed", "stopped":
			default:
				return false
			}
		}
		return true
	}

	box := render()
	lines := strings.Count(box, "\n")
	tty := style.ColorEnabled()

	fmt.Println()
	if tty {
		fmt.Print(box)
	}

	if len(names) > 0 && !settled() {
		sub := o.eventBroadcast.Subscribe(64)
		defer o.eventBroadcast.Unsubscribe(sub)
		timeout := time.NewTimer(20 * time.Second)
		defer timeout.Stop()
	wait:
		for !settled() {
			select {
			case ev, ok := <-sub:
				if !ok {
					break wait
				}
				if ev.Kind != events.KindServiceStatusChange {
					continue
				}
				if _, tracked := status[ev.Service]; !tracked || status[ev.Service] == ev.Status {
					continue
				}
				status[ev.Service] = ev.Status
				if tty {
					fmt.Printf("\x1b[%dA%s", lines, render()) // redraw box in place
				}
			case <-timeout.C:
				break wait
			}
		}
	}

	if !tty {
		fmt.Print(render())
	}
	fmt.Println()
}
