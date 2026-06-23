import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const frontendRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pluginRoot = path.resolve(frontendRoot, '../..');

/** Resolve hydrocomplete-dag/www for local dev, CI checkout, or sibling clone. */
export function resolveDagWww(): string | null {
  const candidates = [
    process.env.HC_DAG_WWW,
    path.join(pluginRoot, '_dag', 'www'),
    path.join(pluginRoot, 'hydrocomplete-dag', 'www'),
    path.join(pluginRoot, '..', 'hydrocomplete-dag', 'www'),
    path.join(
      process.env.APPDATA ?? '',
      'Autodesk',
      'ApplicationPlugins',
      'HydroComplete.bundle',
      'Contents',
      'dag',
    ),
  ].filter((p): p is string => Boolean(p));

  for (const dir of candidates) {
    if (fs.existsSync(path.join(dir, 'index.html'))) {
      return dir;
    }
  }
  return null;
}

export function resolveDagIndex(): string | null {
  const www = resolveDagWww();
  return www ? path.join(www, 'index.html') : null;
}