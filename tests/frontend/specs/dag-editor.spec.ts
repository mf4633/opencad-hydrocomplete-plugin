import fs from 'node:fs';
import { pathToFileURL } from 'node:url';
import { test, expect } from '@playwright/test';
import { resolveDagIndex, resolveDagWww } from '../lib/dag-www.js';

const devDagIndex = resolveDagIndex();
const devDagWww = resolveDagWww();

/** WASM palette requires HTTP; use Playwright webServer when hydrocomplete-dag is present. */
function dagPageUrl(filePath: string, needsWasm: boolean): string {
  if (needsWasm && fs.existsSync(devDagIndex)) {
    return 'http://127.0.0.1:4173/';
  }
  return pathToFileURL(filePath).href;
}

test.describe('HydroComplete DAG model builder', () => {
  test('loads toolbar, palette, and canvas', async ({ page }) => {
    const dagPath = resolveDagIndex();
    test.skip(!dagPath, 'DAG index.html not found (hydrocomplete-dag or Civil 3D bundle)');

    const useWasmServer = Boolean(devDagWww);
    await page.goto(dagPageUrl(dagPath!, useWasmServer));
    await page.waitForLoadState('domcontentloaded');

    await expect(page.locator('#toolbar h1')).toContainText('Model Builder');
    await expect(page.locator('#dag-canvas')).toBeVisible();
    await expect(page.locator('#palette')).toBeVisible();
    await expect(page.locator('#palette-search')).toBeVisible();
    await expect(page.locator('.tbtn.run')).toBeVisible();

    if (useWasmServer) {
      await page.waitForFunction(() => window.__editor != null);
      await expect(page.locator('#palette .pnode')).not.toHaveCount(0);
      await expect(page.locator('#palette .cat-hdr')).not.toHaveCount(0);
    }
  });

  test('WASM palette_json exposes hydrology node catalog', async ({ page }) => {
    test.skip(!devDagWww, 'hydrocomplete-dag dev server fixture not available');

    await page.goto('http://127.0.0.1:4173/');
    await page.waitForLoadState('domcontentloaded');

    const palette = await page.evaluate(async () => {
      const mod = await import('./pkg/hydrocomplete_dag.js');
      await mod.default();
      return JSON.parse(mod.DagEditor.palette_json()) as {
        category: string;
        label: string;
        kind: string;
      }[];
    });

    expect(palette.length).toBeGreaterThan(3);
    const categories = new Set(palette.map((n) => n.category));
    expect(categories.size).toBeGreaterThan(1);
    expect(palette[0].label.length).toBeGreaterThan(0);
    expect(palette[0].kind.length).toBeGreaterThan(0);
  });
});