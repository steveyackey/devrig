import { resolve } from "node:path";
import { runPipeline } from "./pipeline.js";
import type { PipelineConfig } from "./types.js";

function parseArgs(argv: string[]): PipelineConfig {
	let prdPath: string | undefined;
	let model = "claude-sonnet-4-6";
	let maxRetries = 3;
	let startMilestone: number | undefined;

	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		const next = argv[i + 1];

		switch (arg) {
			case "--prd":
				if (next) {
					prdPath = next;
					i++;
				}
				break;
			case "--model":
			case "-m":
				if (next) {
					model = next;
					i++;
				}
				break;
			case "--max-retries":
				if (next) {
					maxRetries = Number.parseInt(next, 10);
					i++;
				}
				break;
			case "--milestone":
				if (next) {
					startMilestone = Number.parseInt(next, 10);
					i++;
				}
				break;
			case "--help":
			case "-h":
				printUsage();
				process.exit(0);
				break;
			default:
				if (arg?.startsWith("-")) {
					console.error(`Unknown flag: ${arg}`);
					process.exit(1);
				}
		}
	}

	if (!prdPath) {
		console.error("Error: --prd <path> is required");
		console.error("");
		printUsage();
		process.exit(1);
	}

	const repoRoot = resolve(".");

	return {
		prdPath: resolve(prdPath),
		model,
		maxRetries,
		startMilestone,
		workDir: resolve("pipeline/agent-data"),
		repoRoot,
	};
}

function printUsage(): void {
	console.error("Usage: bun pipeline/src/index.ts --prd <path> [options]");
	console.error("");
	console.error("Options:");
	console.error("  --prd <path>        Path to PRD.md file (required)");
	console.error("  --model, -m <model> Model to use (default: claude-sonnet-4-6)");
	console.error("  --max-retries <n>   Max retry attempts per milestone (default: 3)");
	console.error("  --milestone <n>     Start from a specific milestone index (0-based)");
	console.error("  --help, -h          Show this help message");
}

const config = parseArgs(process.argv.slice(2));
await runPipeline(config);
