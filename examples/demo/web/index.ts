import { join } from "path";
import {
  extractContext,
  withSpan,
  injectHeaders,
  SpanKind,
  context,
} from "../shared/tracing";

const port = parseInt(process.env.PORT || "3000");
const apiUrl = process.env.DEVRIG_API_URL || "http://localhost:3001";
const publicDir = join(import.meta.dir, "public");

async function handleRequest(req: Request): Promise<Response> {
  const url = new URL(req.url);
  const path = url.pathname;
  const parentCtx = extractContext(req.headers);

  // Proxy /api/* to the api service
  if (path.startsWith("/api/")) {
    return withSpan(
      `proxy ${req.method} ${path}`,
      SpanKind.SERVER,
      async (serverSpan) => {
        serverSpan.setAttribute("http.method", req.method);
        serverSpan.setAttribute("http.url", path);

        const response = await withSpan(
          `${req.method} ${apiUrl}${path}`,
          SpanKind.CLIENT,
          async (clientSpan) => {
            clientSpan.setAttribute("http.method", req.method);
            clientSpan.setAttribute("http.url", `${apiUrl}${path}`);

            const headers = new Headers(req.headers);
            const traceHeaders = injectHeaders();
            for (const [k, v] of Object.entries(traceHeaders)) {
              headers.set(k, v);
            }

            const upstream = await fetch(`${apiUrl}${path}`, {
              method: req.method,
              headers,
              body: req.body,
            });

            clientSpan.setAttribute("http.status_code", upstream.status);
            return upstream;
          },
        );

        serverSpan.setAttribute("http.status_code", response.status);

        return new Response(response.body, {
          status: response.status,
          headers: response.headers,
        });
      },
      parentCtx,
    );
  }

  // Serve static files
  const filePath = path === "/" ? "/index.html" : path;
  const file = Bun.file(join(publicDir, filePath));
  if (await file.exists()) {
    return new Response(file);
  }

  return new Response("Not Found", { status: 404 });
}

Bun.serve({
  port,
  fetch: handleRequest,
});

console.log(`web listening on :${port}`);
