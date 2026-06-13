// Package oidc runs an in-process yauth-backed OIDC provider for local dev.
// It is seeded from [oidc] config and binds on its own port; no Postgres is
// required — the backend is the yauth "memory" driver.
package oidc

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"time"

	"github.com/google/uuid"
	yauth "github.com/yackey-labs/yauth"
	"github.com/yackey-labs/yauth/auth"
	"github.com/yackey-labs/yauth/domain"
	"github.com/yackey-labs/yauth/yauthcfg"

	"github.com/steveyackey/devrig/internal/config"
)

// Server is an in-process OIDC provider.
type Server struct {
	cfg    *config.OIDCConfig
	port   uint16
	logger *slog.Logger
}

// New creates an OIDC server from the given config and resolved port.
func New(cfg *config.OIDCConfig, port uint16, logger *slog.Logger) *Server {
	if logger == nil {
		logger = slog.Default()
	}
	return &Server{cfg: cfg, port: port, logger: logger}
}

// Start builds the yauth instance, seeds users and clients, then listens.
// It blocks until ctx is cancelled.
func (s *Server) Start(ctx context.Context) error {
	issuer := fmt.Sprintf("http://localhost:%d", s.port)
	if s.cfg.Issuer != nil {
		issuer = *s.cfg.Issuer
	}

	hibpDisabled := false
	allowSignups := false

	yauthCfg := &yauthcfg.Config{
		Database: yauthcfg.DatabaseConfig{
			Driver: "memory",
		},
		Server: yauthcfg.ServerConfig{
			BaseURL:      issuer,
			AllowSignups: &allowSignups,
		},
		Plugins: yauthcfg.PluginsConfig{
			EmailPassword: yauthcfg.EmailPasswordPluginConfig{
				RequireEmailVerification: false,
				HIBPCheck:                &hibpDisabled,
			},
			OIDC: yauthcfg.OIDCPluginConfig{
				Enabled: true,
				Issuer:  issuer,
			},
			OAuth2Server: yauthcfg.OAuth2ServerPluginConfig{
				Enabled: true,
				Issuer:  issuer,
			},
		},
	}

	y, err := yauth.NewFromConfig(ctx, yauthCfg, yauth.WithConfigLogger(s.logger))
	if err != nil {
		return fmt.Errorf("oidc: build yauth: %w", err)
	}

	repo := y.Repo()
	if err := s.seedUsers(ctx, repo); err != nil {
		return fmt.Errorf("oidc: seed users: %w", err)
	}
	if err := s.seedClients(ctx, repo); err != nil {
		return fmt.Errorf("oidc: seed clients: %w", err)
	}

	mux := http.NewServeMux()
	y.Mount(mux, yauth.MountOptions{})

	ln, err := net.Listen("tcp", fmt.Sprintf(":%d", s.port))
	if err != nil {
		return fmt.Errorf("oidc: listen on port %d: %w", s.port, err)
	}

	srv := &http.Server{Handler: mux}
	go func() {
		<-ctx.Done()
		shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = srv.Shutdown(shutCtx)
	}()

	s.logger.Info("oidc provider started", "addr", ln.Addr().String(), "issuer", issuer)
	if serveErr := srv.Serve(ln); serveErr != nil && serveErr != http.ErrServerClosed {
		return fmt.Errorf("oidc: serve: %w", serveErr)
	}
	return nil
}

func (s *Server) seedUsers(ctx context.Context, repo interface {
	CreateUser(context.Context, domain.NewUser) (domain.User, error)
	UpsertPassword(context.Context, domain.NewPassword) error
}) error {
	for _, u := range s.cfg.Users {
		role := "user"
		if u.Role != nil {
			role = *u.Role
		}
		created, err := repo.CreateUser(ctx, domain.NewUser{
			ID:            uuid.New().String(),
			Email:         u.Email,
			DisplayName:   u.Name,
			EmailVerified: true,
			Role:          role,
			CreatedAt:     time.Now(),
			UpdatedAt:     time.Now(),
		})
		if err != nil {
			s.logger.Warn("oidc: seeding user (may already exist)", "email", u.Email, "err", err)
			continue
		}
		hash, err := auth.HashPassword(u.Password)
		if err != nil {
			return fmt.Errorf("hash password for %s: %w", u.Email, err)
		}
		if err := repo.UpsertPassword(ctx, domain.NewPassword{
			UserID:       created.ID,
			PasswordHash: hash,
		}); err != nil {
			return fmt.Errorf("set password for %s: %w", u.Email, err)
		}
	}
	return nil
}

func (s *Server) seedClients(ctx context.Context, repo interface {
	CreateOAuth2Client(context.Context, domain.NewOAuth2Client) error
}) error {
	for clientID, cc := range s.cfg.Clients {
		redirectURIsJSON, _ := json.Marshal(cc.RedirectURIs)

		grantTypes := cc.GrantTypes
		if len(grantTypes) == 0 {
			grantTypes = []string{"authorization_code", "refresh_token"}
		}
		grantTypesJSON, _ := json.Marshal(grantTypes)

		scopes := cc.Scopes
		if len(scopes) == 0 {
			scopes = []string{"openid", "profile", "email"}
		}
		scopesJSON, _ := json.Marshal(scopes)

		var secretHash *string
		if cc.ClientSecret != nil {
			h, err := auth.HashPassword(*cc.ClientSecret)
			if err != nil {
				return fmt.Errorf("hash client secret for %s: %w", clientID, err)
			}
			secretHash = &h
		}

		if err := repo.CreateOAuth2Client(ctx, domain.NewOAuth2Client{
			ID:               uuid.New().String(),
			ClientID:         clientID,
			ClientSecretHash: secretHash,
			RedirectURIs:     redirectURIsJSON,
			ClientName:       cc.ClientName,
			GrantTypes:       grantTypesJSON,
			Scopes:           scopesJSON,
			IsPublic:         cc.Public,
			CreatedAt:        time.Now(),
		}); err != nil {
			s.logger.Warn("oidc: seeding client (may already exist)", "client_id", clientID, "err", err)
		}
	}
	return nil
}
