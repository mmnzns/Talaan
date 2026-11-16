import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import { dayGroup, dayMark, formatBytes, formatDuration, formatTime, tildePath } from "./format";

describe("formatTime", () => {
  it("formats minutes and seconds", () => {
    expect(formatTime(0)).toBe("00:00");
    expect(formatTime(754_000)).toBe("12:34");
  });
  it("adds hours when needed", () => {
    expect(formatTime(3_725_000)).toBe("1:02:05");
  });
});

describe("formatDuration", () => {
  it("uses seconds under a minute", () => {
    expect(formatDuration(3_683)).toBe("4 s");
    expect(formatDuration(200)).toBe("1 s");
  });
  it("uses minutes under an hour", () => {
    expect(formatDuration(2_520_000)).toBe("42 min");
  });
  it("uses hours and minutes beyond that", () => {
    expect(formatDuration(7_500_000)).toBe("2 h 5 min");
    expect(formatDuration(7_200_000)).toBe("2 h");
  });
});

describe("dayGroup", () => {
  const now = new Date("2026-10-14T20:00:00");
  it("labels today, yesterday and older days", () => {
    expect(dayGroup("2026-10-14T09:00:00", now).label).toBe("today");
    expect(dayGroup("2026-10-13T09:00:00", now).label).toBe("yesterday");
    expect(dayGroup("2026-10-10T13:00:00", now).label).toBe("oct 10");
  });
});

describe("formatBytes", () => {
  it("picks a readable unit", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(45_000)).toBe("45 KB");
    expect(formatBytes(1_200_000_000)).toBe("1.2 GB");
    expect(formatBytes(250_000_000)).toBe("250 MB");
  });
});

describe("tildePath", () => {
  it("shortens the home folder", () => {
    expect(tildePath("/Users/miguel.monzones/Library/Application Support/Talaan")).toBe("~/Library/Application Support/Talaan");
    expect(tildePath("/opt/data")).toBe("/opt/data");
  });
});

describe("dayMark", () => {
  afterEach(() => setLang("en"));
  it("gives the short month, weekday and full date in each language", () => {
    const iso = "2026-10-28T10:00:00";
    expect(dayMark(iso)).toEqual({ day: "28", month: "oct", weekday: "wed", full: "Wednesday, October 28, 2026" });
    setLang("fr");
    expect(dayMark(iso).month).toBe("oct");
    expect(dayMark(iso).full).toContain("octobre");
    expect(dayMark("2026-12-02T10:00:00").month).toBe("déc");
    expect(dayMark("2026-11-02T10:00:00").month).toBe("nov");
    setLang("es");
    expect(dayMark(iso).month).toBe("oct");
    expect(dayMark("2026-12-02T10:00:00").month).toBe("dic");
    expect(dayMark("2026-11-02T10:00:00").month).toBe("nov");
  });
  it("never keeps a trailing period", () => {
    for (const lang of ["en", "fr", "es"] as const) {
      setLang(lang);
      for (let m = 0; m < 12; m++) {
        const { month, weekday } = dayMark(new Date(2026, m, 5).toISOString());
        expect(month).toMatch(/^[^.]{3,4}$/);
        expect(weekday).not.toContain(".");
      }
    }
  });
});
