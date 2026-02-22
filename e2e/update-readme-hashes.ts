/**
 * Post-screenshot script: patches README.md image references with a
 * content-hash query param so GitHub's CDN cache is busted when images change.
 *
 * Image refs like:
 *   ![Traces](docs/images/dashboard-traces.png)
 *   ![Traces](docs/images/dashboard-traces.png?h=abc123)
 * become:
 *   ![Traces](docs/images/dashboard-traces.png?h=<new_hash>)
 *
 * If the image hasn't changed, the hash stays the same → no README diff.
 */

import { readFileSync, writeFileSync, existsSync } from "fs";
import { createHash } from "crypto";
import { resolve } from "path";

const ROOT = resolve(__dirname, "..");
const README = resolve(ROOT, "README.md");

function shortHash(filePath: string): string {
  const buf = readFileSync(filePath);
  return createHash("sha256").update(buf).digest("hex").slice(0, 8);
}

function main() {
  if (!existsSync(README)) {
    console.error("README.md not found at", README);
    process.exit(1);
  }

  let content = readFileSync(README, "utf-8");
  let updated = 0;

  // Match markdown image refs pointing to docs/images/*.png with optional query
  content = content.replace(
    /!\[([^\]]*)\]\((docs\/images\/[^)?\s]+\.png)(?:\?[^)]*)?\)/g,
    (_match, alt, imgPath) => {
      const absPath = resolve(ROOT, imgPath);
      if (!existsSync(absPath)) {
        console.warn(`  skip: ${imgPath} (file not found)`);
        return _match;
      }
      const hash = shortHash(absPath);
      updated++;
      return `![${alt}](${imgPath}?h=${hash})`;
    },
  );

  writeFileSync(README, content);
  console.log(`Updated ${updated} image reference(s) in README.md`);
}

main();
