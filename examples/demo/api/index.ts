import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import postgres from "postgres";
import {
  tracer,
  extractContext,
  withSpan,
  SpanKind,
  SpanStatusCode,
} from "../shared/tracing";

const pgUrl = `${process.env.DEVRIG_POSTGRES_URL}/demo`;
const sql = postgres(pgUrl);
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

function sendJson(res: ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  res.writeHead(status, { "content-type": "application/json;charset=utf-8" });
  res.end(payload);
}

function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk: Buffer) => chunks.push(chunk));
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

async function handleRequest(req: IncomingMessage, res: ServerResponse): Promise<void> {
  const url = new URL(req.url ?? "/", "http://localhost");
  const method = req.method ?? "GET";
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
        return sendJson(res, 200, { status: "ok" });
      }

      // GET /api/todos
      if (path === "/api/todos" && method === "GET") {
        const rows = await dbSpan(
          "SELECT todos",
          "SELECT * FROM todos ORDER BY created_at DESC",
          () => sql`SELECT * FROM todos ORDER BY created_at DESC`,
        );
        span.setAttribute("http.status_code", 200);
        return sendJson(res, 200, rows);
      }

      // POST /api/todos
      if (path === "/api/todos" && method === "POST") {
        const body = JSON.parse(await readBody(req));
        const title = body?.title;
        if (!title || typeof title !== "string") {
          span.setAttribute("http.status_code", 400);
          return sendJson(res, 400, { error: "title is required" });
        }
        const rows = await dbSpan(
          "INSERT todo",
          "INSERT INTO todos (title) VALUES ($1) RETURNING *",
          () => sql`INSERT INTO todos (title) VALUES (${title}) RETURNING *`,
        );
        span.setAttribute("http.status_code", 201);
        return sendJson(res, 201, rows[0]);
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
          return sendJson(res, 404, { error: "not found" });
        }
        span.setAttribute("http.status_code", 200);
        return sendJson(res, 200, rows[0]);
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
          return sendJson(res, 404, { error: "not found" });
        }
        span.setAttribute("http.status_code", 204);
        res.writeHead(204);
        return res.end();
      }

      span.setAttribute("http.status_code", 404);
      return sendJson(res, 404, { error: "not found" });
    },
    parentCtx,
  );
}

const server = createServer((req, res) => {
  handleRequest(req, res).catch((err) => {
    const message = err instanceof Error ? err.message : String(err);
    if (!res.headersSent) {
      res.writeHead(500, { "content-type": "application/json;charset=utf-8" });
    }
    res.end(JSON.stringify({ error: message }));
  });
});

server.listen(port, () => {
  console.log(`api listening on :${port}`);
});
