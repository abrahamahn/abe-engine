// Headless browser test of the ABE web player.
//
//   node web/test/run.cjs --dist web/dist --out OUT_DIR [--abe-render PATH] \
//     --case name=NAME,scene=WORLD.abew,look=LOOK.json,view=VIEW,time=HOURS,weather=PRESET,size=WxH
//     [--case ...]
//
// For each case it serves web/dist plus the case's world and Look file (from where they are,
// under /assets/), opens the player with the view, time and weather fixed and the animation
// clock paused at `seconds` (default 12, abe-render's default), waits for `frames` presented
// frames (default 3), and saves the canvas pixels (read back from the canvas texture) as
// OUT/NAME-web.png and a page screenshot as OUT/NAME-page.png. With --abe-render it first
// renders the same view natively (one sample, realtime quality) to OUT/NAME-native.png and
// compares the two: OUT/NAME-diff.png shows |web - native| x 4. Console messages and WebGPU
// errors are written to OUT/NAME-console.txt; any WebGPU error fails the case, and so does a
// mean absolute difference above `max_mean` (default 3 levels of 255).
//
'use strict';
const fs = require('fs');
const http = require('http');
const path = require('path');
const { execFileSync } = require('child_process');
const { chromium } = require('playwright');

// WebGPU in headless Chromium. Without a GPU it runs on SwiftShader; canvas presentation then
// needs Vulkan compositing (with the default software compositing the GPU process fails to
// allocate the canvas swap-chain images and drops the WebGPU instance after a few frames).
const CHROMIUM_ARGS = ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan',
  '--use-vulkan=swiftshader'];

function parseArgs(argv) {
  const opts = { cases: [], dist: path.join(__dirname, '..', 'dist'), out: 'web-test-out', abeRender: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = () => argv[++i];
    if (a === '--dist') opts.dist = val();
    else if (a === '--out') opts.out = val();
    else if (a === '--abe-render') opts.abeRender = val();
    else if (a === '--case') {
      const c = { seconds: '12', frames: '3', size: '960x540', max_mean: '3' };
      for (const kv of val().split(',')) {
        const [k, ...v] = kv.split('=');
        c[k] = v.join('=');
      }
      if (!c.name || !c.scene) throw new Error(`a case needs name= and scene=: ${argv[i]}`);
      opts.cases.push(c);
    } else throw new Error(`unknown argument ${a}`);
  }
  if (!opts.cases.length) throw new Error('no --case given');
  return opts;
}

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm',
  '.json': 'application/json', '.png': 'image/png', '.abew': 'application/octet-stream' };

function serve(dist, assets) {
  const server = http.createServer((req, res) => {
    const url = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    let file = null;
    if (url.startsWith('/assets/')) file = assets.get(url.slice('/assets/'.length));
    else {
      const rel = url === '/' ? 'index.html' : url.slice(1);
      const p = path.resolve(dist, rel);
      if (p.startsWith(path.resolve(dist))) file = p;
    }
    if (!file || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      res.writeHead(404);
      res.end('not found');
      return;
    }
    const size = fs.statSync(file).size;
    res.writeHead(200, {
      'content-type': TYPES[path.extname(file)] || 'application/octet-stream',
      'content-length': size,
      'cache-control': 'no-store',
    });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}

function native(opts, c, out) {
  const [w, h] = c.size.split('x');
  const args = ['--scene', c.scene, '--width', w, '--height', h, '--samples', '1',
    '--realtime-quality', '--seconds', c.seconds, '--out', out];
  if (c.look) args.push('--look', c.look);
  if (c.view) args.push('--view', c.view);
  if (c.time) args.push('--time', c.time);
  if (c.weather) args.push('--weather', c.weather);
  const started = Date.now();
  execFileSync(opts.abeRender, args, { stdio: ['ignore', 'inherit', 'inherit'] });
  return (Date.now() - started) / 1000;
}

async function runCase(browser, port, opts, c, assets) {
  const log = [];
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 }, deviceScaleFactor: 1 });
  page.on('console', (m) => log.push(`[${m.type()}] ${m.text()}`));
  page.on('pageerror', (e) => log.push(`[pageerror] ${e.message}`));
  const q = new URLSearchParams({ scene: `/assets/${c.name}.abew`, size: c.size, dpr: '1',
    seconds: c.seconds, paused: '1' });
  if (c.look) q.set('look', `/assets/${c.name}.json`);
  for (const k of ['view', 'time', 'weather', 'quality']) if (c[k]) q.set(k, c[k]);
  const url = `http://localhost:${port}/index.html?${q}`;
  const started = Date.now();
  try {
    return await drive(page, url, started, opts, c, assets, log);
  } finally {
    fs.writeFileSync(path.join(opts.out, `${c.name}-console.txt`), `${log.join('\n')}\n`);
    await page.close();
  }
}

