// Renders the Open Graph social cards to 1200x630 PNGs with Chromium.
// Requires Playwright (`npm i -g playwright` or a local install).
// Regenerate after editing general.html / bulletin.html:
//   node web/og-cards/render.mjs
// Outputs: web/og-glossia.png, web/og-bulletin.png
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
// Resolve Playwright whether it's a local dependency or a global install.
let chromium;
try {
  ({ chromium } = require('playwright'));
} catch {
  const path = process.env.PLAYWRIGHT_PATH
    || require('child_process').execSync('npm root -g').toString().trim() + '/playwright';
  ({ chromium } = require(path));
}
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const here = dirname(fileURLToPath(import.meta.url));
const web = join(here, '..');

const CARDS = [
  { html: 'general.html', out: 'og-glossia.png' },
  { html: 'bulletin.html', out: 'og-bulletin.png' },
];

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 });
for (const { html, out } of CARDS) {
  await page.goto('file://' + join(here, html));
  await page.waitForTimeout(150);
  await page.screenshot({ path: join(web, out), clip: { x: 0, y: 0, width: 1200, height: 630 } });
  console.log('wrote', out);
}
await browser.close();
