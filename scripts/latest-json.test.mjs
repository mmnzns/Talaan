import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { buildLatest, missingPlatforms, platformsFor } from "./latest-json.mjs";

const base = { repo: "mmnzns/Talaan", tag: "v1.0.0", notes: "n", pubDate: "2026-10-20T00:00:00.000Z" };

describe("latest.json", () => {
  it("maps updater signatures to the updater's platform keys", () => {
    expect(platformsFor("Talaan_1.0.0_x64-setup.exe.sig")).toEqual(["windows-x86_64", "windows-x86_64-nsis"]);
    expect(platformsFor("Talaan_aarch64.app.tar.gz.sig")).toEqual(["darwin-aarch64", "darwin-aarch64-app"]);
    expect(platformsFor("Talaan_1.0.0_aarch64.dmg")).toEqual([]);
  });

  it("holds both platforms whichever was uploaded first", () => {
    const sigs = [
      { name: "Talaan_aarch64.app.tar.gz.sig", signature: "MAC\n" },
      { name: "Talaan_1.0.0_x64-setup.exe.sig", signature: "WIN" },
    ];
    const a = buildLatest({ ...base, sigs });
    const b = buildLatest({ ...base, sigs: [...sigs].reverse() });
    expect(a).toEqual(b);
    expect(a.version).toBe("1.0.0");
    expect(a.platforms["windows-x86_64"]).toEqual({
      signature: "WIN",
      url: "https://github.com/mmnzns/Talaan/releases/download/v1.0.0/Talaan_1.0.0_x64-setup.exe",
    });
    expect(a.platforms["darwin-aarch64"].signature).toBe("MAC");
  });

  it("finds signatures the uploaded file lacks, so a racing upload gets rebuilt", () => {
    const win = { name: "Talaan_1.0.0_x64-setup.exe.sig", signature: "WIN" };
    const mac = { name: "Talaan_aarch64.app.tar.gz.sig", signature: "MAC\n" };
    const winOnly = buildLatest({ ...base, sigs: [win] });
    expect(missingPlatforms(winOnly, [win])).toEqual([]);
    expect(missingPlatforms(winOnly, [win, mac])).toEqual(["darwin-aarch64", "darwin-aarch64-app"]);
    expect(missingPlatforms(buildLatest({ ...base, sigs: [win, mac] }), [win, mac])).toEqual([]);
    // A rebuilt artifact re-signed after the file was written counts as missing too.
    expect(missingPlatforms(winOnly, [{ ...win, signature: "WIN2" }])).toEqual(["windows-x86_64", "windows-x86_64-nsis"]);
    expect(missingPlatforms(undefined, [win])).toHaveLength(2);
  });

  it("release notes link to the tag's docs, not main (release.yml and upload-mac-release.sh fill {{TAG}})", () => {
    const notes = readFileSync(new URL("./release-notes.md", import.meta.url), "utf8");
    expect(notes).not.toContain("/blob/main/");
    expect(notes).toContain("/blob/{{TAG}}/docs/INSTALL.md");
    expect(notes).toContain("/blob/{{TAG}}/CHANGELOG.md");
  });

  it("refuses to write an empty file", () => {
    expect(() => buildLatest({ ...base, sigs: [] })).toThrow(/no updater signatures/);
  });
});
