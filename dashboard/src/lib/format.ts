/** Format a duration in milliseconds to a human-readable string. */
export const formatDuration = (ms: number): string => {
	if (ms < 1) return "<1ms";
	if (ms < 1000) return `${ms.toFixed(1)}ms`;
	return `${(ms / 1000).toFixed(2)}s`;
};

/** Format an ISO timestamp to a locale time string (HH:MM:SS). */
export const formatTime = (iso: string): string => {
	try {
		const d = new Date(iso);
		return d.toLocaleTimeString(undefined, {
			hour: "2-digit",
			minute: "2-digit",
			second: "2-digit",
		});
	} catch {
		return iso;
	}
};

/** Format an ISO timestamp with millisecond precision. */
export const formatTimeMs = (iso: string): string => {
	try {
		const d = new Date(iso);
		return d.toLocaleTimeString(undefined, {
			hour: "2-digit",
			minute: "2-digit",
			second: "2-digit",
			fractionalSecondDigits: 3,
		} as Intl.DateTimeFormatOptions);
	} catch {
		return iso;
	}
};

/** Map a log severity string to a Badge variant. */
export const severityVariant = (severity: string) => {
	switch (severity) {
		case "Fatal":
			return "fatal" as const;
		case "Error":
			return "error" as const;
		case "Warn":
			return "warning" as const;
		case "Info":
			return "info" as const;
		case "Debug":
			return "debug" as const;
		case "Trace":
			return "trace" as const;
		default:
			return "default" as const;
	}
};

/** Format a large number with K/M suffixes. */
export const formatValue = (value: number): string => {
	if (Math.abs(value) >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
	if (Math.abs(value) >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
	if (Number.isInteger(value)) return value.toLocaleString();
	return value.toFixed(2);
};
