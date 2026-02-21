import { runQuery } from "../query.js";
import type { PhaseResult, PipelineConfig } from "../types.js";
import { existsMilestone, read, readMilestone } from "../workspace.js";

export async function research(config: PipelineConfig, milestoneIndex: number): Promise<PhaseResult> {
	const milestones = await read(config.workDir, "milestones.json");
	const parsed = JSON.parse(milestones);
	const milestone = parsed.milestones[milestoneIndex];

	// Gather context from prior milestones
	const priorContext: string[] = [];
	for (let i = 0; i < milestoneIndex; i++) {
		if (await existsMilestone(config.workDir, i, "report.md")) {
			const report = await readMilestone(config.workDir, i, "report.md");
			priorContext.push(`## Milestone ${i} (${parsed.milestones[i].version}) Report\n\n${report}`);
		}
	}

	const prdContent = await Bun.file(config.prdPath).text();
	const milestoneDir = `${config.workDir}/milestone-${milestoneIndex}`;

	const prompt = `You are a research agent preparing for milestone ${milestone.version} — "${milestone.name}" of the devrig project.

Your goal is to research best practices, crate/library choices, design patterns, and implementation strategies for this milestone's features. Write your findings to: ${milestoneDir}/research.md

## Milestone Details

Version: ${milestone.version}
Name: ${milestone.name}
Features:
${milestone.features.map((f: string) => `- ${f}`).join("\n")}

Tests required:
${milestone.tests.map((t: string) => `- ${t}`).join("\n")}

Docs required:
${milestone.docs.map((d: string) => `- ${d}`).join("\n")}

## Full PRD (for context)

<prd>
${prdContent}
</prd>

${priorContext.length > 0 ? `## Prior Milestone Reports\n\n${priorContext.join("\n\n---\n\n")}` : ""}

## Current Codebase

The project root is at: ${config.repoRoot}
Explore what exists already (if anything) to understand the current state.

## Research Instructions

1. Search the web for best practices related to this milestone's features
2. Explore the current codebase to understand what already exists
3. For Rust crates, research the latest versions, API patterns, and common pitfalls
4. For each major feature, identify the recommended approach
5. Note any architectural decisions that need to be made
6. Consider how this milestone's work integrates with previous milestones

Write comprehensive research findings to: ${milestoneDir}/research.md

Structure the output as:
- ## Crate/Library Recommendations (with versions)
- ## Design Patterns
- ## Implementation Strategy (for each major feature)
- ## Risks and Considerations
- ## References`;

	return runQuery({
		prompt,
		config,
		phase: `research-${milestoneIndex}`,
		tools: ["Read", "Write", "Glob", "Grep", "Bash", "WebSearch", "WebFetch"],
	});
}
