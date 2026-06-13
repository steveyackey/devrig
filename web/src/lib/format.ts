export function formatDuration(ms: number): string {
  if (ms < 1) return "<1ms";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60000).toFixed(1)}m`;
}

export function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString();
  } catch {
    return iso;
  }
}

export function formatTimeMs(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString(undefined, { hour12: false, fractionalSecondDigits: 3 });
  } catch {
    return iso;
  }
}

export function truncateId(id: string, length = 16): string {
  if (id.length <= length) return id;
  return id.slice(0, 8) + "…" + id.slice(-4);
}

/** Format a large number with K/M suffixes. */
export function formatValue(value: number): string {
  if (Math.abs(value) >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (Math.abs(value) >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  if (Number.isInteger(value)) return value.toLocaleString();
  return value.toFixed(2);
}

export function severityVariant(sev: string): string {
  switch (sev) {
    case "Error":
    case "Fatal":
      return "error";
    case "Warn":
      return "warning";
    case "Info":
      return "info";
    default:
      return "default";
  }
}

export function severityColor(sev: string): string {
  switch (sev) {
    case "Error":
    case "Fatal":
      return "text-error";
    case "Warn":
      return "text-warning";
    case "Info":
      return "text-info";
    default:
      return "text-text-muted";
  }
}
