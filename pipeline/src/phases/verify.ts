import { runQuery } from "../query.js";
import type { PhaseResult, PipelineConfig } from "../types.js";
import { existsMilestone, read, readMilestone } from "../workspace.js";

export async function verify(config: PipelineConfig, milestoneIndex: number): Promise<PhaseResult> {
	const milestones = await read(config.workDir, "milestones.json");
	const parsed = JSON.parse(milestones);
	const milestone = parsed.milestones[milestoneIndex];
	const milestoneDir = `${config.workDir}/milestone-${milestoneIndex}`;

	let validationMd = "";
	if (await existsMilestone(config.workDir, milestoneIndex, "validation.md")) {
		validationMd = await readMilestone(config.workDir, milestoneIndex, "validation.md");
	}

	let stepValidations = "";
	if (await existsMilestone(config.workDir, milestoneIndex, "steps.json")) {
		const stepsTxt = await readMilestone(config.workDir, milestoneIndex, "steps.json");
		const steps = JSON.parse(stepsTxt);
		stepValidations = steps
			.map(
				(s: { id: number; name: string; validation: string }) =>
					`- **Step ${s.id} (${s.name}):** \`${s.validation}\``,
			)
			.join("\n");
	}

	const prompt = `You are a verification agent for milestone ${milestone.version} — "${milestone.name}" of the devrig project.

Run ALL verification checks and produce two output files:
1. ${milestoneDir}/verification-results.md — Detailed output from every check
2. ${milestoneDir}/verification-status.json — Structured pass/fail status

## Project Root
${config.repoRoot}

All commands must be run from this directory.

## Standard Checks (run these for every milestone)

Run each of these commands and record the exit code and output:

1. \`cargo fmt --check\` — Code formatting
2. \`cargo clippy -- -D warnings\` — Linting
3. \`cargo build\` — Compilation
4. \`cargo test\` — Unit tests

${stepValidations ? `## Per-Step Validation Commands\n\nRun each of these and verify they pass:\n${stepValidations}` : ""}

## Milestone-Specific Checks
${validationMd || "No additional validation criteria specified."}

## Verification Procedure

For each check:
1. Run the command
2. Record whether it passed (exit code 0) or failed
3. Capture relevant output (especially error messages)

## Output Format

### verification-results.md
\`\`\`markdown
# Verification Results — ${milestone.version}

## cargo fmt --check
**Status:** PASSED/FAILED
\`\`\`
<output here>
\`\`\`

## cargo clippy
**Status:** PASSED/FAILED
\`\`\`
<output here>
\`\`\`

(... for each check)

## Summary
- Total checks: N
- Passed: N
- Failed: N
\`\`\`

### verification-status.json
Write this file with the Write tool:
\`\`\`json
{
  "milestone": "${milestone.version}",
  "passed": true/false,
  "checks": [
    { "name": "cargo fmt", "passed": true/false, "output": "..." },
    { "name": "cargo clippy", "passed": true/false, "output": "..." },
    { "name": "cargo build", "passed": true/false, "output": "..." },
    { "name": "cargo test", "passed": true/false, "output": "..." }
  ],
  "failures": ["list of failed check names"]
}
\`\`\`

IMPORTANT: The "passed" top-level field should be true ONLY if ALL checks passed. If ANY check failed, set it to false and list the failures.

Run every check. Do not skip any. Do not assume results — actually run the commands.`;

	return runQuery({
		prompt,
		config,
		phase: `verify-${milestoneIndex}`,
		tools: ["Read", "Write", "Bash", "Glob", "Grep"],
	});
}
