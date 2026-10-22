// The content security policy in src-tauri/tauri.conf.json. The frontend makes no outside web requests (everything
// goes through Rust over IPC), so the policy is strict; these tests keep it that way and keep what the app needs.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const conf = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
const security = conf.app.security;

/** "a 'self'; b x y" -> { a: ["'self'"], b: ["x", "y"] } */
export function parseCsp(csp) {
  const out = {};
  for (const part of csp.split(";")) {
    const [name, ...sources] = part.trim().split(/\s+/);
    if (name) out[name.toLowerCase()] = sources;
  }
  return out;
}

const prod = parseCsp(security.csp ?? "");
const dev = parseCsp(security.devCsp ?? "");

describe.each([
  ["csp", prod],
  ["devCsp", dev],
])("%s", (_name, p) => {
  it("is set", () => {
    expect(Object.keys(p).length).toBeGreaterThan(0);
  });

  it("starts from default-src 'self'", () => {
    expect(p["default-src"]).toEqual(["'self'"]);
  });

  it("never allows eval", () => {
    for (const [directive, sources] of Object.entries(p)) {
      expect(sources, directive).not.toContain("'unsafe-eval'");
      expect(sources, directive).not.toContain("'wasm-unsafe-eval'");
    }
  });

  it("has no wildcard or scheme-only web source anywhere", () => {
    for (const [directive, sources] of Object.entries(p)) {
      for (const s of sources) {
        expect(s, directive).not.toBe("*");
        expect(s, directive).not.toMatch(/^\*\./);
        expect(s, directive).not.toMatch(/^(https?|wss?|ws):$/);
        expect(s, directive).not.toMatch(/^https?:\/\/\*/);
      }
    }
  });

  it("only lets the frontend talk to Tauri's IPC (plus the dev server in dev)", () => {
    const connect = p["connect-src"];
    expect(connect).toContain("ipc:");
    expect(connect).toContain("http://ipc.localhost");
    for (const s of connect) expect(s, "connect-src").toMatch(/^(ipc:|http:\/\/ipc\.localhost|(ws|http):\/\/localhost:1420)$/);
  });

  it("plays audio from the meetings folder (asset protocol) and from blobs", () => {
    const media = p["media-src"];
    for (const s of ["'self'", "asset:", "http://asset.localhost", "blob:"]) expect(media).toContain(s);
  });

  it("allows the bundled fonts, images and the inline styles React and Tailwind need", () => {
    expect(p["font-src"]).toEqual(expect.arrayContaining(["'self'", "data:"]));
    expect(p["img-src"]).toEqual(expect.arrayContaining(["'self'", "data:"]));
    expect(p["style-src"]).toEqual(expect.arrayContaining(["'self'", "'unsafe-inline'"]));
  });

  it("blocks plugins, frames and a changed base URL", () => {
    expect(p["object-src"]).toEqual(["'none'"]);
    expect(p["frame-src"]).toEqual(["'none'"]);
    expect(p["base-uri"]).toEqual(["'self'"]);
  });
});

describe("csp (production)", () => {
  it("lets only the app's own scripts run", () => {
    expect(prod["script-src"]).toEqual(["'self'"]);
  });
});

describe("devCsp", () => {
  it("lets Vite's dev server work: inline preamble script and its websocket", () => {
    expect(dev["script-src"]).toContain("'unsafe-inline'");
    expect(dev["connect-src"]).toContain("ws://localhost:1420");
    expect(new URL(conf.build.devUrl).port).toBe("1420");
  });
});

describe("platform configs", () => {
  it("do not replace the policy", () => {
    for (const file of ["tauri.windows.conf.json", "tauri.macos.conf.json"]) {
      const c = JSON.parse(readFileSync(new URL(`../src-tauri/${file}`, import.meta.url), "utf8"));
      expect(c.app?.security?.csp, file).toBeUndefined();
      expect(c.app?.security?.devCsp, file).toBeUndefined();
    }
  });
});

describe("asset protocol", () => {
  it("is on, and limited to the meetings folder", () => {
    expect(security.assetProtocol.enable).toBe(true);
    expect(security.assetProtocol.scope).toEqual(["$DATA/Talaan/meetings/**"]);
  });
});
