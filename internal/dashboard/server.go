// Package dashboard provides the HTTP API and WebSocket telemetry server.
package dashboard

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"sync"
	"time"

	"github.com/steveyackey/devrig/internal/cluster"
	"github.com/steveyackey/devrig/internal/otel"
	"github.com/steveyackey/devrig/internal/state"
)

// ServerConfig configures the dashboard server.
type ServerConfig struct {
	Port           uint16
	ConfigPath     string
	StateDir       string
	Store          *otel.Store
	Events         <-chan otel.WSEvent
	ClusterManager *cluster.Manager
}

// Server is the dashboard HTTP + WS server.
type Server struct {
	cfg    ServerConfig
	mux    *http.ServeMux
	eventsMu sync.RWMutex
	subs     []chan otel.WSEvent
}

// NewServer creates the dashboard server and registers all routes.
func NewServer(cfg ServerConfig) *Server {
	s := &Server{cfg: cfg, mux: http.NewServeMux()}
	s.registerRoutes()
	return s
}

// Start listens and serves until ctx is cancelled.
func (s *Server) Start(ctx context.Context) error {
	// Fan-out events to all WS subscribers.
	go s.fanOut(ctx)

	// Retention sweeper.
	go func() {
		t := time.NewTicker(30 * time.Second)
		defer t.Stop()
		for {
			select {
			case <-t.C:
				s.cfg.Store.SweepRetention()
			case <-ctx.Done():
				return
			}
		}
	}()

	srv := &http.Server{
		Addr:    fmt.Sprintf("0.0.0.0:%d", s.cfg.Port),
		Handler: s.mux,
	}
	ln, err := net.Listen("tcp", srv.Addr)
	if err != nil {
		return fmt.Errorf("dashboard: binding port %d: %w", s.cfg.Port, err)
	}
	go func() {
		<-ctx.Done()
		shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = srv.Shutdown(shutCtx)
	}()
	return srv.Serve(ln)
}

func (s *Server) fanOut(ctx context.Context) {
	for {
		select {
		case ev, ok := <-s.cfg.Events:
			if !ok {
				return
			}
			s.eventsMu.RLock()
			for _, ch := range s.subs {
				select {
				case ch <- ev:
				default:
				}
			}
			s.eventsMu.RUnlock()
		case <-ctx.Done():
			return
		}
	}
}

func (s *Server) subscribe() <-chan otel.WSEvent {
	ch := make(chan otel.WSEvent, 512)
	s.eventsMu.Lock()
	s.subs = append(s.subs, ch)
	s.eventsMu.Unlock()
	return ch
}

func (s *Server) unsubscribe(ch <-chan otel.WSEvent) {
	s.eventsMu.Lock()
	defer s.eventsMu.Unlock()
	for i, sub := range s.subs {
		if sub == ch {
			s.subs = append(s.subs[:i], s.subs[i+1:]...)
			close(sub)
			return
		}
	}
}

// --- route helpers ---

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}

func queryParam(r *http.Request, key, def string) string {
	if v := r.URL.Query().Get(key); v != "" {
		return v
	}
	return def
}

func queryInt(r *http.Request, key string, def int) int {
	v := r.URL.Query().Get(key)
	if v == "" {
		return def
	}
	var n int
	fmt.Sscanf(v, "%d", &n)
	return n
}

func queryFloat(r *http.Request, key string, def float64) float64 {
	v := r.URL.Query().Get(key)
	if v == "" {
		return def
	}
	var f float64
	fmt.Sscanf(v, "%f", &f)
	return f
}

// readStateFile reads state.json from StateDir.
func (s *Server) currentState() *state.ProjectState {
	return state.Load(s.cfg.StateDir)
}

// readConfigFile reads the raw config file.
func (s *Server) readConfigFile() ([]byte, error) {
	if s.cfg.ConfigPath == "" {
		return nil, fmt.Errorf("no config path configured")
	}
	return os.ReadFile(s.cfg.ConfigPath)
}
