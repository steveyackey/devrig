import { log } from "./log.js";
import { notify } from "./notify.js";
import { architect } from "./phases/architect.js";
import { execute } from "./phases/execute.js";
import { fix } from "./phases/fix.js";
import { parse } from "./phases/parse.js";
import { finalReport, report } from "./phases/report.js";
import { research } from "./phases/research.js";
import { verify } from "./phases/verify.js";
import type { MilestonePhaseFunction, ParsedPRD, PipelineConfig, PipelineState, VerificationStatus } from "./types.js";
import { exists, existsMilestone, init, initMilestone, read, readMilestone, write } from "./workspace.js";

async function loadOrCreateState(config: PipelineConfig): Promise<PipelineState> {
	if (await exists(config.workDir, "pipeline-state.json")) {
		return JSON.parse(await read(config.workDir, "pipeline-state.json"));
	}
	return {
		started_at: new Date().toISOString(),
		current_milestone: config.startMilestone ?? 0,
		milestones: [],
		total_cost: 0,
	};
}

async function saveState(config: PipelineConfig, state: PipelineState): Promise<void> {
	await write(config.workDir, "pipeline-state.json", JSON.stringify(state, null, 2));
}

async function runMilestonePhase(
	name: string,
	fn: MilestonePhaseFunction,
	config: PipelineConfig,
	milestoneIndex: number,
): Promise<number> {
	log("phase_start", { name, milestone: milestoneIndex });
	const result = await fn(config, milestoneIndex);
	log("phase_complete", {
		name,
		milestone: milestoneIndex,
		cost: result.cost,
		duration: result.duration,
		turns: result.turns,
	});
	return result.cost;
}

