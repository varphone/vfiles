import type { AxiosInstance } from "axios";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  MAX_AUTOMATIC_RETRY_AFTER_MS,
  MAX_RETRIES,
  RETRYABLE_STATUS_CODES,
  computeRetryDelayMs,
  computeRetryDelayForResponse,
  isRetryableError,
  parseRetryAfterMs,
  apiService,
} from "../src/services/api.service";

const axiosInstance = Reflect.get(apiService, "api") as AxiosInstance;

afterEach(() => {
  vi.restoreAllMocks();
});

describe("ApiService.get", () => {
  it("forwards an abort signal to Axios", async () => {
    const getSpy = vi
      .spyOn(axiosInstance, "get")
      .mockResolvedValue({ data: { ok: true } } as never);
    const controller = new AbortController();

    await apiService.get("/files/list", undefined, {
      signal: controller.signal,
    });

    expect(getSpy).toHaveBeenCalledWith("/files/list", {
      params: undefined,
      signal: controller.signal,
    });
  });
});

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

describe("Retry-After handling", () => {
  const fixedNow = Date.parse("Wed, 21 Oct 2015 07:28:00 GMT");

  it("parses delta-seconds and HTTP-date values", () => {
    expect(parseRetryAfterMs(" 4 ", fixedNow)).toBe(4000);
    expect(parseRetryAfterMs("Wed, 21 Oct 2015 07:28:10 GMT", fixedNow)).toBe(
      10_000,
    );
    expect(parseRetryAfterMs("invalid", fixedNow)).toBeUndefined();
  });

  it("waits at least as long as the server requests", () => {
    expect(computeRetryDelayForResponse(1, "2", () => 0, fixedNow)).toBe(2000);
    expect(computeRetryDelayForResponse(1, "0", () => 0, fixedNow)).toBe(300);
  });

  it("skips automatic retries when the server requests an excessive delay", () => {
    expect(MAX_AUTOMATIC_RETRY_AFTER_MS).toBe(30_000);
    expect(computeRetryDelayForResponse(1, "31", () => 0, fixedNow)).toBeNull();
    expect(
      computeRetryDelayForResponse(
        1,
        "Wed, 21 Oct 2015 07:29:00 GMT",
        () => 0,
        fixedNow,
      ),
    ).toBeNull();
  });
});
