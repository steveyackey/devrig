use crate::config::model::{ClusterLogsConfig, NamespaceFilter};

/// Name of the generated manifest file written to the state directory.
pub const MANIFEST_FILENAME: &str = "fluent-bit-log-collector.yaml";

/// Synthetic addon key used in the addons map (prefixed with `__` to avoid
/// collisions with user-defined addon names).
pub const ADDON_KEY: &str = "__devrig-log-collector";

/// Render a complete Kubernetes manifest for a Fluent Bit DaemonSet that
/// collects pod logs and forwards them to devrig's OTLP HTTP receiver.
pub fn render_fluent_bit_manifest(
    logs_config: &ClusterLogsConfig,
    otlp_endpoint: &str,
) -> String {
    let namespace_filters = build_namespace_filters(logs_config);
    let pod_filters = build_pod_filters(logs_config);

    format!(
        r#"---
apiVersion: v1
kind: Namespace
metadata:
  name: devrig-logs
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: fluent-bit
  namespace: devrig-logs
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: devrig-fluent-bit
rules:
  - apiGroups: [""]
    resources: ["namespaces", "pods"]
    verbs: ["get", "list", "watch"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: devrig-fluent-bit
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: devrig-fluent-bit
subjects:
  - kind: ServiceAccount
    name: fluent-bit
    namespace: devrig-logs
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: fluent-bit-config
  namespace: devrig-logs
data:
  fluent-bit.conf: |
    [SERVICE]
        Flush        1
        Log_Level    warn
        Parsers_File parsers.conf
        Parsers_Multiline_File parsers.conf

    [INPUT]
        Name             tail
        Tag              kube.*
        Path             /var/log/containers/*.log
        multiline.parser cri
        Refresh_Interval 5
        Mem_Buf_Limit    5MB
        Skip_Long_Lines  On

    [FILTER]
        Name          kubernetes
        Match         kube.*
        Kube_URL      https://kubernetes.default.svc:443
        Merge_Log     On
        Keep_Log      Off
        K8S-Logging.Parser On

    [FILTER]
        Name                  multiline
        Match                 kube.*
        multiline.key_content log
        multiline.parser      multiline-dotnet
{namespace_filters}{pod_filters}
    [OUTPUT]
        Name                 opentelemetry
        Match                kube.*
        Host                 {host}
        Port                 {port}
        Metrics_uri          /v1/metrics
        Logs_uri             /v1/logs
        Traces_uri           /v1/traces
        Log_response_payload False
        Tls                  Off
        Add_label            log.source otlp

  parsers.conf: |
    [PARSER]
        Name        cri
        Format      regex
        Regex       ^(?<time>[^ ]+) (?<stream>stdout|stderr) (?<logtag>[^ ]*) (?<log>.*)$
        Time_Key    time
        Time_Format %Y-%m-%dT%H:%M:%S.%L%z

    [MULTILINE_PARSER]
        Name          multiline-dotnet
        type          regex
        flush_timeout 1000
        rule          "start_state" "/^[^\s]/" "cont"
        rule          "cont"        "/^\s/"    "cont"
---
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: fluent-bit
  namespace: devrig-logs
  labels:
    app: fluent-bit
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
        - operator: Exists
      containers:
        - name: fluent-bit
          image: fluent/fluent-bit:3.2
          resources:
            requests:
              cpu: 10m
              memory: 15Mi
            limits:
              cpu: 100m
              memory: 64Mi
          volumeMounts:
            - name: varlog
              mountPath: /var/log
              readOnly: true
            - name: config
              mountPath: /fluent-bit/etc/
      volumes:
        - name: varlog
          hostPath:
            path: /var/log
        - name: config
          configMap:
            name: fluent-bit-config
"#,
        host = extract_host(otlp_endpoint),
        port = extract_port(otlp_endpoint),
        namespace_filters = namespace_filters,
        pod_filters = pod_filters,
    )
}

/// Build Fluent Bit FILTER directives for namespace inclusion/exclusion.
fn build_namespace_filters(config: &ClusterLogsConfig) -> String {
    match &config.namespaces {
        NamespaceFilter::List(namespaces) if !namespaces.is_empty() => {
            let pattern = format!("^({})$", namespaces.join("|"));
            format!(
                r#"
    [FILTER]
        Name    grep
        Match   kube.*
        Regex   $kubernetes['namespace_name'] {}
"#,
                pattern
            )
        }
        NamespaceFilter::All => {
            if let Some(excludes) = &config.exclude_namespaces {
                if !excludes.is_empty() {
                    let pattern = format!("^({})$", excludes.join("|"));
                    return format!(
                        r#"
    [FILTER]
        Name    grep
        Match   kube.*
        Exclude $kubernetes['namespace_name'] {}
"#,
                        pattern
                    );
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// Build Fluent Bit FILTER directives for pod name exclusion.
fn build_pod_filters(config: &ClusterLogsConfig) -> String {
    if let Some(pods) = &config.exclude_pods {
        if !pods.is_empty() {
            let pattern = format!("^({})$", pods.join("|"));
            return format!(
                r#"
    [FILTER]
        Name    grep
        Match   kube.*
        Exclude $kubernetes['pod_name'] {}
"#,
                pattern
            );
        }
    }
    String::new()
}

/// Extract host from an endpoint string like "host.k3d.internal:4318".
fn extract_host(endpoint: &str) -> &str {
    endpoint.rsplit_once(':').map(|(h, _)| h).unwrap_or(endpoint)
}

/// Extract port from an endpoint string like "host.k3d.internal:4318".
fn extract_port(endpoint: &str) -> &str {
    endpoint.rsplit_once(':').map(|(_, p)| p).unwrap_or("4318")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> ClusterLogsConfig {
        ClusterLogsConfig {
            enabled: true,
            collector: true,
            namespaces: NamespaceFilter::default(),
            exclude_namespaces: None,
            exclude_pods: None,
        }
    }

    #[test]
    fn render_default_manifest_contains_namespace() {
        let manifest = render_fluent_bit_manifest(&default_config(), "host.k3d.internal:4318");
        assert!(manifest.contains("kind: Namespace"));
        assert!(manifest.contains("name: devrig-logs"));
        assert!(manifest.contains("kind: DaemonSet"));
        assert!(manifest.contains("fluent/fluent-bit:3.2"));
    }

    #[test]
    fn render_manifest_with_otlp_endpoint() {
        let manifest = render_fluent_bit_manifest(&default_config(), "host.k3d.internal:4318");
        assert!(manifest.contains("Host                 host.k3d.internal"));
        assert!(manifest.contains("Port                 4318"));
    }

    #[test]
    fn namespace_filter_list() {
        let config = ClusterLogsConfig {
            namespaces: NamespaceFilter::List(vec!["default".to_string(), "app".to_string()]),
            ..default_config()
        };
        let filters = build_namespace_filters(&config);
        assert!(filters.contains("Regex"));
        assert!(filters.contains("^(default|app)$"));
    }

    #[test]
    fn namespace_filter_all_with_excludes() {
        let config = ClusterLogsConfig {
            namespaces: NamespaceFilter::All,
            exclude_namespaces: Some(vec!["kube-system".to_string(), "traefik".to_string()]),
            ..default_config()
        };
        let filters = build_namespace_filters(&config);
        assert!(filters.contains("Exclude"));
        assert!(filters.contains("^(kube-system|traefik)$"));
    }

    #[test]
    fn namespace_filter_all_no_excludes() {
        let config = ClusterLogsConfig {
            namespaces: NamespaceFilter::All,
            exclude_namespaces: None,
            ..default_config()
        };
        let filters = build_namespace_filters(&config);
        assert!(filters.is_empty());
    }

    #[test]
    fn pod_filter_with_excludes() {
        let config = ClusterLogsConfig {
            exclude_pods: Some(vec!["noisy-.*".to_string()]),
            ..default_config()
        };
        let filters = build_pod_filters(&config);
        assert!(filters.contains("Exclude"));
        assert!(filters.contains("^(noisy-.*)$"));
    }

    #[test]
    fn pod_filter_none() {
        let config = default_config();
        let filters = build_pod_filters(&config);
        assert!(filters.is_empty());
    }

    #[test]
    fn extract_host_and_port() {
        assert_eq!(extract_host("host.k3d.internal:4318"), "host.k3d.internal");
        assert_eq!(extract_port("host.k3d.internal:4318"), "4318");
        assert_eq!(extract_host("localhost"), "localhost");
        assert_eq!(extract_port("localhost"), "4318");
    }

    #[test]
    fn render_manifest_resource_limits() {
        let manifest = render_fluent_bit_manifest(&default_config(), "host.k3d.internal:4318");
        assert!(manifest.contains("cpu: 10m"));
        assert!(manifest.contains("memory: 15Mi"));
        assert!(manifest.contains("cpu: 100m"));
        assert!(manifest.contains("memory: 64Mi"));
    }
}
