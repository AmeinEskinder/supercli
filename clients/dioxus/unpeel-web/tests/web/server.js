// Minimal static file server for the Playwright suite. Node builtins only —
// no extra dependencies. Serves the `dx build --platform web` output with
// the MIME types a Dioxus web bundle needs (notably application/wasm).
//
// DIST_DIR env overrides the served directory; otherwise the dx 0.7 output
// locations are probed (see CANDIDATES below).
const http = require('http');
const fs = require('fs');
const path = require('path');

// `dx build --platform web` (dioxus-cli 0.7) writes the bundle to
// `<cargo-target-dir>/dx/<package>/<profile>/web/public`. Candidates are
// probed in order; DIST_DIR env wins outright.
const CANDIDATES = [
  process.env.DIST_DIR,
  // Default cargo target dir for the clients/dioxus workspace.
  path.resolve(__dirname, '..', '..', '..', 'target', 'dx', 'unpeel-web', 'debug', 'web', 'public'),
  // Legacy fallback.
  path.resolve(__dirname, '..', '..', 'dist'),
].filter(Boolean);

const DIST = CANDIDATES.find((d) => {
  try {
    return fs.statSync(d).isDirectory() && fs.existsSync(path.join(d, 'index.html'));
  } catch {
    return false;
  }
});
if (!DIST) {
  console.error('no web bundle found; build with `dx build --platform web` or set DIST_DIR');
  console.error('tried:', CANDIDATES.join('\n  '));
  process.exit(1);
}
const PORT = parseInt(process.env.PORT || '8327', 10);

const MIME = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.mjs': 'text/javascript',
  '.wasm': 'application/wasm',
  '.css': 'text/css',
  '.json': 'application/json',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
};

const server = http.createServer((req, res) => {
  let urlPath = decodeURIComponent(req.url.split('?')[0]);
  if (urlPath.endsWith('/')) urlPath += 'index.html';
  const file = path.normalize(path.join(DIST, urlPath));
  if (!file.startsWith(DIST)) {
    res.writeHead(403);
    res.end('forbidden');
    return;
  }
  fs.readFile(file, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end('not found');
      return;
    }
    res.writeHead(200, {
      'Content-Type': MIME[path.extname(file).toLowerCase()] || 'application/octet-stream',
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    });
    res.end(data);
  });
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`serving ${DIST} on http://127.0.0.1:${PORT}`);
});
