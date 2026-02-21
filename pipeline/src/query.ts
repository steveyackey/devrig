import { type AgentDefinition, query, type SDKMessage } from "@anthropic-ai/claude-agent-sdk";
import { log } from "./log.js";
import type { PhaseResult, PipelineConfig } from "./types.js";

export interface QueryOptions {
	prompt: string;
	config: PipelineConfig;
	phase: string;
	sessionId?: string;
	tools: string[];
	cwd?: string;
	systemPrompt?: string;
	agents?: Record<string, AgentDefinition>;
}

export async function runQuery(opts: QueryOptions): Promise<PhaseResult> {
	const messages: SDKMessage[] = [];
	let sid: string | undefined = opts.sessionId;

	// Strip CLAUDECODE env var to allow running from within a Claude Code session
	const env = { ...process.env };
	delete env.CLAUDECODE;

	for await (const msg of query({
		prompt: opts.prompt,
		options: {
			resume: opts.sessionId,
			model: opts.config.model,
			cwd: opts.cwd ?? opts.config.repoRoot,
			env,
			allowedTools: opts.tools,
			agents: opts.agents,
			permissionMode: "bypassPermissions",
			allowDangerouslySkipPermissions: true,
			settingSources: [],
			systemPrompt: opts.systemPrompt
				? { type: "preset", preset: "claude_code", append: opts.systemPrompt }
				: { type: "preset", preset: "claude_code" },
			additionalDirectories: [opts.config.workDir, opts.config.repoRoot],
		},
	})) {
		messages.push(msg);

		if (msg.type === "system" && msg.subtype === "init") {
			sid = msg.session_id;
		}

		log("sdk_message", {
			phase: opts.phase,
			type: msg.type,
			...(msg.type === "result" ? { subtype: msg.subtype } : {}),
		});

		process.stdout.write(`${JSON.stringify(msg)}\n`);
	}

	const result = messages.find((m): m is Extract<SDKMessage, { type: "result" }> => m.type === "result");

	return {
		sessionId: sid ?? "",
		cost: result?.subtype === "success" ? result.total_cost_usd : 0,
		duration: result?.subtype === "success" ? result.duration_ms : 0,
		turns: result?.subtype === "success" ? result.num_turns : 0,
		output: result?.subtype === "success" ? result.result : "",
	};
}
