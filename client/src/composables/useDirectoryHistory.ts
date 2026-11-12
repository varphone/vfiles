import { computed, getCurrentScope, onScopeDispose, ref, watch, type Ref } from "vue";
import { filesService } from "../services/files.service";

export interface DirectoryHistoryCommit {
  hash: string;
  date?: string;
}

export interface DirectoryHistoryPage {
  commits: Array<{ hash: string; date?: string }>;
  currentVersion?: string;
}

export interface DirectoryHistoryDeps {
  /** 当前浏览的目录（与文件列表一致）。 */
  currentPath: Ref<string>;
  /** 当前浏览的版本；`undefined` 表示 HEAD。 */
  browseCommit: Ref<string | undefined>;
  /** 切换浏览版本（写回 files store）。 */
  setBrowseCommit: (commit?: string) => void;
  /** 测试注入点；默认走 `filesService.getFileHistory`。 */
  fetchHistory?: (
    path: string,
    limit: number,
    opts: { signal: AbortSignal },
  ) => Promise<DirectoryHistoryPage>;
  /** 顶栏只需要最近若干条。 */
  limit?: number;
}

/**
 * 顶栏「版本切换」的目录历史：加载、步进与状态。
 *
 * 从 Home.vue 中抽出，便于单测。这里有两类必须守住的约束：
 *
 * - 目录切换或重复请求会取消上一笔请求，并且**任何一次被取代的请求都不能把
 *   `loading` 留在 true**——否则顶栏会一直显示「加载历史...」，所有按钮失去响应。
 * - 失败只影响本次目录历史：保留错误供顶栏提示，下一次点击立即重新请求，
 *   不缓存空结果。
 */
