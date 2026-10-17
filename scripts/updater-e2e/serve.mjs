// Updater end-to-end test: a static file server on 127.0.0.1 standing in for GitHub Releases. Logs every request
// (the test checks that the installer was downloaded). No dependencies.
//   node scripts/updater-e2e/serve.mjs <root folder> <port>
import { createReadStream, statSync } from "node:fs";
import { createServer } from "node:http";
import { join, normalize, resolve, sep } from "node:path";

const [rootArg, portArg] = process.argv.slice(2);
if (!rootArg || !portArg) {
  console.error("usage: node scripts/updater-e2e/serve.mjs <root folder> <port>");
  process.exit(1);
}
const root = resolve(rootArg);

createServer((req, res) => {
  const path = decodeURIComponent(new URL(req.url, "http://127.0.0.1").pathname);
  const file = normalize(join(root, path));
  let size = -1;
  try {
    if (file.startsWith(root + sep)) size = statSync(file).isFile() ? statSync(file).size : -1;
  } catch {
    size = -1;
  }
  const status = size < 0 ? 404 : 200;
  console.log(`${new Date().toISOString()} ${req.method} ${path} ${status}${size >= 0 ? ` ${size}` : ""}`);
  if (status === 404) return res.writeHead(404).end("not found");
  const type = file.endsWith(".json") ? "application/json" : "application/octet-stream";
  res.writeHead(200, { "content-type": type, "content-length": size });
  if (req.method === "HEAD") return res.end();
  createReadStream(file).pipe(res);
}).listen(Number(portArg), "127.0.0.1", () => console.log(`serving ${root} on http://127.0.0.1:${portArg}`));
