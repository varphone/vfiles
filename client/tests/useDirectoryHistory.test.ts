import { describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";
import {
  useDirectoryHistory,
  type DirectoryHistoryPage,
} from "../src/composables/useDirectoryHistory";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function setup(options?: {
  path?: string;
  commit?: string;
  fetchHistory?: (path: string, limit: number, opts: { signal: AbortSignal }) => Promise<DirectoryHistoryPage>;
}) {
  const currentPath = ref(options?.path ?? "docs");
  const browseCommit = ref<string | undefined>(options?.commit);
  const setBrowseCommit = vi.fn((commit?: string) => {
    browseCommit.value = commit;
  });
  const fetchHistory = vi.fn(
    options?.fetchHistory ??
      (async (_path: string, _limit: number, _opts: { signal: AbortSignal }) => ({
        commits: [
          { hash: "newest", date: "2026-01-03T00:00:00Z" },
          { hash: "middle", date: "2026-01-02T00:00:00Z" },
          { hash: "oldest", date: "2026-01-01T00:00:00Z" },
        ],
        currentVersion: "newest",
      })),
  );

  const scope = effectScope();
  const history = scope.run(() =>
    useDirectoryHistory({
      currentPath,
      browseCommit,
      setBrowseCommit,
      fetchHistory,
    }),
  )!;

  return {
    history,
    currentPath,
    browseCommit,
    setBrowseCommit,
    fetchHistory,
    stop: () => scope.stop(),
  };
}

const flush = async () => {
  await Promise.resolve();
  await nextTick();
  await Promise.resolve();
};

describe("useDirectoryHistory", () => {
  it("loads the newest page once and steps back through it", async () => {
    const { history, setBrowseCommit, fetchHistory, browseCommit } = setup();

    await history.stepBack();

    expect(fetchHistory).toHaveBeenCalledTimes(1);
    expect(history.commits.value.map((commit) => commit.hash)).toEqual([
      "newest",
      "middle",
      "oldest",
    ]);
    expect(setBrowseCommit).toHaveBeenCalledWith("newest");
    expect(browseCommit.value).toBe("newest");
    expect(history.loading.value).toBe(false);

    await history.stepBack();
    expect(setBrowseCommit).toHaveBeenLastCalledWith("middle");

    // 已加载的列表不再重新请求。
    expect(fetchHistory).toHaveBeenCalledTimes(1);
  });

  it("jumps to the oldest snapshot on fast back and exits on fast forward", async () => {
    const { history, setBrowseCommit } = setup();

    await history.fastBack();
    expect(setBrowseCommit).toHaveBeenLastCalledWith("oldest");

    history.fastForward();
    expect(setBrowseCommit).toHaveBeenLastCalledWith(undefined);
  });

  it("clears the loading state when the directory changes mid-request", async () => {
    const pending = deferred<DirectoryHistoryPage>();
    const { history, currentPath, setBrowseCommit } = setup({
      fetchHistory: async () => pending.promise,
    });

    const step = history.stepBack();
    expect(history.loading.value).toBe(true);

    currentPath.value = "docs/sub";
    await nextTick();
    // 目录已经切换：旧请求作废，不能把「加载历史...」留在顶栏。
    expect(history.loading.value).toBe(false);
    expect(history.commits.value).toEqual([]);

    pending.resolve({
      commits: [{ hash: "stale", date: "2026-01-01T00:00:00Z" }],
      currentVersion: "stale",
    });
    await step;
    await flush();

    expect(history.loading.value).toBe(false);
    expect(history.commits.value).toEqual([]);
    expect(setBrowseCommit).not.toHaveBeenCalledWith("stale");
  });

  it("surfaces a failure without sticking and retries on the next click", async () => {
    const failure = new Error("Network Error");
    const page: DirectoryHistoryPage = {
      commits: [{ hash: "newest", date: "2026-01-03T00:00:00Z" }],
      currentVersion: "newest",
    };
    const fetchHistory = vi
      .fn<(path: string, limit: number, opts: { signal: AbortSignal }) => Promise<DirectoryHistoryPage>>()
      .mockRejectedValueOnce(failure)
      .mockResolvedValue(page);
    const { history, setBrowseCommit } = setup({ fetchHistory });

    await history.stepBack();
    expect(history.error.value).toBe("Network Error");
    expect(history.loading.value).toBe(false);
    expect(setBrowseCommit).not.toHaveBeenCalled();

    // 失败不缓存：再点一次立刻重新请求，不需要刷新页面。
    await history.stepBack();
    expect(fetchHistory).toHaveBeenCalledTimes(2);
    expect(history.error.value).toBeNull();
    expect(setBrowseCommit).toHaveBeenCalledWith("newest");
  });

  it("ignores an aborted request instead of reporting an error", async () => {
    const { history } = setup({
      fetchHistory: async (_path, _limit, opts) =>
        new Promise((_resolve, reject) => {
          opts.signal.addEventListener(
            "abort",
            () => reject(new DOMException("Aborted", "AbortError")),
            { once: true },
          );
        }),
    });

    const step = history.stepBack();
    await flush();
    expect(history.loading.value).toBe(true);

    // 同一目录的强制刷新会取消上一次请求。
    void history.load({ force: true });
    await step;
    await flush();

    expect(history.error.value).toBeNull();
  });

  it("loads history for a path entered while browsing a commit", async () => {
    const { history, currentPath, fetchHistory, browseCommit } = setup({
      path: "docs",
      commit: "middle",
    });

    currentPath.value = "docs/sub";
    await nextTick();
    await flush();

    expect(fetchHistory).toHaveBeenCalledTimes(1);
    expect(fetchHistory.mock.calls[0]?.[0]).toBe("docs/sub");
    // 仍然停留在浏览的版本上，等待新目录的历史加载完成。
    expect(browseCommit.value).toBe("middle");
    expect(history.selectedHash.value).toBe("middle");
  });
});
