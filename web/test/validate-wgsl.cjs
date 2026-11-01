// Compiles WGSL files with a real browser WebGPU implementation and reports every message.
//
//   node web/test/validate-wgsl.cjs DIR...
//
// Each module is compiled with createShaderModule and checked with getCompilationInfo(); a
// module with errors (or a validation error) fails the run. Warnings are printed. Needs
// Playwright's Chromium; WebGPU is enabled with --enable-unsafe-webgpu (SwiftShader when no
// GPU is available).
'use strict';
const fs = require('fs');
const http = require('http');
const path = require('path');
const { chromium } = require('playwright');

// WebGPU in headless Chromium. Without a GPU it runs on SwiftShader; canvas presentation then
// needs Vulkan compositing (with the default software compositing the GPU process fails to
// allocate the canvas swap-chain images and drops the WebGPU instance after a few frames).
// SwiftShader is slow, so tests wait for frame counts rather than for time.
const CHROMIUM_ARGS = ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan',
  '--use-vulkan=swiftshader'];

async function main() {
  const dirs = process.argv.slice(2);
  if (!dirs.length) {
    console.error('usage: node validate-wgsl.cjs DIR...');
    process.exit(2);
  }
  const files = dirs.flatMap((d) =>
    fs.readdirSync(d).filter((f) => f.endsWith('.wgsl')).sort().map((f) => path.join(d, f)));
  const server = http.createServer((req, res) => {
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end('<!doctype html><title>wgsl</title>');
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const url = `http://localhost:${server.address().port}/`;
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const page = await browser.newPage();
  await page.goto(url);
  const adapter = await page.evaluate(async () => {
    const a = await navigator.gpu.requestAdapter();
    window.device = await a.requestDevice();
    return `${a.info.vendor} ${a.info.architecture} ${a.info.description}`.trim();
  });
  console.log(`browser ${browser.version()}, adapter ${adapter}`);
  let failed = 0;
  for (const file of files) {
    const code = fs.readFileSync(file, 'utf8');
    const result = await page.evaluate(async (code) => {
      device.pushErrorScope('validation');
      const module = device.createShaderModule({ code });
      const info = await module.getCompilationInfo();
      const error = await device.popErrorScope();
      return {
        messages: info.messages.map((m) => ({ type: m.type, line: m.lineNum, col: m.linePos, text: m.message })),
        error: error ? error.message : null,
      };
    }, code);
    const errors = result.messages.filter((m) => m.type === 'error');
    const ok = !errors.length && !result.error;
    if (!ok) failed++;
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${file}`);
    const lines = code.split('\n');
    for (const m of result.messages) {
      console.log(`     ${m.type} ${m.line}:${m.col} ${m.text}`);
      if (m.line > 0) console.log(`       | ${lines[m.line - 1]}`);
    }
    if (result.error && !errors.length) console.log(`     ${result.error}`);
  }
  await browser.close();
  server.close();
  console.log(`${files.length - failed}/${files.length} modules compiled without errors`);
  process.exit(failed ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