async function drive(page, url, started, opts, c, assets, log) {
  await page.goto(url);
  const timeout = Number(c.timeout || 600) * 1000;
  await page.waitForFunction((n) => window.abe && (window.abe.failed || window.abe.frames() >= n
    || window.abe.errors().length > 0), Number(c.frames), { timeout, polling: 250 });
  const state = await page.evaluate(() => ({
    failed: window.abe.failed || null,
    frames: window.abe.failed ? 0 : window.abe.frames(),
    errors: window.abe.failed ? [] : window.abe.errors(),
    adapter: window.abe.adapter || '',
    status: document.getElementById('status').textContent,
    userAgent: navigator.userAgent,
  }));
  const result = { name: c.name, url, seconds: (Date.now() - started) / 1000, ...state };
  if (!state.failed) {
    // Pixels of one more frame, read back from the canvas texture.
    const png = await page.evaluate(async () => {
      const img = await window.abe.capture();
      const cv = document.createElement('canvas');
      cv.width = img.width; cv.height = img.height;
      cv.getContext('2d').putImageData(new ImageData(img.rgba, img.width, img.height), 0, 0);
      window.lastCapture = img;
      return cv.toDataURL('image/png').split(',')[1];
    });
    const webPng = path.join(opts.out, `${c.name}-web.png`);
    fs.writeFileSync(webPng, Buffer.from(png, 'base64'));
    result.web = webPng;
    // Software rendering can take seconds per frame; stop the loop so the page settles.
    await page.evaluate(() => window.abe.stop());
    await page.screenshot({ path: path.join(opts.out, `${c.name}-page.png`), timeout: 120000 });
    result.errors = await page.evaluate(() => window.abe.errors());
    if (c.native) {
      assets.set(`${c.name}-native.png`, c.native);
      const cmp = await page.evaluate(async (src) => {
        const im = new Image();
        im.src = src;
        await im.decode();
        const a = window.lastCapture;
        if (im.width !== a.width || im.height !== a.height) return { error: `size ${im.width}x${im.height} vs ${a.width}x${a.height}` };
        const cv = document.createElement('canvas');
        cv.width = a.width; cv.height = a.height;
        const g = cv.getContext('2d');
        g.drawImage(im, 0, 0);
        const b = g.getImageData(0, 0, a.width, a.height).data;
        const diff = new Uint8ClampedArray(a.rgba.length);
        let sum = 0; let sq = 0; let over8 = 0; let over32 = 0;
        const n = a.width * a.height;
        for (let i = 0; i < n; i++) {
          let worst = 0;
          for (let k = 0; k < 3; k++) {
            const d = Math.abs(a.rgba[i * 4 + k] - b[i * 4 + k]);
            sum += d; sq += d * d;
            worst = Math.max(worst, d);
            diff[i * 4 + k] = Math.min(255, d * 4);
          }
          diff[i * 4 + 3] = 255;
          if (worst > 8) over8++;
          if (worst > 32) over32++;
        }
        g.putImageData(new ImageData(diff, a.width, a.height), 0, 0);
        const mse = sq / (n * 3);
        return {
          mean: sum / (n * 3),
          psnr: mse > 0 ? 10 * Math.log10(255 * 255 / mse) : Infinity,
          over8: over8 / n,
          over32: over32 / n,
          png: cv.toDataURL('image/png').split(',')[1],
        };
      }, `/assets/${c.name}-native.png`);
      if (cmp.png) {
        fs.writeFileSync(path.join(opts.out, `${c.name}-diff.png`), Buffer.from(cmp.png, 'base64'));
        delete cmp.png;
      }
      result.compare = cmp;
    }
  }
  result.ok = !result.failed && result.frames >= Number(c.frames) && result.errors.length === 0
    && (!result.compare || (!result.compare.error && result.compare.mean <= Number(c.max_mean)));
  return result;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  fs.mkdirSync(opts.out, { recursive: true });
  const assets = new Map();
  for (const c of opts.cases) {
    assets.set(`${c.name}.abew`, path.resolve(c.scene));
    if (c.look) assets.set(`${c.name}.json`, path.resolve(c.look));
    if (opts.abeRender && c.native !== '0') {
      const out = path.join(opts.out, `${c.name}-native.png`);
      const s = native(opts, c, out);
      console.log(`${c.name}: native reference ${out} (${s.toFixed(1)} s)`);
      c.native = out;
    } else if (c.native && c.native !== '0') {
      c.native = path.resolve(c.native);
    } else {
      delete c.native;
    }
  }
  const server = await serve(path.resolve(opts.dist), assets);
  const port = server.address().port;
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  console.log(`browser: Chromium ${browser.version()} (headless)`);
  const results = [];
  for (const c of opts.cases) {
    let r;
    try {
      r = await runCase(browser, port, opts, c, assets);
    } catch (e) {
      r = { name: c.name, ok: false, failed: e.message };
    }
    results.push(r);
    console.log(JSON.stringify(r, null, 2));
  }
  await browser.close();
  server.close();
  fs.writeFileSync(path.join(opts.out, 'results.json'), `${JSON.stringify(results, null, 2)}\n`);
  const failed = results.filter((r) => !r.ok).map((r) => r.name);
  console.log(failed.length ? `FAILED: ${failed.join(', ')}` : `all ${results.length} cases passed`);
  process.exit(failed.length ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
