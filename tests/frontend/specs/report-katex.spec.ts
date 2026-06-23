import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { test, expect } from '@playwright/test';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const fixturePath = path.resolve(__dirname, '../fixtures/sample-report.html');
const liveReport = process.env.HC_REPORT_HTML;

function reportUrl(): string {
  const chosen = liveReport && fs.existsSync(liveReport) ? liveReport : fixturePath;
  if (!fs.existsSync(chosen)) {
    throw new Error(
      `Report HTML missing: ${chosen}. Run scripts/run_frontend_tests.ps1 to generate the fixture.`,
    );
  }
  return pathToFileURL(chosen).href;
}

async function waitForKaTeX(page: import('@playwright/test').Page) {
  await page.waitForFunction(
    () => typeof (window as Window & { katex?: unknown }).katex !== 'undefined',
    { timeout: 15_000 },
  );
  await page.waitForFunction(
    () => document.querySelectorAll('code.hc-tex-fallback').length === 0,
    { timeout: 15_000 },
  );
}

test.describe('HydroComplete KaTeX HTML report', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(reportUrl());
    await page.waitForLoadState('networkidle');
    await waitForKaTeX(page);
  });

  test('report header and major sections render', async ({ page }) => {
    await expect(page.locator('h1')).toContainText('HydroComplete');
    await expect(page.getByRole('heading', { name: 'Manning Pipe Capacity' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Design Capacity Check' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Steady HGL Profile' })).toBeVisible();
    await expect(page.locator('table')).toHaveCount(3, { timeout: 5_000 });
  });

  test('equation and result labels are visible', async ({ page }) => {
    const equations = page.locator('.hc-formula-equation .hc-formula-label');
    const results = page.locator('.hc-formula-result .hc-formula-label');
    await expect(equations.first()).toHaveText('Equation');
    await expect(results.first()).toHaveText('Result');
    expect(await equations.count()).toBeGreaterThan(3);
    expect(await results.count()).toBeGreaterThan(3);
  });

  test('KaTeX replaces all formula fallback code blocks', async ({ page }) => {
    expect(await page.locator('code.hc-tex-fallback').count()).toBe(0);
    expect(await page.locator('.katex').count()).toBeGreaterThan(6);
    expect(await page.locator('.hc-formula-equation .katex').count()).toBeGreaterThan(2);
    expect(await page.locator('.hc-formula-result .katex').count()).toBeGreaterThan(2);
  });

  test('result panels use distinct styling from equations', async ({ page }) => {
    const equationBg = await page
      .locator('.hc-formula-equation')
      .first()
      .evaluate((el) => getComputedStyle(el).backgroundColor);
    const resultBg = await page
      .locator('.hc-formula-result')
      .first()
      .evaluate((el) => getComputedStyle(el).backgroundColor);
    const resultBorder = await page
      .locator('.hc-formula-result')
      .first()
      .evaluate((el) => getComputedStyle(el).borderLeftColor);

    expect(equationBg).not.toBe(resultBg);
    expect(resultBorder).not.toBe('rgba(0, 0, 0, 0)');
  });

  test('units render as superscript math, not raw LaTeX', async ({ page }) => {
    const bodyText = await page.locator('body').innerText();
    expect(bodyText).not.toContain('\\text{ft^2}');
    expect(bodyText).not.toContain('\\mathrm{ft}^{2}');
    expect(await page.locator('.hc-formula-result .katex .mord').count()).toBeGreaterThan(4);
  });

  test('subscript labels render in results (Q_full)', async ({ page }) => {
    const qFull = page
      .locator('.hc-formula-step[data-label="Q_full"] .hc-formula-result .katex')
      .first();
    await expect(qFull).toBeVisible();
    const resultMath = await qFull.innerText();
    expect(resultMath.toLowerCase()).toMatch(/q/);
    expect(resultMath).toMatch(/=/);
  });
});