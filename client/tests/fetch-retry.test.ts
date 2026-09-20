import { describe, expect, it, vi } from "vitest";
import { fetchWithRetry, isRetryableStatus } from "../src/services/fetch-retry";
import { MAX_RETRIES } from "../src/services/api.service";

const noSleep = () => Promise.resolve();

function response(status: number, body = "x") {
  return new Response(body, { status });
}

describe("isRetryableStatus", () => {
  it("only accepts transient gateway and rate-limit statuses", () => {
    expect(isRetryableStatus(429)).toBe(true);
    expect(isRetryableStatus(502)).toBe(true);
    expect(isRetryableStatus(503)).toBe(true);
    expect(isRetryableStatus(504)).toBe(true);
    expect(isRetryableStatus(500)).toBe(false);
    expect(isRetryableStatus(404)).toBe(false);
    expect(isRetryableStatus(200)).toBe(false);
  });
});

describe("fetchWithRetry", () => {
  it("retries a transient status and returns the successful response", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(response(503))
      .mockResolvedValueOnce(response(200, "ok"));
    const sleep = vi.fn(noSleep);

    const result = await fetchWithRetry(
      "/api/files/content",
      undefined,
      { sleep, random: () => 0 },
      fetchMock,
    );

    expect(result.status).toBe(200);
    expect(await result.text()).toBe("ok");
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(sleep).toHaveBeenCalledTimes(1);
    expect(sleep).toHaveBeenCalledWith(300);
  });

  it("does not retry statuses that are not transient", async () => {
    const fetchMock = vi.fn().mockResolvedValue(response(404));
    const sleep = vi.fn(noSleep);

    const result = await fetchWithRetry(
      "/api/files/content",
      undefined,
      { sleep },
      fetchMock,
    );

    expect(result.status).toBe(404);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(sleep).not.toHaveBeenCalled();
  });

  it("gives up after the retry budget and returns the last response", async () => {
    const fetchMock = vi.fn().mockImplementation(() => response(503));
    const sleep = vi.fn(noSleep);

    const result = await fetchWithRetry(
      "/api/files/content",
      undefined,
      { sleep },
      fetchMock,
    );

    expect(result.status).toBe(503);
    expect(fetchMock).toHaveBeenCalledTimes(MAX_RETRIES + 1);
    expect(sleep).toHaveBeenCalledTimes(MAX_RETRIES);
  });

  it("retries network errors and rethrows once the budget is spent", async () => {
    const networkError = new TypeError("Failed to fetch");
    const recovered = vi
      .fn()
      .mockRejectedValueOnce(networkError)
      .mockResolvedValueOnce(response(200, "ok"));

    await expect(
      fetchWithRetry("/x", undefined, { sleep: noSleep }, recovered),
    ).resolves.toHaveProperty("status", 200);
    expect(recovered).toHaveBeenCalledTimes(2);

    const failing = vi.fn().mockRejectedValue(networkError);
    await expect(
      fetchWithRetry("/x", undefined, { sleep: noSleep }, failing),
    ).rejects.toBe(networkError);
    expect(failing).toHaveBeenCalledTimes(MAX_RETRIES + 1);
  });

  it("releases the discarded response body before retrying", async () => {
    const discarded = response(503, "gateway");
    const cancel = vi.spyOn(discarded.body!, "cancel");
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(discarded)
      .mockResolvedValueOnce(response(200, "ok"));

    await fetchWithRetry(
      "/x",
      undefined,
      { sleep: noSleep },
      fetchMock as unknown as typeof fetch,
    );

    expect(cancel).toHaveBeenCalled();
  });

  it("does not retry once the caller aborted the request", async () => {
    const controller = new AbortController();
    const abortError = new DOMException("Aborted", "AbortError");
    const fetchMock = vi.fn().mockImplementation(() => {
      controller.abort();
      return Promise.reject(abortError);
    });

    await expect(
      fetchWithRetry(
        "/x",
        { signal: controller.signal },
        { sleep: noSleep },
        fetchMock,
      ),
    ).rejects.toBe(abortError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("fails fast when the signal is already aborted", async () => {
    const controller = new AbortController();
    controller.abort();
    const fetchMock = vi.fn();

    await expect(
      fetchWithRetry(
        "/x",
        { signal: controller.signal },
        { sleep: noSleep },
        fetchMock,
      ),
    ).rejects.toBeInstanceOf(DOMException);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
