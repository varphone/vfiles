import {
  MAX_RETRIES,
  RETRYABLE_STATUS_CODES,
  computeRetryDelayMs,
} from "./api.service";

/**
 * 原始 `fetch` 的有界重试。
 *
 * axios 实例已经带上了重试拦截器（见 `api.service.ts`），但预览内容、diff、
 * 带进度的下载走的是原始 `fetch`，此前一次失败就直接报错。这里复用同一套
 * 重试策略：
 *
 * - 只针对幂等的读取请求使用；
 * - 网络错误与 `RETRYABLE_STATUS_CODES`（429/502/503/504）才重试；
 * - 调用方通过 `signal` 主动取消时不重试；
 * - 最多重试 `MAX_RETRIES` 次，退避时间与 axios 拦截器一致（指数 + 抖动）。
 *
 * 只重试“拿到响应头之前”的失败：响应体已经开始读取后再重试会造成进度回退，
 * 因此流式读取失败由调用方按普通错误处理。
 */

export type SleepFn = (ms: number) => Promise<void>;

export interface FetchWithRetryOptions {
  maxRetries?: number;
  sleep?: SleepFn;
  random?: () => number;
}

const defaultSleep: SleepFn = (ms) =>
  new Promise((resolve) => setTimeout(resolve, ms));

export function isRetryableStatus(status: number): boolean {
  return RETRYABLE_STATUS_CODES.includes(
    status as (typeof RETRYABLE_STATUS_CODES)[number],
  );
}

function isAborted(signal: AbortSignal | null | undefined): boolean {
  return Boolean(signal?.aborted);
}

/**
 * 发起 GET 并在可重试的失败上重试。
 *
 * 返回最后一次的 `Response`（即使状态码仍然可重试，交由调用方按其错误格式
 * 处理）；网络错误在重试耗尽后抛出。
 */
export async function fetchWithRetry(
  input: RequestInfo | URL,
  init?: RequestInit,
  options?: FetchWithRetryOptions,
  fetchImpl: typeof fetch = fetch,
): Promise<Response> {
  const maxRetries = options?.maxRetries ?? MAX_RETRIES;
  const sleep = options?.sleep ?? defaultSleep;
  const signal = init?.signal ?? null;

  let lastError: unknown;

  for (let attempt = 0; attempt <= maxRetries; attempt += 1) {
    if (isAborted(signal)) {
      throw lastError ?? new DOMException("Aborted", "AbortError");
    }

    try {
      const response = await fetchImpl(input, init);
      if (!isRetryableStatus(response.status) || attempt === maxRetries) {
        return response;
      }
      // 该响应不会被使用，释放连接
      await response.body?.cancel().catch(() => undefined);
    } catch (error) {
      if (isAborted(signal)) throw error;
      lastError = error;
      if (attempt === maxRetries) throw error;
    }

    await sleep(computeRetryDelayMs(attempt + 1, options?.random));
  }

  // 循环要么返回、要么抛出，这里只是让类型完整
  throw lastError ?? new Error("请求失败");
}
