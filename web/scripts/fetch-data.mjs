// Downloads the generated stats JSON (produced weekly by the GitHub Action)
// from a GitHub Release asset and extracts it into public/data.
// Runs as part of `npm run build` (i.e. on every Vercel build).
//
// Override the source with DATA_URL if needed.

import AdmZip from 'adm-zip';
import { existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const DATA_URL =
  process.env.DATA_URL ||
  'https://github.com/maxjiang216/wca_stats/releases/download/data-latest/wca_stats_data.zip';

const here = dirname(fileURLToPath(import.meta.url));
const dataDir = join(here, '..', 'public', 'data');

function hasJson(dir) {
  try {
    return readdirSync(dir).some((f) => f.endsWith('.json'));
  } catch {
    return false;
  }
}

async function main() {
  console.log(`[fetch-data] downloading ${DATA_URL}`);
  let buf;
  try {
    const res = await fetch(DATA_URL);
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
    buf = Buffer.from(await res.arrayBuffer());
  } catch (e) {
    // Tolerate failure if data already present (e.g. local dev via the
    // public/data -> ../../out symlink). Otherwise the build cannot proceed.
    if (hasJson(dataDir)) {
      console.warn(`[fetch-data] download failed (${e}); using existing data in public/data`);
      return;
    }
    throw new Error(`[fetch-data] download failed and no local data present: ${e}`);
  }

  // Replace whatever is at public/data (symlink in local dev, stale dir on CI)
  // with a fresh real directory.
  if (existsSync(dataDir) || isSymlink(dataDir)) {
    rmSync(dataDir, { recursive: true, force: true });
  }
  mkdirSync(dataDir, { recursive: true });

  new AdmZip(buf).extractAllTo(dataDir, /* overwrite */ true);
  const n = readdirSync(dataDir).filter((f) => f.endsWith('.json')).length;
  console.log(`[fetch-data] extracted ${n} JSON file(s) to public/data`);
}

function isSymlink(p) {
  try {
    return lstatSync(p).isSymbolicLink();
  } catch {
    return false;
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
