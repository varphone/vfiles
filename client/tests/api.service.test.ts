import { describe, expect, it } from "vitest";
import {
  MAX_RETRIES,
  RETRYABLE_STATUS_CODES,
  computeRetryDelayMs,
  isRetryableError,
} from "../src/services/api.service";

describe("isRetryableError", () => {
  it("retries idempotent GETs on network errors and timeouts", () => {
    expect(isRetryableError({ config: { method: "get" } })).toBe(true);
    expect(
      isRetryableError({ code: "ECONNABORTED", config: { method: "get" } }),
    ).toBe(true);
  });

  it("retries GETs for transient server errors", () => {
    for (const status of RETRYABLE_STATUS_CODES) {
      expect(
        isRetryableError({
          config: { method: "get" },
          response: { status },
        }),
      ).toBe(true);
    }

    expect(
      isRetryableError({
        config: { method: "get" },
        response: { status: 500 },
      }),
    ).toBe(false);
    expect(
      isRetryableError({
        config: { method: "get" },
        response: { status: 404 },
      }),
    ).toBe(false);
  });

  it("never retries non-idempotent or cancelled requests", () => {
    expect(
      isRetryableError({
        config: { method: "post" },
        response: { status: 503 },
      }),
    ).toBe(false);
    expect(
      isRetryableError({
        code: "ERR_CANCELED",
        config: { method: "get", signal: { aborted: true } },
      }),
    ).toBe(false);
    expect(
      isRetryableError({
        config: { method: "get", signal: { aborted: true } },
        response: { status: 503 },
      }),
    ).toBe(false);
    expect(isRetryableError({})).toBe(false);
  });
});

describe("computeRetryDelayMs", () => {
  it("backs off exponentially and caps at 2s", () => {
    const noJitter = () => 0;
    expect(computeRetryDelayMs(1, noJitter)).toBe(300);
    expect(computeRetryDelayMs(2, noJitter)).toBe(600);
    expect(computeRetryDelayMs(3, noJitter)).toBe(1200);
    expect(computeRetryDelayMs(10, noJitter)).toBe(2000);
  });

  it("adds bounded jitter", () => {
    expect(computeRetryDelayMs(1, () => 0.5)).toBe(350);
    expect(computeRetryDelayMs(1, () => 0.999)).toBe(399);
  });

  it("uses a small retry budget", () => {
    expect(MAX_RETRIES).toBeGreaterThan(0);
    expect(MAX_RETRIES).toBeLessThanOrEqual(3);
  });
});
