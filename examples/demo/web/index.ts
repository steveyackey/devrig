import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  extractContext,
  withSpan,
  injectHeaders,
  SpanKind,
} from "../shared/tracing";

const port = parseInt(process.env.PORT || "3000");
const apiUrl = process.env.DEVRIG_API_URL || "http://localhost:3001";
const publicDir = join(dirname(fileURLToPath(import.meta.url)), "public");

const mimeTypes: Record<string, string> = {
  ".html": "text/html;charset=utf-8",
  ".js": "text/javascript;charset=utf-8",
  ".css": "text/css;charset=utf-8",
  ".json": "application/json;charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".ico": "image/x-icon",
  ".txt": "text/plain;charset=utf-8",
};

function readBody(req: IncomingMessage): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk: Buffer) => chunks.push(chunk));
    req.on("end", () => resolve(Buffer.concat(chunks)));
    req.on("error", reject);
  });
}

async function handleRequest(req: IncomingMessage, res: ServerResponse): Promise<void> {
  const url = new URL(req.url ?? "/", "http://localhost");
  const path = url.pathname;
  const method = req.method ?? "GET";
  const parentCtx = extractContext(req.headers);

  // Proxy /api/* to the api service
  if (path.startsWith("/api/")) {
    return withSpan(
      `proxy ${method} ${path}`,
      SpanKind.SERVER,
      async (serverSpan) => {
        serverSpan.setAttribute("http.method", method);
        serverSpan.setAttribute("http.url", path);

        const response = await withSpan(
          `${method} ${apiUrl}${path}`,
          SpanKind.CLIENT,
          async (clientSpan) => {
            clientSpan.setAttribute("http.method", method);
            clientSpan.setAttribute("http.url", `${apiUrl}${path}`);

            const headers = new Headers();
            for (const [k, v] of Object.entries(req.headers)) {
              if (v === undefined) continue;
              if (k === "host" || k === "connection" || k === "content-length") continue;
              headers.set(k, Array.isArray(v) ? v.join(", ") : v);
            }
            const traceHeaders = injectHeaders();
            for (const [k, v] of Object.entries(traceHeaders)) {
              headers.set(k, v);
            }

            const body =
              method === "GET" || method === "HEAD" ? undefined : await readBody(req);

            const upstream = await fetch(`${apiUrl}${path}`, {
              method,
              headers,
              body,
            });

            clientSpan.setAttribute("http.status_code", upstream.status);
            return upstream;
          },
        );

        serverSpan.setAttribute("http.status_code", response.status);

        const responseHeaders: Record<string, string> = {};
        response.headers.forEach((v, k) => {
          if (k === "content-encoding" || k === "content-length" || k === "transfer-encoding" || k === "connection") {
            return;
          }
          responseHeaders[k] = v;
        });
        res.writeHead(response.status, responseHeaders);
        const responseBody = Buffer.from(await response.arrayBuffer());
        res.end(responseBody.length > 0 ? responseBody : undefined);
      },
      parentCtx,
    );
  }

  // Serve static files
  const filePath = path === "/" ? "/index.html" : path;
  try {
    const contents = await readFile(join(publicDir, filePath));
    const contentType = mimeTypes[extname(filePath).toLowerCase()] ?? "application/octet-stream";
    res.writeHead(200, { "content-type": contentType });
    res.end(contents);
    return;
  } catch {
    res.writeHead(404, { "content-type": "text/plain;charset=utf-8" });
    res.end("Not Found");
  }
}

const server = createServer((req, res) => {
  handleRequest(req, res).catch((err) => {
    const message = err instanceof Error ? err.message : String(err);
    if (!res.headersSent) {
      res.writeHead(500, { "content-type": "text/plain;charset=utf-8" });
    }
    res.end(message);
  });
});

server.listen(port, () => {
  console.log(`web listening on :${port}`);
});
