// Runs the site's installed Vite with its own config. Network settings belong
// to Werd; no project config or npm script is rewritten.
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";

const [folder, portText, httpsText, originsText, readyFile] = process.argv.slice(2);
const require = createRequire(path.join(folder, "package.json"));
const vitePackage = require.resolve("vite/package.json");
const { createServer } = await import(pathToFileURL(path.join(path.dirname(vitePackage), "dist/node/index.js")));
const port = Number(portText);
const httpsPort = Number(httpsText);
const origin = `https://localhost:${httpsPort}`;
const hotFile = path.join(folder, "public", "hot");

// Check here too, immediately before Vite can write the hot file.
if (fs.existsSync(hotFile)) {
  throw new Error("public/hot already exists. Stop your other Vite server; remove the file only if it is stale.");
}
const network = {
  host: "127.0.0.1",
  port,
  strictPort: true,
  https: false,
  origin,
  open: false,
  allowedHosts: ["localhost"],
  cors: { origin: [...JSON.parse(originsText), /^https:\/\/(?:localhost|[a-z0-9.-]+\.test)(?::\d+)?$/] },
  hmr: { protocol: "wss", host: "localhost", clientPort: httpsPort, port },
};
const server = await createServer({
  root: folder,
  clearScreen: false,
  server: network,
  plugins: [
    {
      name: "werd-network",
      enforce: "post",
      configResolved(config) {
        if (!config.plugins.some((plugin) => plugin.name === "laravel")) {
          throw new Error("Managed Vite requires laravel-vite-plugin in your Vite config.");
        }
        // Override project/plugin TLS and HMR settings only for this process.
        Object.assign(config.server, network);
      },
    },
  ],
});
await server.listen();
const value = fs.existsSync(hotFile) ? fs.readFileSync(hotFile, "utf8") : "";
if (!value.startsWith(origin) || (value !== origin && !value.startsWith(`${origin}/`))) {
  await server.close();
  throw new Error(
    "Managed Vite requires Laravel's standard public/hot file. Use your dev script for a custom hotFile.",
  );
}
fs.writeFileSync(readyFile, JSON.stringify({ value, pid: process.pid }));
console.log(`Vite ready: ${origin}`);
