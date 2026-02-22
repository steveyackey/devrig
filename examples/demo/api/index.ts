import { SQL } from "bun";
import {
  tracer,
  extractContext,
  withSpan,
  SpanKind,
  SpanStatusCode,
} from "../shared/tracing";

const pgUrl = `${process.env.DEVRIG_POSTGRES_URL}/demo`;
const sql = new SQL(pgUrl);
const port = parseInt(process.env.PORT || "3001");

function dbSpan<T>(name: string, statement: string, fn: () => Promise<T>): Promise<T> {
  return tracer.startActiveSpan(
    name,
    {
      kind: SpanKind.CLIENT,
      attributes: {
        "db.system": "postgresql",
        "db.statement": statement,
      },
    },
    async (span) => {
      try {
        const result = await fn();
        span.setStatus({ code: SpanStatusCode.OK });
        return result;
      } catch (err) {
        span.setStatus({
          code: SpanStatusCode.ERROR,
          message: err instanceof Error ? err.message : String(err),
        });
        throw err;
      } finally {
        span.end();
      }
    },
  );
}

async function handleRequest(req: Request): Promise<Response> {
  const url = new URL(req.url);
  const method = req.method;
  const path = url.pathname;
  const parentCtx = extractContext(req.headers);

  return withSpan(
    `${method} ${path}`,
    SpanKind.SERVER,
    async (span) => {
      span.setAttribute("http.method", method);
      span.setAttribute("http.url", path);

      // GET /api/health
      if (path === "/api/health" && method === "GET") {
        span.setAttribute("http.status_code", 200);
        return Response.json({ status: "ok" });
      }

      // GET /api/todos
      if (path === "/api/todos" && method === "GET") {
        const rows = await dbSpan(
          "SELECT todos",
          "SELECT * FROM todos ORDER BY created_at DESC",
          () => sql`SELECT * FROM todos ORDER BY created_at DESC`,
        );
        span.setAttribute("http.status_code", 200);
        return Response.json(rows);
      }

      // POST /api/todos
      if (path === "/api/todos" && method === "POST") {
        const body = await req.json();
        const title = body?.title;
        if (!title || typeof title !== "string") {
          span.setAttribute("http.status_code", 400);
          return Response.json({ error: "title is required" }, { status: 400 });
        }
        const rows = await dbSpan(
          "INSERT todo",
          "INSERT INTO todos (title) VALUES ($1) RETURNING *",
          () => sql`INSERT INTO todos (title) VALUES (${title}) RETURNING *`,
        );
        span.setAttribute("http.status_code", 201);
        return Response.json(rows[0], { status: 201 });
      }

      // PATCH /api/todos/:id
      const patchMatch = path.match(/^\/api\/todos\/(\d+)$/);
      if (patchMatch && method === "PATCH") {
        const id = parseInt(patchMatch[1]);
        const rows = await dbSpan(
          "UPDATE todo",
          "UPDATE todos SET completed = NOT completed WHERE id = $1 RETURNING *",
          () =>
            sql`UPDATE todos SET completed = NOT completed WHERE id = ${id} RETURNING *`,
        );
        if (rows.length === 0) {
          span.setAttribute("http.status_code", 404);
          return Response.json({ error: "not found" }, { status: 404 });
        }
        span.setAttribute("http.status_code", 200);
        return Response.json(rows[0]);
      }

      // DELETE /api/todos/:id
      const deleteMatch = path.match(/^\/api\/todos\/(\d+)$/);
      if (deleteMatch && method === "DELETE") {
        const id = parseInt(deleteMatch[1]);
        const rows = await dbSpan(
          "DELETE todo",
          "DELETE FROM todos WHERE id = $1 RETURNING id",
          () => sql`DELETE FROM todos WHERE id = ${id} RETURNING id`,
        );
        if (rows.length === 0) {
          span.setAttribute("http.status_code", 404);
          return Response.json({ error: "not found" }, { status: 404 });
        }
        span.setAttribute("http.status_code", 204);
        return new Response(null, { status: 204 });
      }

      span.setAttribute("http.status_code", 404);
      return Response.json({ error: "not found" }, { status: 404 });
    },
    parentCtx,
  );
}

Bun.serve({
  port,
  fetch: handleRequest,
});

console.log(`api listening on :${port}`);
