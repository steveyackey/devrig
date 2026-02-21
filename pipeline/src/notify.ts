import { $ } from "bun";

/** Send a push notification via ntfy. Non-fatal on failure. */
export async function notify(message: string): Promise<void> {
	try {
		await $`${process.env.HOME}/.local/bin/ntfy ${message}`.quiet();
	} catch {
		// Non-fatal — don't break pipeline if ntfy fails
	}
}
