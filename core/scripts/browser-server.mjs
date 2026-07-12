import { chromium } from 'playwright';
import { homedir } from 'os';
import { join } from 'path';
import { mkdirSync, unlinkSync, existsSync, writeFileSync } from 'fs';
import { execSync } from 'child_process';
import http from 'http';

const USER_DIR = join(homedir(), 'Library', 'Application Support', 'clawtao', 'browser-profile');
const PORT_FILE = join(homedir(), 'Library', 'Application Support', 'clawtao', 'browser-port');
mkdirSync(USER_DIR, { recursive: true });

// Kill stale Chrome using our profile
execSync(`pkill -f "${USER_DIR}" 2>/dev/null || true`);
// Clear singleton locks
for (const f of ['SingletonLock','SingletonSocket','SingletonCookie'])
  try { unlinkSync(join(USER_DIR, f)); } catch {}

class Lock {
  constructor() { this._p = Promise.resolve(); }
  async acquire() {
    await this._p;
    let release;
    this._p = new Promise(r => { release = r; });
    return release;
  }
}

let browser, page;
const startupLock = new Lock();

async function ensureBrowser() {
  if (browser) return;
  const release = await startupLock.acquire();
  try {
    if (browser) return;
    browser = await chromium.launchPersistentContext(USER_DIR, {
      headless: false,
      viewport: { width: 1280, height: 800 },
      args: ['--no-first-run', '--no-default-browser-check'],
    });
    page = browser.pages()[0] || await browser.newPage();
  } finally {
    release();
  }
}

async function getSnapshot() {
  return await page.evaluate(() => {
    const els = document.querySelectorAll('a, button, input, textarea, select, h1, h2, h3, p, li');
    const seen = new Set(), parts = [];
    els.forEach(el => {
      const text = (el.textContent || '').trim().slice(0, 150);
      if (text && !seen.has(text)) {
        seen.add(text);
        const tag = el.tagName.toLowerCase();
        parts.push(`<${tag}> ${text}${tag === 'a' ? ' → ' + (el.getAttribute('href') || '') : ''}`);
      }
    });
    return parts.join('\n').slice(0, 6000);
  });
}

const TIMEOUT = 20000;

const server = http.createServer(async (req, res) => {
  res.setHeader('Content-Type', 'application/json');
  let body = '';
  req.on('data', d => body += d);
  req.on('end', async () => {
    try {
      const { action, url, selector, text, code } = JSON.parse(body || '{}');
      await ensureBrowser();
      const result = { ok: true };

      switch (action) {
        case 'start':
          await page.goto('about:blank', { timeout: TIMEOUT }).catch(() => {});
          result.message = 'ready'; break;
        case 'stop':
          await browser.close().catch(() => {});
          browser = null; result.message = 'stopped'; break;
        case 'navigate':
        case 'search': {
          const u = (url||'').startsWith('http') ? url : `https://www.google.com/search?q=${encodeURIComponent(url||'')}`;
          await page.goto(u, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
          result.title = await page.title(); result.url = page.url(); break;
        }
        case 'snapshot':
          if (url) await page.goto(url, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
          result.text = await getSnapshot(); result.title = await page.title(); result.url = page.url(); break;
        case 'screenshot':
          if (url) await page.goto(url, { waitUntil: 'domcontentloaded', timeout: TIMEOUT });
          result.screenshot = (await page.screenshot({ type: 'png' })).toString('base64'); break;
        case 'click':
          try { await page.click(selector, { timeout: 5000 }); } catch { await page.getByText(selector).first().click({ timeout: 5000 }); }
          result.title = await page.title(); break;
        case 'type':
          if (text) await page.fill(selector, text, { timeout: 5000 });
          else await page.keyboard.type(selector, { delay: 10 }); break;
        case 'evaluate':
          result.result = await page.evaluate(code || selector); break;
        case 'tabs':
          result.tabs = await Promise.all(browser.pages().map(async (p,i) =>
            ({ index: i, url: p.url(), title: await p.title().catch(() => '') }))); break;
        case 'newTab': {
          const np = await browser.newPage();
          if (url) await np.goto(url.startsWith('http')?url:`https://www.google.com/search?q=${encodeURIComponent(url)}`, { timeout: TIMEOUT });
          result.title = await np.title(); result.url = np.url(); page = np; break;
        }
        default: result.ok = false; result.error = `Unknown: ${action}`;
      }
      res.end(JSON.stringify(result));
    } catch (e) {
      res.end(JSON.stringify({ ok: false, error: e.message }));
    }
  });
});

server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  writeFileSync(PORT_FILE, String(port));
  console.log(`BROWSER_READY:${port}`);
});

process.on('exit', () => { try { execSync(`pkill -f "${USER_DIR}" 2>/dev/null || true`); } catch {} });
process.on('SIGINT', () => process.exit());
process.on('SIGTERM', () => process.exit());
