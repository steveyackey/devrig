import { mkdir } from "node:fs/promises";
import { join } from "node:path";

/** Initialize the agent-data directory and milestone subdirectories. */
export async function init(workDir: string): Promise<void> {
	await mkdir(workDir, { recursive: true });
}

/** Ensure a milestone's working directory exists. */
export async function initMilestone(workDir: string, milestoneIndex: number): Promise<void> {
	await mkdir(milestoneDir(workDir, milestoneIndex), { recursive: true });
}

/** Get the path to a milestone's working directory. */
export function milestoneDir(workDir: string, milestoneIndex: number): string {
	return join(workDir, `milestone-${milestoneIndex}`);
}

/** Write a file into the agent-data root. */
export async function write(workDir: string, filename: string, content: string): Promise<void> {
	await Bun.write(join(workDir, filename), content);
}

/** Write a file into a milestone's directory. */
export async function writeMilestone(
	workDir: string,
	milestoneIndex: number,
	filename: string,
	content: string,
): Promise<void> {
	const dir = milestoneDir(workDir, milestoneIndex);
	await mkdir(dir, { recursive: true });
	await Bun.write(join(dir, filename), content);
}

/** Read a file from the agent-data root. */
export async function read(workDir: string, filename: string): Promise<string> {
	return Bun.file(join(workDir, filename)).text();
}

/** Read a file from a milestone's directory. */
export async function readMilestone(workDir: string, milestoneIndex: number, filename: string): Promise<string> {
	return Bun.file(join(milestoneDir(workDir, milestoneIndex), filename)).text();
}

/** Check if a file exists in the agent-data root. */
export async function exists(workDir: string, filename: string): Promise<boolean> {
	return Bun.file(join(workDir, filename)).exists();
}

/** Check if a file exists in a milestone's directory. */
export async function existsMilestone(workDir: string, milestoneIndex: number, filename: string): Promise<boolean> {
	return Bun.file(join(milestoneDir(workDir, milestoneIndex), filename)).exists();
}
