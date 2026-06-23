package cluster

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/steveyackey/devrig/internal/config"
)

// logCollectorAddonName is the synthetic addon name for the Fluent Bit log collector.
const logCollectorAddonName = "devrig-log-collector"

// WriteLogCollectorManifest writes a Fluent Bit DaemonSet manifest to
// stateDir/log-collector.yaml and returns the path.
func WriteLogCollectorManifest(
	otlpEndpoint string,
	logsCfg *config.ClusterLogsConfig,
	stateDir string,
) (string, error) {
	var nsFilter string
	if logsCfg.Namespaces.All {
		nsFilter = ""
	} else if len(logsCfg.Namespaces.List) > 0 {
		nsFilter = fmt.Sprintf("    Namespace  %s", strings.Join(logsCfg.Namespaces.List, " "))
	}

	var excludeNS, excludePods string
	for _, ns := range logsCfg.ExcludeNamespaces {
		excludeNS += fmt.Sprintf("    Exclude_Namespace  %s\n", ns)
	}
	for _, pod := range logsCfg.ExcludePods {
		excludePods += fmt.Sprintf("    Exclude_Path  */%s*/*\n", pod)
	}

	manifest := fmt.Sprintf(`---
apiVersion: v1
kind: Namespace
metadata:
  name: devrig-logging
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: fluent-bit
  namespace: devrig-logging
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: fluent-bit-read
rules:
- apiGroups: [""]
  resources: [namespaces, pods]
  verbs: [get, list, watch]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: fluent-bit-read
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: fluent-bit-read
subjects:
- kind: ServiceAccount
  name: fluent-bit
  namespace: devrig-logging
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: fluent-bit-config
  namespace: devrig-logging
data:
  fluent-bit.conf: |
    [SERVICE]
        Flush         1
        Daemon        Off
        Log_Level     warn
        Parsers_File  parsers.conf

    [INPUT]
        Name              tail
        Path              /var/log/containers/*.log
        Parser            docker
        Tag               kube.*
        Mem_Buf_Limit     5MB
        Skip_Long_Lines   On
%s%s%s
    [OUTPUT]
        Name        opentelemetry
        Match       *
        Host        %s
        Port        %s
        Logs_uri    /v1/logs

  parsers.conf: |
    [PARSER]
        Name   docker
        Format json
        Time_Key time
        Time_Format %%Y-%%m-%%dT%%H:%%M:%%S.%%L
        Time_Keep On
---
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: fluent-bit
  namespace: devrig-logging
spec:
  selector:
    matchLabels:
      app: fluent-bit
  template:
    metadata:
      labels:
        app: fluent-bit
    spec:
      serviceAccountName: fluent-bit
      tolerations:
      - key: node-role.kubernetes.io/control-plane
        effect: NoSchedule
      containers:
      - name: fluent-bit
        image: fluent/fluent-bit:3.2
        resources:
          limits:
            memory: 128Mi
        volumeMounts:
        - name: varlog
          mountPath: /var/log
        - name: config
          mountPath: /fluent-bit/etc/
      volumes:
      - name: varlog
        hostPath:
          path: /var/log
      - name: config
        configMap:
          name: fluent-bit-config
`,
		indentIf(nsFilter, "    "),
		indentIf(excludeNS, ""),
		indentIf(excludePods, ""),
		otlpHost(otlpEndpoint),
		otlpPort(otlpEndpoint),
	)

	path := filepath.Join(stateDir, "log-collector.yaml")
	if err := os.WriteFile(path, []byte(manifest), 0o644); err != nil {
		return "", fmt.Errorf("write log collector manifest: %w", err)
	}
	return path, nil
}

func indentIf(s, prefix string) string {
	if s == "" {
		return ""
	}
	return prefix + s + "\n"
}

func otlpHost(endpoint string) string {
	parts := strings.Split(endpoint, ":")
	if len(parts) >= 1 {
		return parts[0]
	}
	return "localhost"
}

func otlpPort(endpoint string) string {
	parts := strings.Split(endpoint, ":")
	if len(parts) >= 2 {
		return parts[len(parts)-1]
	}
	return "4318"
}
