import { runQuery } from "../query.js";
import type { PhaseResult, PipelineConfig } from "../types.js";
import { existsMilestone, read, readMilestone } from "../workspace.js";

export async function architect(config: PipelineConfig, milestoneIndex: number): Promise<PhaseResult> {
	const milestones = await read(config.workDir, "milestones.json");
	const parsed = JSON.parse(milestones);
	const milestone = parsed.milestones[milestoneIndex];
	const researchMd = await readMilestone(config.workDir, milestoneIndex, "research.md");
	const prdContent = await Bun.file(config.prdPath).text();
	const milestoneDir = `${config.workDir}/milestone-${milestoneIndex}`;

	// Gather context from prior milestones
	const priorContext: string[] = [];
	for (let i = 0; i < milestoneIndex; i++) {
		if (await existsMilestone(config.workDir, i, "plan.md")) {
			const plan = await readMilestone(config.workDir, i, "plan.md");
			priorContext.push(`## Milestone ${i} (${parsed.milestones[i].version}) Plan\n\n${plan}`);
		}
		if (await existsMilestone(config.workDir, i, "report.md")) {
			const report = await readMilestone(config.workDir, i, "report.md");
			priorContext.push(`## Milestone ${i} (${parsed.milestones[i].version}) Report\n\n${report}`);
		}
	}

	const prompt = `You are an architect designing the implementation plan for milestone ${milestone.version} — "${milestone.name}" of the devrig project.

You must produce THREE files:
1. ${milestoneDir}/plan.md — Detailed implementation plan in markdown
2. ${milestoneDir}/steps.json — Structured step array for the execution phase
3. ${milestoneDir}/validation.md — Validation criteria for the verification phase

## Milestone Details

Version: ${milestone.version}
Name: ${milestone.name}
Features:
${milestone.features.map((f: string) => `- ${f}`).join("\n")}

Tests required:
${milestone.tests.map((t: string) => `- ${t}`).join("\n")}

Docs required:
${milestone.docs.map((d: string) => `- ${d}`).join("\n")}

## Research Findings

${researchMd}

## Full PRD

<prd>
${prdContent}
</prd>

${priorContext.length > 0 ? `## Prior Milestone Context\n\n${priorContext.join("\n\n---\n\n")}` : ""}

## Current Codebase

The project root is at: ${config.repoRoot}
Explore the current state thoroughly before designing the plan.

## Output Requirements

### plan.md
A detailed implementation plan covering:
- Architecture overview for this milestone
- File structure (what files to create/modify)
- Implementation order and rationale
- Key design decisions
- Integration points with existing code
- Testing strategy
- Documentation plan

### steps.json
An ordered array of implementation steps:
[
  {
    "id": 1,
    "name": "Short step name",
    "description": "Detailed description of what to implement",
    "files": ["path/to/file1.rs", "path/to/file2.rs"],
    "validation": "cargo build && cargo test -- step_name",
    "depends_on": []
  },
  {
    "id": 2,
    "name": "...",
    "description": "...",
    "files": ["..."],
    "validation": "cargo test",
    "depends_on": [1]
  }
]

Rules for steps:
- Each step should be independently verifiable
- Steps should be ordered by dependency
- Include file paths relative to the repo root (${config.repoRoot})
- Validation commands must be runnable from the repo root
- Keep steps focused — one logical unit of work each
- Include steps for tests and documentation, not just implementation
- Aim for 5-20 steps depending on milestone complexity

### validation.md
Validation criteria covering:
- Standard checks (cargo fmt, clippy, build, test)
- Milestone-specific integration tests
- Documentation completeness checks
- Any custom validation (e.g., agent-browser for dashboard)

Write all three files using the Write tool.`;

	return runQuery({
		prompt,
		config,
		phase: `architect-${milestoneIndex}`,
		tools: ["Read", "Write", "Glob", "Grep", "Bash"],
	});
}
