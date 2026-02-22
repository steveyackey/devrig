import { generateTraces } from "./generators/traces";
import { generateLogs } from "./generators/logs";
import { generateMetrics } from "./generators/metrics";
import { sendOtlp } from "./otlp";

function parseArgs() {
  const args = process.argv.slice(2);
  let otlpUrl = "http://localhost:4318";
  let duration = 30;
  let continuous = false;
  let traceCount = 15;
  let logCount = 50;

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--otlp-url":
        otlpUrl = args[++i];
        break;
      case "--duration":
        duration = parseInt(args[++i], 10);
        break;
      case "--continuous":
        continuous = true;
        break;
      case "--traces":
        traceCount = parseInt(args[++i], 10);
        break;
      case "--logs":
        logCount = parseInt(args[++i], 10);
        break;
    }
  }

  return { otlpUrl, duration, continuous, traceCount, logCount };
}

async function generateBurst(opts: ReturnType<typeof parseArgs>) {
  const seed = Date.now();
  console.log(`Generating telemetry (seed=${seed})...`);

  const traces = generateTraces(opts.traceCount, seed);
  const logs = generateLogs(opts.logCount, seed + 1);
  const metrics = generateMetrics(opts.duration, 5, seed + 2);

  const sendOpts = { url: opts.otlpUrl };

  await Promise.all([
    sendOtlp("/v1/traces", traces, sendOpts),
    sendOtlp("/v1/logs", logs, sendOpts),
    sendOtlp("/v1/metrics", metrics, sendOpts),
  ]);

  console.log(
    `Sent: ${opts.traceCount} traces, ${opts.logCount} logs, ${metrics.resourceMetrics.length} service metric sets`,
  );
}

async function main() {
  const opts = parseArgs();
  console.log(`DevRig Telemetry Generator`);
  console.log(`  OTLP endpoint: ${opts.otlpUrl}`);
  console.log(`  Duration: ${opts.duration}s`);
  console.log(`  Mode: ${opts.continuous ? "continuous" : "single burst"}`);

  await generateBurst(opts);

  if (opts.continuous) {
    console.log(`Continuous mode: generating every ${opts.duration}s...`);
    setInterval(() => generateBurst(opts), opts.duration * 1000);
  }
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