export function useDirectoryHistory(deps: DirectoryHistoryDeps) {
  const loading = ref(false);
  const error = ref<string | null>(null);
  const commits = ref<DirectoryHistoryCommit[]>([]);
  /** 当前文件/目录版本的 hash（HEAD）；用于从当前版本正确后退一格。 */
  const currentVersion = ref("");
  /** 已成功加载 `commits` 的目录；空串表示尚未加载。 */
  const loadedPath = ref("");
  /** 当前目录是否已经成功拉取过一次（用于区分「没有历史」和「还没加载」）。 */
  const loaded = ref(false);
  const selectedHash = ref(deps.browseCommit.value || "");

  let requestSeq = 0;
  let controller: AbortController | null = null;
  const limit = deps.limit ?? 50;

  const fetchHistory =
    deps.fetchHistory ??
    ((path, pageSize, opts) =>
      filesService.getFileHistory(path, pageSize, undefined, {
        signal: opts.signal,
        withTotal: false,
        timeoutMs: 15_000,
      }));

  const selectedIndex = computed(() => {
    const hash = selectedHash.value;
    if (!hash) return -1;
    return commits.value.findIndex((commit) => commit.hash === hash);
  });

  const canPrev = computed(
    () => selectedIndex.value >= 0 && selectedIndex.value < commits.value.length - 1,
  );
  const canNext = computed(() => selectedIndex.value > 0);
  const canLast = computed(() => selectedIndex.value > 0);

  function abortInFlight() {
    controller?.abort();
    controller = null;
  }

  function isAbortError(err: unknown): boolean {
    return err instanceof DOMException
      ? err.name === "AbortError"
      : err instanceof Error && (err.name === "AbortError" || err.name === "CanceledError");
  }

  /**
   * 丢掉上一个目录的状态。取消中的请求会带着旧的序号返回，因序号不匹配而
   * 不再改写任何状态；`loading` 在这里显式复位，避免残留的「加载历史...」。
   */
  function resetForPath() {
    requestSeq += 1;
    abortInFlight();
    loading.value = false;
    error.value = null;
    loadedPath.value = "";
    loaded.value = false;
    commits.value = [];
    currentVersion.value = "";
    selectedHash.value = deps.browseCommit.value || "";
  }

  async function load(options: { force?: boolean } = {}): Promise<void> {
    const path = deps.currentPath.value || "";
    if (!options.force && loadedPath.value === path && commits.value.length > 0) {
      return;
    }

    requestSeq += 1;
    const seq = requestSeq;
    abortInFlight();
    const active = new AbortController();
    controller = active;
    loading.value = true;
    error.value = null;

    try {
      const page = await fetchHistory(path, limit, { signal: active.signal });
      if (seq !== requestSeq) return;
      const list = (page?.commits ?? []).map((commit) => ({
        hash: commit.hash,
        date: commit.date,
      }));
      commits.value = list;
      currentVersion.value = page?.currentVersion ?? "";
      loadedPath.value = path;
      loaded.value = true;

      const preferred =
        deps.browseCommit.value ||
        selectedHash.value ||
        page?.currentVersion ||
        list[0]?.hash ||
        "";
      const exists = preferred !== "" && list.some((commit) => commit.hash === preferred);
      selectedHash.value = exists
        ? preferred
        : page?.currentVersion || list[0]?.hash || "";
      // 仍然处于 HEAD 时只记录/高亮当前版本，不要把浏览版本切到一个提交；
      // 否则第一次点「后退」会先落到当前快照，看起来像点了一次没反应。
      if (deps.browseCommit.value && selectedHash.value) {
        deps.setBrowseCommit(selectedHash.value);
      }
    } catch (err) {
      if (seq !== requestSeq || isAbortError(err)) return;
      loadedPath.value = "";
      loaded.value = false;
      error.value = err instanceof Error ? err.message : "加载历史失败";
      commits.value = [];
      selectedHash.value = "";
    } finally {
      // Only the newest request owns the flag: a superseded one must not clear
      // the spinner the newest one just turned on.
      if (seq === requestSeq) {
        loading.value = false;
        if (controller === active) controller = null;
      }
    }
  }

  async function ensureLoaded(): Promise<boolean> {
    if (loading.value) return false;
    const path = deps.currentPath.value || "";
    if (loadedPath.value !== path || commits.value.length === 0) {
      await load();
    }
    return commits.value.length > 0;
  }

  function select(hash: string | undefined) {
    if (!hash) return;
    selectedHash.value = hash;
    deps.setBrowseCommit(hash);
  }

  function exit() {
    selectedHash.value = "";
    deps.setBrowseCommit(undefined);
  }

  /** 跳到最后一次快照（最早）。 */
  async function fastBack(): Promise<void> {
    if (!(await ensureLoaded())) return;
    select(commits.value[commits.value.length - 1]?.hash);
  }

  /** 后退一步；从 HEAD 出发时直接落到当前版本之前的版本。 */
  async function stepBack(): Promise<void> {
    // ensureLoaded 只负责加载与高亮当前版本，不会改写 browseCommit；
    // 因此这里在加载后仍然知道起点是 HEAD。
    const fromHead = !deps.browseCommit.value;
    if (!(await ensureLoaded())) return;
    if (fromHead) {
      const newest = commits.value[0];
      // 最新提交通常就是当前版本：再后退一格才是“上一个版本”。
      // 若 HEAD 比最新快照更新（未生成快照），则最新快照本身就是上一个版本。
      if (currentVersion.value && newest?.hash === currentVersion.value) {
        select(commits.value[1]?.hash);
      } else {
        select(newest?.hash);
      }
      return;
    }
    const index = selectedIndex.value;
    if (index >= 0) select(commits.value[index + 1]?.hash);
  }

  function stepForward() {
    if (!deps.browseCommit.value) return;
    const index = selectedIndex.value;
    if (index > 0) {
      const next = commits.value[index - 1];
      // 回到当前版本时退出历史模式，让标签重新显示“当前版本”。
      if (next && currentVersion.value && next.hash === currentVersion.value) {
        exit();
      } else {
        select(next?.hash);
      }
      return;
    }
    exit();
  }

  function fastForward() {
    if (!deps.browseCommit.value) return;
    exit();
  }

  watch(
    () => deps.currentPath.value,
    () => {
      resetForPath();
      if (deps.browseCommit.value) void load();
    },
  );

  watch(
    () => deps.browseCommit.value,
    (nextCommit) => {
      selectedHash.value = nextCommit || "";
      if (nextCommit && loadedPath.value !== (deps.currentPath.value || "")) {
        void load();
      }
    },
  );

  if (getCurrentScope()) {
    onScopeDispose(() => {
      requestSeq += 1;
      abortInFlight();
    });
  }

  return {
    loading,
    error,
    commits,
    currentVersion,
    loadedPath,
    loaded,
    selectedHash,
    selectedIndex,
    canPrev,
    canNext,
    canLast,
    load,
    ensureLoaded,
    select,
    exit,
    fastBack,
    stepBack,
    stepForward,
    fastForward,
  };
}
