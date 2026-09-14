// Local art review server. Run: node tools/art/serve-preview.cjs
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../..');
const types = { '.html': 'text/html; charset=utf-8', '.png': 'image/png', '.gif': 'image/gif', '.json': 'application/json', '.md': 'text/plain; charset=utf-8' };
http.createServer((req, res) => {
  try {
    const pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    const filename = path.resolve(root, '.' + pathname);
    const relative = path.relative(root, filename);
    if (relative.startsWith('..') || path.isAbsolute(relative) || !['assets', 'docs'].includes(relative.split(path.sep)[0])) {
      res.writeHead(403); res.end('Forbidden'); return;
    }
    fs.stat(filename, (error, stat) => {
      if (error || !stat.isFile()) { res.writeHead(404); res.end('Not found'); return; }
      res.writeHead(200, { 'Content-Type': types[path.extname(filename)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
      const stream = fs.createReadStream(filename);
      stream.on('error', () => res.destroy()); stream.pipe(res);
    });
  } catch { res.writeHead(400); res.end('Bad request'); }
}).listen(5181, '127.0.0.1', () => {
  console.log('Review: http://127.0.0.1:5181/assets/Monkey/Baboon%20Chef/v2/animation-review.html');
});
