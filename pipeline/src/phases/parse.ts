import { runQuery } from "../query.js";
import type { PhaseResult, PipelineConfig } from "../types.js";

export async function parse(config: PipelineConfig): Promise<PhaseResult> {
	const prdContent = await Bun.file(config.prdPath).text();

	const prompt = `You are parsing a Product Requirements Document (PRD) into structured milestones for an automated build pipeline.

Read the PRD content below, then produce a JSON file at: ${config.workDir}/milestones.json

The JSON must follow this exact schema:
{
  "project": {
    "name": "devrig",
    "language": "rust"
  },
  "milestones": [
    {
      "id": 0,
      "version": "v0.1",
      "name": "Short descriptive name",
      "features": ["feature 1", "feature 2", ...],
      "tests": ["test category 1: description", ...],
      "docs": ["doc file or section", ...],
      "dependencies": []
    },
    {
      "id": 1,
      "version": "v0.2",
      "name": "...",
      "features": [...],
      "tests": [...],
      "docs": [...],
      "dependencies": [0]
    }
  ]
}

Rules:
- Extract every milestone from the PRD's "Milestones" section
- Each milestone's features should include ALL bullet points from the PRD
- Each milestone's tests should include ALL test requirements mentioned
- Each milestone's docs should include ALL documentation requirements mentioned
- Dependencies should reference earlier milestone IDs (e.g., v0.2 depends on v0.1)
- Preserve the version numbering from the PRD exactly
- Be exhaustive — do not omit any feature, test, or doc requirement

Write the file using the Write tool to: ${config.workDir}/milestones.json

After writing, verify the file is valid JSON by reading it back.

<prd>
${prdContent}
</prd>`;

	return runQuery({
		prompt,
		config,
		phase: "parse",
		tools: ["Read", "Write", "Bash"],
		cwd: config.workDir,
	});
}
