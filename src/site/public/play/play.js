// Loads the browser build named in /release.json ("web"), with progress.
const $ = (id) => document.getElementById(id);
const status = (t) => { $('play-status').textContent = t; };

async function buildPath() {
  const release = await fetch('/release.json', { cache: 'no-store' }).then(r => r.json());
  const js = release.web?.js;
  if (!js || !js.startsWith('/downloads/web/')) return null;
  return { js, wasm: js.replace(/peakrunner\.js$/, 'peakrunner_bg.wasm'), version: release.web.version };
}

// Fetch the wasm with a progress readout, then hand the bytes to the module.
async function fetchWasm(url) {
  const r = await fetch(url);
  if (!r.ok) throw Error(`HTTP ${r.status}`);
  const total = Number(r.headers.get('content-length')) || 0;
  if (!r.body || !total) return r.arrayBuffer();
  const reader = r.body.getReader(), parts = []; let got = 0;
  for (;;) {
    const { done, value } = await reader.read(); if (done) break;
    parts.push(value); got += value.length;
    status(`Downloading… ${Math.round(got / 1e6)} of ${Math.round(total / 1e6)} MB`);
  }
  const bytes = new Uint8Array(got); let at = 0; for (const p of parts) { bytes.set(p, at); at += p.length; }
  return bytes.buffer;
}

async function start() {
  const button = $('play-start'); button.disabled = true;
  try {
    if (!('WebAssembly' in window)) throw Error('This browser cannot run WebAssembly.');
    const probe = document.createElement('canvas');
    if (!navigator.gpu && !probe.getContext('webgl2')) throw Error('This browser has neither WebGPU nor WebGL 2 enabled.');
    const build = await buildPath();
    if (!build) throw Error('The browser version is not published yet. Check back soon, or get the desktop game.');
    status('Loading…');
    const module = await import(build.js);
    const wasm = await fetchWasm(build.wasm);
    status('Starting…');
    await module.default({ module_or_path: wasm });
    $('play-overlay').hidden = true;
    $('peakrunner_canvas').focus();
  } catch (e) {
    status(e.message || String(e)); button.disabled = false;
  }
}
$('play-start').addEventListener('click', start);
if (new URLSearchParams(location.search).has('autostart')) start();
