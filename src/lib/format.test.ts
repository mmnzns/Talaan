import { describe, expect, it } from "vitest";
import { dayGroup, formatBytes, formatDuration, formatTime, tildePath } from "./format";

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
