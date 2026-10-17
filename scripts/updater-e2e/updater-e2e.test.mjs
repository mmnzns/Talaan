import { describe, expect, it } from "vitest";
import { overlay } from "./config.mjs";
import { localize } from "./feed.mjs";

describe("updater test overlay", () => {
  const base = { version: "0.0.1", pubkey: "PUB\n", endpoint: "http://127.0.0.1:8765/o/r/releases/latest/download/latest.json" };

  it("sets the version, the throwaway key, the local feed and the test feature", () => {
    const c = overlay(base);
    expect(c.version).toBe("0.0.1");
    expect(c.build.features).toEqual(["updater-e2e"]);
    expect(c.bundle).toEqual({ createUpdaterArtifacts: true });
    expect(c.plugins.updater).toEqual({ pubkey: "PUB", endpoints: [base.endpoint], dangerousInsecureTransportProtocol: true });
    expect(overlay({ ...base, mac: true }).bundle.macOS).toEqual({ signingIdentity: "-" });
  });

  it("only points at 127.0.0.1, with a plain version", () => {
    expect(() => overlay({ ...base, endpoint: "https://github.com/o/r/releases/latest/download/latest.json" })).toThrow();
    expect(() => overlay({ ...base, version: "1.0.0-test" })).toThrow();
  });
});

describe("updater test feed", () => {
  it("swaps only the origin of each url", () => {
    const latest = {
      version: "0.0.2",
      platforms: { "windows-x86_64": { signature: "S", url: "https://github.com/o/r/releases/download/v0.0.2/Talaan_0.0.2_x64-setup.exe" } },
    };
    expect(localize(latest, 8765).platforms["windows-x86_64"]).toEqual({
      signature: "S",
      url: "http://127.0.0.1:8765/o/r/releases/download/v0.0.2/Talaan_0.0.2_x64-setup.exe",
    });
    expect(() => localize({ platforms: { x: { url: "https://example.com/a" } } }, 1)).toThrow();
  });
});
