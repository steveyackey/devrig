import { runQuery } from "../query.js";
import type { PhaseResult, PipelineConfig } from "../types.js";
import { exists, existsMilestone, read, readMilestone } from "../workspace.js";

export async function report(config: PipelineConfig, milestoneIndex: number): Promise<PhaseResult> {
	const milestones = await read(config.workDir, "milestones.json");
	const parsed = JSON.parse(milestones);
	const milestone = parsed.milestones[milestoneIndex];
	const milestoneDir = `${config.workDir}/milestone-${milestoneIndex}`;

	// Collect all milestone artifacts
	const artifacts: string[] = [];

	for (const file of ["research.md", "plan.md", "steps.json", "execution-results.md", "verification-results.md"]) {
		if (await existsMilestone(config.workDir, milestoneIndex, file)) {
			const content = await readMilestone(config.workDir, milestoneIndex, file);
			artifacts.push(`## ${file}\n\n${content}`);
		}
	}

	let verificationStatus = "";
	if (await existsMilestone(config.workDir, milestoneIndex, "verification-status.json")) {
		verificationStatus = await readMilestone(config.workDir, milestoneIndex, "verification-status.json");
	}

	const prompt = `You are generating a milestone report for ${milestone.version} — "${milestone.name}" of the devrig project.

Synthesize all artifacts from this milestone into a clear, comprehensive report.

## Milestone Details
Version: ${milestone.version}
Name: ${milestone.name}
Features: ${milestone.features.join(", ")}

## Artifacts

${artifacts.join("\n\n---\n\n")}

${verificationStatus ? `## Verification Status\n\n\`\`\`json\n${verificationStatus}\n\`\`\`` : ""}

## Report Instructions

Write a comprehensive milestone report to: ${milestoneDir}/report.md

Structure:
1. **Summary** — What was built in 2-3 sentences
2. **Features Implemented** — List each feature with brief description and status
3. **Architecture** — Key structural decisions made during implementation
4. **Tests** — What tests were written and their status
5. **Documentation** — What docs were created/updated
6. **Verification Status** — Pass/fail summary
7. **Known Issues** — Any remaining problems or incomplete items
8. **Next Milestone Context** — What the next milestone should know about this one's implementation

Be factual — reference actual files, test names, and error messages. Don't invent or assume results.`;

	return runQuery({
		prompt,
		config,
		phase: `report-${milestoneIndex}`,
		tools: ["Read", "Write", "Glob", "Grep"],
	});
}

export async function finalReport(config: PipelineConfig): Promise<PhaseResult> {
	const milestones = await read(config.workDir, "milestones.json");
	const parsed = JSON.parse(milestones);

	// Collect all milestone reports
	const milestoneReports: string[] = [];
	for (let i = 0; i < parsed.milestones.length; i++) {
		if (await existsMilestone(config.workDir, i, "report.md")) {
			const report = await readMilestone(config.workDir, i, "report.md");
			milestoneReports.push(
				`## Milestone ${i} — ${parsed.milestones[i].version}: ${parsed.milestones[i].name}\n\n${report}`,
			);
		}
	}

	// Read pipeline state
	let pipelineState = "";
	if (await exists(config.workDir, "pipeline-state.json")) {
		pipelineState = await read(config.workDir, "pipeline-state.json");
	}

	const prompt = `You are generating the final comprehensive report for the devrig project pipeline.

All milestones have been processed. Synthesize everything into a final report.

## Project
Name: ${parsed.project.name}
Language: ${parsed.project.language}
Total milestones: ${parsed.milestones.length}

## Pipeline State
${pipelineState ? `\`\`\`json\n${pipelineState}\n\`\`\`` : "Not available"}

## Milestone Reports

${milestoneReports.join("\n\n---\n\n")}

## Report Instructions

Write the final report to: ${config.workDir}/report.md

Structure:
1. **Executive Summary** — Project overview and overall status
2. **Milestones Completed** — Brief summary of each milestone with pass/fail
3. **Architecture Overview** — How the final system is structured
4. **Test Coverage** — Summary of all tests across milestones
5. **Documentation** — What docs were created
6. **Total Cost** — Pipeline execution costs (from pipeline state)
7. **Known Issues** — Any remaining problems across all milestones
8. **Recommendations** — Next steps, improvements, areas for future work

Also explore the current codebase to verify the final state matches expectations.`;

	return runQuery({
		prompt,
		config,
		phase: "final-report",
		tools: ["Read", "Write", "Glob", "Grep", "Bash"],
	});
}
