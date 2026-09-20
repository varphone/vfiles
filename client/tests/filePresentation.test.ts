import { describe, expect, it } from "vitest";
import { formatRelativeDate } from "../src/utils/filePresentation";

const NOW = new Date(2026, 8, 20, 21, 3, 0); // 2026-09-20 21:03 本地时间

function iso(year: number, month: number, day: number, hour = 9, minute = 0) {
  return new Date(year, month - 1, day, hour, minute).toISOString();
}

describe("formatRelativeDate", () => {
  it("shows the time for today", () => {
    expect(formatRelativeDate(iso(2026, 9, 20, 8, 5), NOW)).toBe("今天 08:05");
  });

  it("labels yesterday explicitly", () => {
    expect(formatRelativeDate(iso(2026, 9, 19, 22, 40), NOW)).toBe(
      "昨天 22:40",
    );
  });

  it("uses a day count within the last week", () => {
    expect(formatRelativeDate(iso(2026, 9, 17), NOW)).toBe("3 天前");
    expect(formatRelativeDate(iso(2026, 9, 14), NOW)).toBe("6 天前");
  });

  it("falls back to an absolute date for older entries", () => {
    const label = formatRelativeDate(iso(2026, 9, 1), NOW);
    expect(label).not.toContain("天前");
    expect(label).toContain("2026");
  });

  it("keeps missing or invalid values readable", () => {
    expect(formatRelativeDate(undefined, NOW)).toBe("--");
    expect(formatRelativeDate("not-a-date", NOW)).toBe("not-a-date");
  });
});
