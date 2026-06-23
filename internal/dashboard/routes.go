package dashboard

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/otel"
)

func (s *Server) registerRoutes() {
	s.mux.HandleFunc("/api/status", s.handleStatus)
	s.mux.HandleFunc("/api/traces", s.handleTraces)
	s.mux.HandleFunc("/api/traces/", s.handleTraceByID) // /api/traces/{id} and /api/traces/{id}/related
	s.mux.HandleFunc("/api/logs", s.handleLogs)
	s.mux.HandleFunc("/api/metrics", s.handleMetrics)
	s.mux.HandleFunc("/api/metrics/series", s.handleMetricSeries)
	s.mux.HandleFunc("/api/services", s.handleServices)
	s.mux.HandleFunc("/api/cluster", s.handleCluster)
	s.mux.HandleFunc("/api/config", s.handleConfig)
	s.mux.HandleFunc("/ws", s.handleWS)

	// SPA fallback — serve embedded or built assets.
	s.mux.HandleFunc("/", s.handleSPA)
}

func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, s.cfg.Store.Status())
}

func (s *Server) handleTraces(w http.ResponseWriter, r *http.Request) {
	status := queryParam(r, "status", "")
	q := &otel.SpanQuery{
		Service:       queryParam(r, "service", ""),
		TraceID:       queryParam(r, "trace_id", ""),
		OnlyError:     strings.EqualFold(status, "error") || queryParam(r, "only_error", "") == "true",
		OnlyOk:        strings.EqualFold(status, "ok"),
		Search:        queryParam(r, "search", ""),
		MinDurationMs: queryFloat(r, "min_duration_ms", 0),
		Limit:         queryInt(r, "limit", 100),
		Offset:        queryInt(r, "offset", 0),
	}
	writeJSON(w, s.cfg.Store.QueryTraces(q))
}

func (s *Server) handleTraceByID(w http.ResponseWriter, r *http.Request) {
	// Strip /api/traces/ prefix.
	rest := r.URL.Path[len("/api/traces/"):]
	if path.Base(rest) == "related" {
		traceID := path.Dir(rest)
		logs, metrics := s.cfg.Store.GetRelated(traceID)
		writeJSON(w, map[string]any{"logs": logs, "metrics": metrics})
		return
	}
	traceID := rest
	detail, ok := s.cfg.Store.GetTrace(traceID)
	if !ok {
		http.NotFound(w, r)
		return
	}
	writeJSON(w, detail)
}

func (s *Server) handleLogs(w http.ResponseWriter, r *http.Request) {
	q := &otel.LogQuery{
		Service:  queryParam(r, "service", ""),
		Severity: queryParam(r, "severity", ""),
		Search:   queryParam(r, "search", ""),
		TraceID:  queryParam(r, "trace_id", ""),
		Limit:    queryInt(r, "limit", 100),
		Offset:   queryInt(r, "offset", 0),
	}
	if sinceStr := queryParam(r, "since", ""); sinceStr != "" {
		t, err := time.Parse(time.RFC3339, sinceStr)
		if err == nil {
			q.Since = &t
		}
	}
	writeJSON(w, s.cfg.Store.QueryLogs(q))
}

func (s *Server) handleMetrics(w http.ResponseWriter, r *http.Request) {
	q := &otel.MetricQuery{
		Service:    queryParam(r, "service", ""),
		MetricName: queryParam(r, "metric_name", ""),
		Limit:      queryInt(r, "limit", 100),
		Offset:     queryInt(r, "offset", 0),
	}
	writeJSON(w, s.cfg.Store.QueryMetrics(q))
}

func (s *Server) handleMetricSeries(w http.ResponseWriter, r *http.Request) {
	q := &otel.MetricSeriesQuery{
		Service:    queryParam(r, "service", ""),
		MetricName: queryParam(r, "metric_name", ""),
	}
	writeJSON(w, s.cfg.Store.GetMetricSeries(q))
}

// serviceInfo matches the Rust ServiceInfo shape consumed by the dashboard.
type serviceInfo struct {
	Name      string  `json:"name"`
	Port      *uint16 `json:"port"`
	Kind      string  `json:"kind"`
	PortAuto  bool    `json:"port_auto"`
	Protocol  *string `json:"protocol,omitempty"`
	Phase     *string `json:"phase,omitempty"`
	ExitCode  *int    `json:"exit_code,omitempty"`
	AddonType *string `json:"addon_type,omitempty"`
	URL       *string `json:"url,omitempty"`
}

