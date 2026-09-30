import { describe, expect, it, vi } from "vitest";
import { createErrorBoundary } from "../src/utils/errorBoundary";

describe("createErrorBoundary", () => {
  it("notifies synchronously and throttles repeated notifications", () => {
    let timestamp = 10_000;
    const notify = vi.fn();
    const report = vi.fn();
    const boundary = createErrorBoundary({
      log: vi.fn(),
      incrementErrorCount: vi.fn(),
      notify,
      report,
      now: () => timestamp,
    });

    boundary("vue", new Error("first"));
    expect(notify).toHaveBeenCalledTimes(1);

    timestamp += 1000;
    boundary("promise", new Error("second"));
    expect(notify).toHaveBeenCalledTimes(1);

    timestamp += 3001;
    boundary("window", new Error("third"));
    expect(notify).toHaveBeenCalledTimes(2);
    expect(report).toHaveBeenCalledTimes(3);
  });

  it("contains notification and reporting failures", () => {
    const boundary = createErrorBoundary({
      log: vi.fn(),
      incrementErrorCount: vi.fn(),
      notify: () => {
        throw new Error("notification failed");
      },
      report: () => {
        throw new Error("report failed");
      },
      now: () => 10_000,
    });

    expect(() => boundary("vue", new Error("render failed"))).not.toThrow();
  });
});