export async function runPipeline(config: PipelineConfig): Promise<void> {
	await init(config.workDir);

	const state = await loadOrCreateState(config);

	// Phase 1: Parse PRD into milestones (one-time)
	if (!(await exists(config.workDir, "milestones.json"))) {
		log("phase_start", { name: "parse" });
		const parseResult = await parse(config);
		log("phase_complete", {
			name: "parse",
			cost: parseResult.cost,
			duration: parseResult.duration,
			turns: parseResult.turns,
		});
		state.total_cost += parseResult.cost;
	}

	// Load parsed milestones
	const prd: ParsedPRD = JSON.parse(await read(config.workDir, "milestones.json"));
	const milestoneCount = prd.milestones.length;

	// Initialize milestone states if needed
	if (state.milestones.length === 0) {
		state.milestones = prd.milestones.map((m) => ({
			id: m.id,
			version: m.version,
			status: "pending",
			attempts: 0,
			cost: 0,
		}));
	}

	await notify(`devrig-pipeline: Starting — ${milestoneCount} milestones to build`);
	await saveState(config, state);

	log("pipeline_start", {
		project: prd.project.name,
		milestones: milestoneCount,
		startMilestone: state.current_milestone,
	});

	// Process each milestone
	const startFrom = config.startMilestone ?? state.current_milestone;
	for (let i = startFrom; i < milestoneCount; i++) {
		const milestone = prd.milestones[i];
		if (!milestone) continue;

		const msState = state.milestones[i];
		if (!msState) continue;

		// Skip already completed milestones
		if (msState.status === "completed") {
			log("milestone_skip", { id: i, version: milestone.version, reason: "already completed" });
			continue;
		}

		await initMilestone(config.workDir, i);
		msState.status = "in_progress";
		state.current_milestone = i;
		await saveState(config, state);

		await notify(`devrig-pipeline: Starting ${milestone.version} — ${milestone.name}`);
		log("milestone_start", { id: i, version: milestone.version, name: milestone.name });

		let milestoneCost = 0;

		// Research phase
		if (!(await existsMilestone(config.workDir, i, "research.md"))) {
			milestoneCost += await runMilestonePhase("research", research, config, i);
			await notify(`devrig-pipeline: ${milestone.version} — research complete`);
		}

		// Architect phase
		if (!(await existsMilestone(config.workDir, i, "plan.md"))) {
			milestoneCost += await runMilestonePhase("architect", architect, config, i);
			await notify(`devrig-pipeline: ${milestone.version} — architecture complete`);
		}

		// Execute → Verify retry loop
		let passed = false;
		for (let attempt = 1; attempt <= config.maxRetries; attempt++) {
			msState.attempts = attempt;
			await saveState(config, state);

			log("execute_verify_attempt", { milestone: i, attempt, maxRetries: config.maxRetries });

			// Execute (attempt 1) or Fix (attempt 2+)
			if (attempt === 1) {
				const executeCost = await runMilestonePhase("execute", execute, config, i);
				milestoneCost += executeCost;
			} else {
				await notify(`devrig-pipeline: ${milestone.version} — running targeted fix (attempt ${attempt})`);
				const fixCost = await runMilestonePhase("fix", fix, config, i);
				milestoneCost += fixCost;
			}

			// Verify
			const verifyCost = await runMilestonePhase("verify", verify, config, i);
			milestoneCost += verifyCost;

			// Check verification status
			if (await existsMilestone(config.workDir, i, "verification-status.json")) {
				const statusJson = await readMilestone(config.workDir, i, "verification-status.json");
				const status: VerificationStatus = JSON.parse(statusJson);

				if (status.passed) {
					passed = true;
					await notify(`devrig-pipeline: ✓ ${milestone.version} verified`);
					log("execute_verify_passed", { milestone: i, attempt });
					break;
				}

				if (attempt < config.maxRetries) {
					await notify(
						`devrig-pipeline: ✗ ${milestone.version} failed (attempt ${attempt}/${config.maxRetries}), retrying`,
					);
					log("execute_verify_retry", {
						milestone: i,
						attempt,
						failures: status.failures,
					});
				} else {
					await notify(`devrig-pipeline: ✗ ${milestone.version} FAILED after ${attempt} attempts`);
					log("execute_verify_exhausted", {
						milestone: i,
						attempt,
						failures: status.failures,
					});
				}
			} else {
				log("execute_verify_no_status", { milestone: i, attempt });
				break;
			}
		}

		// Report phase (always runs)
		milestoneCost += await runMilestonePhase("report", report, config, i);

		// Update state
		msState.cost = milestoneCost;
		msState.status = passed ? "completed" : "failed";
		state.total_cost += milestoneCost;
		await saveState(config, state);

		await notify(
			`devrig-pipeline: ${passed ? "✓" : "✗"} ${milestone.version} ${passed ? "complete" : "failed"} (cost: $${milestoneCost.toFixed(2)})`,
		);

		log("milestone_complete", {
			id: i,
			version: milestone.version,
			passed,
			attempts: msState.attempts,
			cost: milestoneCost,
		});

		// Commit and push on success
		if (passed) {
			log("git_commit_start", { milestone: i, version: milestone.version });
			const commitMsg = `feat(${milestone.version}): ${milestone.name}`;
			const proc = Bun.spawn(["bash", "-c", `cd ${config.repoRoot} && git add -A && git commit -m "${commitMsg}" && git push`], {
				stdout: "pipe",
				stderr: "pipe",
			});
			const exitCode = await proc.exited;
			if (exitCode === 0) {
				log("git_commit_complete", { milestone: i, version: milestone.version });
				await notify(`devrig-pipeline: ${milestone.version} committed and pushed`);
			} else {
				const stderr = await new Response(proc.stderr).text();
				log("git_commit_failed", { milestone: i, version: milestone.version, exitCode, stderr });
				await notify(`devrig-pipeline: ⚠ ${milestone.version} git commit/push failed (exit ${exitCode})`);
			}
		}

		// Stop pipeline on failure
		if (!passed) {
			await notify(`devrig-pipeline: BLOCKED — ${milestone.version} failed after all retries`);
			log("pipeline_blocked", { milestone: i, version: milestone.version });
			break;
		}
	}

	// Final report (always runs)
	log("phase_start", { name: "final-report" });
	const finalReportResult = await finalReport(config);
	state.total_cost += finalReportResult.cost;
	await saveState(config, state);
	log("phase_complete", {
		name: "final-report",
		cost: finalReportResult.cost,
		duration: finalReportResult.duration,
		turns: finalReportResult.turns,
	});

	await notify(`devrig-pipeline: COMPLETE — all milestones done ($${state.total_cost.toFixed(2)} total)`);
	log("pipeline_complete", { totalCost: state.total_cost });
}