func (s *Server) handleServices(w http.ResponseWriter, r *http.Request) {
	result := []serviceInfo{}
	running := "running"

	if st := s.currentState(); st != nil {
		for name, ss := range st.Services {
			result = append(result, serviceInfo{
				Name: name, Port: ss.Port, Kind: "service",
				PortAuto: ss.PortAuto, Protocol: ss.Protocol,
				Phase: ss.Phase, ExitCode: ss.ExitCode,
			})
		}
		for name, ds := range st.Docker {
			ph := running
			result = append(result, serviceInfo{
				Name: name, Port: ds.Port, Kind: "docker",
				PortAuto: ds.PortAuto, Protocol: ds.Protocol, Phase: &ph,
			})
		}
		for name, cs := range st.ComposeServices {
			ph := running
			result = append(result, serviceInfo{
				Name: name, Port: cs.Port, Kind: "compose", Phase: &ph,
			})
		}
	}

	// Links, cluster addons, and cluster ports come from the config file.
	if cfg, _, err := config.Load(s.cfg.ConfigPath); err == nil {
		for name, url := range cfg.Links {
			u := url
			result = append(result, serviceInfo{
				Name: name, Port: parsePortFromURL(url), Kind: "link", URL: &u,
			})
		}
		if cfg.Cluster != nil {
			for name, addon := range cfg.Cluster.Addons {
				var port *uint16
				if pf := addon.ParsedPortForwards(); len(pf) > 0 {
					p := pf[0].Local
					port = &p
				}
				at := addon.Type
				result = append(result, serviceInfo{
					Name: name, Port: port, Kind: "addon", AddonType: &at,
				})
			}
			for _, mapping := range cfg.Cluster.Ports {
				host, _, ok := strings.Cut(mapping, ":")
				if !ok {
					continue
				}
				var p uint16
				if _, err := fmt.Sscanf(host, "%d", &p); err != nil || p == 0 {
					continue
				}
				hp := p
				result = append(result, serviceInfo{
					Name: "cluster:" + mapping, Port: &hp, Kind: "cluster-port",
				})
			}
		}
	}

	sort.Slice(result, func(i, j int) bool {
		if result[i].Kind != result[j].Kind {
			return result[i].Kind < result[j].Kind
		}
		return result[i].Name < result[j].Name
	})
	writeJSON(w, result)
}

// parsePortFromURL extracts the port from URLs like "http://localhost:8080/path".
func parsePortFromURL(url string) *uint16 {
	rest := url
	if _, after, ok := strings.Cut(url, "://"); ok {
		rest = after
	}
	hostPort, _, _ := strings.Cut(rest, "/")
	idx := strings.LastIndex(hostPort, ":")
	if idx == -1 {
		return nil
	}
	var p uint16
	if _, err := fmt.Sscanf(hostPort[idx+1:], "%d", &p); err != nil || p == 0 {
		return nil
	}
	return &p
}

func (s *Server) handleCluster(w http.ResponseWriter, r *http.Request) {
	st := s.currentState()
	if st == nil || st.Cluster == nil {
		writeJSON(w, nil)
		return
	}

	// Build response with cluster state
	resp := map[string]any{
		"cluster_name":      st.Cluster.ClusterName,
		"kubeconfig_path":   st.Cluster.KubeconfigPath,
		"deployed_services": st.Cluster.DeployedServices,
		"installed_addons":  st.Cluster.InstalledAddons,
		"k3d_version":       st.Cluster.K3dVersion,
	}

	if st.Cluster.RegistryName != nil {
		resp["registry_name"] = *st.Cluster.RegistryName
	}
	if st.Cluster.RegistryPort != nil {
		resp["registry_port"] = *st.Cluster.RegistryPort
	}

	// Fetch live pod information
	if s.cfg.ClusterManager != nil {
		ctx := r.Context()
		pods, err := s.cfg.ClusterManager.ListPods(ctx, st.Cluster.KubeconfigPath)
		if err == nil {
			resp["pods"] = pods
		}
		// Silently ignore errors — pods field will be absent if kubectl fails
	}

	writeJSON(w, resp)
}

func (s *Server) handleConfig(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		data, err := s.readConfigFile()
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		info, _ := os.Stat(s.cfg.ConfigPath)
		hash := strconv.FormatInt(info.ModTime().UnixNano(), 16)
		writeJSON(w, map[string]string{"content": string(data), "hash": hash})
	case http.MethodPut:
		var body struct {
			Content string `json:"content"`
			Hash    string `json:"hash"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			http.Error(w, "invalid request body", http.StatusBadRequest)
			return
		}
		info, err := os.Stat(s.cfg.ConfigPath)
		if err != nil {
			http.Error(w, "config file not found", http.StatusInternalServerError)
			return
		}
		currentHash := strconv.FormatInt(info.ModTime().UnixNano(), 16)
		if body.Hash != "" && body.Hash != currentHash {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusConflict)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": "config modified since last read"})
			return
		}
		if err := os.WriteFile(s.cfg.ConfigPath, []byte(body.Content), 0o644); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		info2, _ := os.Stat(s.cfg.ConfigPath)
		newHash := strconv.FormatInt(info2.ModTime().UnixNano(), 16)
		writeJSON(w, map[string]string{"content": body.Content, "hash": newHash})
	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleSPA(w http.ResponseWriter, r *http.Request) {
	serveSPA(w, r)
}
