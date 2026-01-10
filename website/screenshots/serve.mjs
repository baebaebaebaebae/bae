// Custom server that serves the demo web build and covers
import http from 'http';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DEMO_BUILD_DIR = path.join(__dirname, '../../bae/target/dx/demo_web/release/web/public');
const COVERS_DIR = path.join(__dirname, '../fixtures/screenshots/covers');
const PORT = 8080;

const MIME_TYPES = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.wasm': 'application/wasm',
  '.css': 'text/css',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.ico': 'image/x-icon',
  '.svg': 'image/svg+xml',
};

function getMimeType(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return MIME_TYPES[ext] || 'application/octet-stream';
}

function serveFile(res, filePath) {
  fs.readFile(filePath, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end('Not found');
      return;
    }
    res.writeHead(200, { 'Content-Type': getMimeType(filePath) });
    res.end(data);
  });
}

const server = http.createServer((req, res) => {
  let urlPath = req.url.split('?')[0];
  
  // Serve covers from /covers/
  if (urlPath.startsWith('/covers/')) {
    const coverPath = path.join(COVERS_DIR, urlPath.slice(8));
    serveFile(res, coverPath);
    return;
  }
  
  // Serve demo build files
  let filePath = path.join(DEMO_BUILD_DIR, urlPath);
  
  // Default to index.html for directory requests or SPA routing
  if (urlPath === '/' || !fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
    // For SPA routing, always serve index.html for non-asset paths
    if (!urlPath.includes('.') || !fs.existsSync(filePath)) {
      filePath = path.join(DEMO_BUILD_DIR, 'index.html');
    }
  }
  
  serveFile(res, filePath);
});

server.listen(PORT, () => {
  console.log(`Server running at http://localhost:${PORT}`);
  console.log(`Serving demo build from: ${DEMO_BUILD_DIR}`);
  console.log(`Serving covers from: ${COVERS_DIR}`);
});
