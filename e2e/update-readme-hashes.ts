/**
 * Post-screenshot script: renames each screenshot to include a content hash
 * in the filename, deletes stale hashed copies, and updates README.md refs.
 *
 * Flow:
 *   1. Playwright writes  docs/images/dashboard-traces.png  (unhashed)
 *   2. This script hashes the file → dashboard-traces-a1b2c3d4.png
 *   3. Deletes the unhashed original + any old hashed versions
 *   4. Updates README.md:  ![...](docs/images/dashboard-traces-a1b2c3d4.png)
 *
 * If the image content hasn't changed, the hash is the same → no file rename,
 * no README diff.
 */

import { readFileSync, writeFileSync, existsSync, renameSync, unlinkSync, readdirSync } from "fs";
import { createHash } from "crypto";
import { resolve, basename } from "path";

const ROOT = resolve(__dirname, "..");
const README = resolve(ROOT, "README.md");
const IMAGES_DIR = resolve(ROOT, "docs/images");

// The base names that screenshots are generated as (without hash)
const SCREENSHOT_BASES = [
  "dashboard-traces",
  "dashboard-trace-detail",
  "dashboard-logs",
  "dashboard-metrics",
  "dashboard-status",
];

function shortHash(filePath: string): string {
  const buf = readFileSync(filePath);
  return createHash("sha256").update(buf).digest("hex").slice(0, 8);
}

function main() {
  if (!existsSync(README)) {
    console.error("README.md not found at", README);
    process.exit(1);
  }

  let readmeContent = readFileSync(README, "utf-8");
  const allFiles = readdirSync(IMAGES_DIR);
  let updated = 0;

  for (const base of SCREENSHOT_BASES) {
    const unhashed = resolve(IMAGES_DIR, `${base}.png`);

    if (!existsSync(unhashed)) {
      // No fresh screenshot — nothing to do for this base
      console.log(`  skip: ${base}.png (not found, keeping existing hashed version)`);
      continue;
    }

    const hash = shortHash(unhashed);
    const hashedName = `${base}-${hash}.png`;
    const hashedPath = resolve(IMAGES_DIR, hashedName);

    // Delete old hashed versions of this screenshot
    const oldPattern = new RegExp(`^${base}-[0-9a-f]{8}\\.png$`);
    for (const file of allFiles) {
      if (oldPattern.test(file) && file !== hashedName) {
        unlinkSync(resolve(IMAGES_DIR, file));
        console.log(`  deleted: ${file}`);
      }
    }

    // Rename unhashed → hashed (skip if already identical)
    if (existsSync(hashedPath)) {
      // Same content, just remove the unhashed original
      unlinkSync(unhashed);
    } else {
      renameSync(unhashed, hashedPath);
    }
    console.log(`  ${base}.png → ${hashedName}`);

    // Update README: match any existing reference to this base (hashed or unhashed, with or without query param)
    const readmePattern = new RegExp(
      `(!\\[[^\\]]*\\]\\()docs/images/${base}(?:-[0-9a-f]{8})?\\.png(?:\\?[^)]*)?\\)`,
      "g",
    );
    readmeContent = readmeContent.replace(readmePattern, (match, prefix) => {
      updated++;
      return `${prefix}docs/images/${hashedName})`;
    });
  }

  writeFileSync(README, readmeContent);
  console.log(`\nUpdated ${updated} image reference(s) in README.md`);
}

main();
