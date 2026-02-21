/** CLI configuration parsed from command-line arguments. */
export interface PipelineConfig {
	prdPath: string;
	model: string;
	maxRetries: number;
	startMilestone?: number;
	workDir: string;
	repoRoot: string;
}

/** Result from a single Agent SDK query. */
export interface PhaseResult {
	sessionId: string;
	cost: number;
	duration: number;
	turns: number;
	output: string;
}

/** Top-level parsed PRD structure. */
export interface ParsedPRD {
	project: {
		name: string;
		language: string;
	};
	milestones: Milestone[];
}

/** A single milestone extracted from the PRD. */
export interface Milestone {
	id: number;
	version: string;
	name: string;
	features: string[];
	tests: string[];
	docs: string[];
	dependencies: number[];
}

/** A single implementation step within a milestone. */
export interface PlanStep {
	id: number;
	name: string;
	description: string;
	files: string[];
	validation: string;
	depends_on: number[];
}

/** Result of a single verification check. */
export interface VerificationCheck {
	name: string;
	passed: boolean;
	output: string;
}

/** Aggregate verification status for a milestone. */
export interface VerificationStatus {
	milestone: string;
	passed: boolean;
	checks: VerificationCheck[];
	failures: string[];
}

/** State of a single milestone in the pipeline. */
export interface MilestoneState {
	id: number;
	version: string;
	status: "pending" | "in_progress" | "completed" | "failed";
	attempts: number;
	cost: number;
}

/** Persistent pipeline state across runs. */
export interface PipelineState {
	started_at: string;
	current_milestone: number;
	milestones: MilestoneState[];
	total_cost: number;
}

/** Phase function signature — each phase takes config + milestone index. */
export type MilestonePhaseFunction = (config: PipelineConfig, milestoneIndex: number) => Promise<PhaseResult>;

/** Phase function for the parse phase (no milestone index). */
export type ParsePhaseFunction = (config: PipelineConfig) => Promise<PhaseResult>;
