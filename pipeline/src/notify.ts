import { execFile } from "node:child_process";

/** Send a push notification via ntfy. Non-fatal on failure. */
export async function notify(message: string): Promise<void> {
  await new Promise<void>((resolve) => {
    execFile(`${process.env.HOME}/.local/bin/ntfy`, [message], () => resolve());
  });
}
