export interface OtlpSendOptions {
  url: string;
  retries?: number;
}

export async function sendOtlp(
  path: string,
  payload: unknown,
  opts: OtlpSendOptions,
): Promise<void> {
  const retries = opts.retries ?? 2;
  const url = `${opts.url}${path}`;

  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (res.ok) return;
      const body = await res.text().catch(() => "");
      if (attempt === retries) {
        console.error(`OTLP ${path} failed: ${res.status} ${body}`);
      }
    } catch (err) {
      if (attempt === retries) {
        console.error(`OTLP ${path} error:`, err);
      }
    }
    // brief backoff
    if (attempt < retries) {
      await new Promise((r) => setTimeout(r, 200 * (attempt + 1)));
    }
  }
}
