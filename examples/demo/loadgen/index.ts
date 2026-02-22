import {
  tracer,
  withSpan,
  injectHeaders,
  SpanKind,
} from "../shared/tracing";

const webUrl = process.env.DEVRIG_WEB_URL || "http://localhost:3000";

async function apiCall(method: string, path: string, body?: unknown): Promise<unknown> {
  return withSpan(`${method} ${path}`, SpanKind.CLIENT, async (span) => {
    span.setAttribute("http.method", method);
    span.setAttribute("http.url", `${webUrl}${path}`);

    const res = await fetch(`${webUrl}${path}`, {
      method,
      headers: {
        "Content-Type": "application/json",
        ...injectHeaders(),
      },
      body: body ? JSON.stringify(body) : undefined,
    });

    span.setAttribute("http.status_code", res.status);

    if (res.status === 204) return null;
    return res.json();
  });
}

function randomTitle(): string {
  const verbs = ["Build", "Fix", "Deploy", "Test", "Review", "Update", "Refactor", "Debug"];
  const nouns = ["dashboard", "API", "database", "auth", "pipeline", "config", "service", "cache"];
  const verb = verbs[Math.floor(Math.random() * verbs.length)];
  const noun = nouns[Math.floor(Math.random() * nouns.length)];
  return `${verb} the ${noun}`;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function loop() {
  console.log(`loadgen targeting ${webUrl}`);

  while (true) {
    try {
      // Create a todo
      await withSpan("create-todo", SpanKind.INTERNAL, async () => {
        await apiCall("POST", "/api/todos", { title: randomTitle() });
      });

      await sleep(500);

      // List todos
      const todos = (await withSpan("list-todos", SpanKind.INTERNAL, async () => {
        return apiCall("GET", "/api/todos");
      })) as Array<{ id: number; completed: boolean }>;

      await sleep(500);

      // Toggle a random todo
      if (todos.length > 0) {
        const target = todos[Math.floor(Math.random() * todos.length)];
        await withSpan("toggle-todo", SpanKind.INTERNAL, async () => {
          await apiCall("PATCH", `/api/todos/${target.id}`);
        });
      }

      await sleep(500);

      // Delete oldest if > 20
      if (todos.length > 20) {
        const oldest = todos[todos.length - 1];
        await withSpan("cleanup-todo", SpanKind.INTERNAL, async () => {
          await apiCall("DELETE", `/api/todos/${oldest.id}`);
        });
      }
    } catch (err) {
      console.error("loadgen error:", err instanceof Error ? err.message : err);
    }

    // Sleep 1-3s
    await sleep(1000 + Math.random() * 2000);
  }
}

loop();
