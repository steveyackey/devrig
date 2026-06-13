package config

import (
	"bufio"
	"fmt"
	"os"
	"strings"
	"sync"
)

// SecretRegistry tracks expanded secret values so they can be masked in output.
type SecretRegistry struct {
	mu      sync.Mutex
	secrets []string
}

func (r *SecretRegistry) Track(value string) {
	if value == "" {
		return
	}
	r.mu.Lock()
	r.secrets = append(r.secrets, value)
	r.mu.Unlock()
}

func (r *SecretRegistry) Mask(s string) string {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, secret := range r.secrets {
		s = strings.ReplaceAll(s, secret, "****")
	}
	return s
}

func (r *SecretRegistry) ContainsSecret(s string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, secret := range r.secrets {
		if strings.Contains(s, secret) {
			return true
		}
	}
	return false
}

// ParseEnvFile parses a .env file into a map. Supports KEY=VALUE, KEY="VALUE",
// and KEY='VALUE' lines. Lines starting with # or blank lines are ignored.
func ParseEnvFile(path string) (map[string]string, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("open env file %s: %w", path, err)
	}
	defer f.Close()

	result := make(map[string]string)
	scanner := bufio.NewScanner(f)
	lineNo := 0
	for scanner.Scan() {
		lineNo++
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		idx := strings.IndexByte(line, '=')
		if idx < 0 {
			continue
		}
		key := strings.TrimSpace(line[:idx])
		val := strings.TrimSpace(line[idx+1:])
		val = unquote(val)
		result[key] = val
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("read env file %s: %w", path, err)
	}
	return result, nil
}

func unquote(s string) string {
	if len(s) >= 2 {
		if (s[0] == '"' && s[len(s)-1] == '"') || (s[0] == '\'' && s[len(s)-1] == '\'') {
			return s[1 : len(s)-1]
		}
	}
	return s
}

// ExpandEnvVars expands $VAR, ${VAR}, and $$ in s using the provided lookup
// order: envFileVars first, then os.Getenv. Returns the expanded string and
// whether any secret (non-literal) value was substituted.
func ExpandEnvVars(s string, envFileVars map[string]string, registry *SecretRegistry) (string, bool, error) {
	var sb strings.Builder
	wasSecret := false
	i := 0
	for i < len(s) {
		if s[i] != '$' {
			sb.WriteByte(s[i])
			i++
			continue
		}
		i++ // skip $
		if i >= len(s) {
			sb.WriteByte('$')
			continue
		}
		if s[i] == '$' {
			sb.WriteByte('$')
			i++
			continue
		}
		var varName string
		if s[i] == '{' {
			end := strings.IndexByte(s[i:], '}')
			if end < 0 {
				return "", false, fmt.Errorf("unterminated ${... in: %s", s)
			}
			varName = s[i+1 : i+end]
			i += end + 1
		} else {
			j := i
			for j < len(s) && isEnvChar(s[j]) {
				j++
			}
			varName = s[i:j]
			i = j
		}
		if varName == "" {
			sb.WriteByte('$')
			continue
		}
		val, ok := envFileVars[varName]
		if !ok {
			val, ok = os.LookupEnv(varName)
		}
		if !ok {
			return "", false, fmt.Errorf("undefined environment variable: $%s", varName)
		}
		if registry != nil {
			registry.Track(val)
		}
		wasSecret = true
		sb.WriteString(val)
	}
	return sb.String(), wasSecret, nil
}

func isEnvChar(c byte) bool {
	return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_'
}

// ExpandConfigEnvVars expands $VAR references throughout the config in-place:
// global env, service env, docker env, docker image, docker registry_auth,
// cluster registries.
func ExpandConfigEnvVars(cfg *Config, envFileVars map[string]string, reg *SecretRegistry) error {
	var errs []string

	expandMap := func(m map[string]string, context string) {
		for k, v := range m {
			expanded, _, err := ExpandEnvVars(v, envFileVars, reg)
			if err != nil {
				errs = append(errs, fmt.Sprintf("%s[%s]: %v", context, k, err))
				continue
			}
			m[k] = expanded
		}
	}

	expandStr := func(s *string, context string) {
		if s == nil {
			return
		}
		expanded, _, err := ExpandEnvVars(*s, envFileVars, reg)
		if err != nil {
			errs = append(errs, fmt.Sprintf("%s: %v", context, err))
			return
		}
		*s = expanded
	}

	expandMap(cfg.Env, "env")
	for name, svc := range cfg.Services {
		expandMap(svc.Env, fmt.Sprintf("services.%s.env", name))
		cfg.Services[name] = svc
	}
	for name, d := range cfg.Docker {
		expandMap(d.Env, fmt.Sprintf("docker.%s.env", name))
		expandStr(&d.Image, fmt.Sprintf("docker.%s.image", name))
		if d.RegistryAuth != nil {
			expandStr(&d.RegistryAuth.Username, fmt.Sprintf("docker.%s.registry_auth.username", name))
			expandStr(&d.RegistryAuth.Password, fmt.Sprintf("docker.%s.registry_auth.password", name))
		}
		cfg.Docker[name] = d
	}
	if cfg.Cluster != nil {
		for i := range cfg.Cluster.Registries {
			expandStr(&cfg.Cluster.Registries[i].URL, fmt.Sprintf("cluster.registries[%d].url", i))
			expandStr(&cfg.Cluster.Registries[i].Username, fmt.Sprintf("cluster.registries[%d].username", i))
			expandStr(&cfg.Cluster.Registries[i].Password, fmt.Sprintf("cluster.registries[%d].password", i))
		}
	}

	if len(errs) > 0 {
		return fmt.Errorf("env expansion errors:\n  %s", strings.Join(errs, "\n  "))
	}
	return nil
}
