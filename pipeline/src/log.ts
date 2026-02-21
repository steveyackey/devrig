/** Structured JSONL logging to stdout. */
export function log(event: string, data?: Record<string, unknown>): void {
	const line = JSON.stringify({
		ts: new Date().toISOString(),
		event,
		...data,
	});
	process.stdout.write(`${line}\n`);
}
