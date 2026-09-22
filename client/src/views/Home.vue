<template>
  <div class="home">
    <nav
      class="navbar is-fixed-top app-top-bar"
      role="navigation"
      aria-label="主页导航"
    >
      <div class="container is-fluid">
        <div class="navbar-brand">
          <router-link class="navbar-item" to="/">
            <figure class="image is-32x32 mr-2">
              <img src="/vfiles-icon.svg" alt="VFiles" />
            </figure>
            <span class="has-text-weight-semibold">VFiles</span>
          </router-link>

          <a
            role="button"
            class="navbar-burger"
            :class="{ 'is-active': mobileMenuOpen }"
            aria-label="menu"
            :aria-expanded="mobileMenuOpen ? 'true' : 'false'"
            @click="mobileMenuOpen = !mobileMenuOpen"
          >
            <span aria-hidden="true"></span>
            <span aria-hidden="true"></span>
            <span aria-hidden="true"></span>
          </a>
        </div>

        <div class="navbar-menu" :class="{ 'is-active': mobileMenuOpen }">
          <div class="navbar-start">
            <div class="navbar-item is-hidden-touch">
              <div
                class="app-bar-history"
                :class="{ 'is-error': dirHistoryError }"
                role="group"
                aria-label="版本切换"
              >
                <button
                  class="vf-icon-button app-bar-history-button"
                  :disabled="dirHistoryLoading"
                  title="快退到最早快照"
                  aria-label="快退到最早快照"
                  @click="desktopHistoryFastBack"
                >
                  <IconChevronsLeft :size="16" />
                </button>
                <button
                  class="vf-icon-button app-bar-history-button"
                  :disabled="dirHistoryLoading"
                  title="后退到更早快照"
                  aria-label="后退到更早快照"
                  @click="desktopHistoryStepBack"
                >
                  <IconChevronLeft :size="16" />
                </button>
                <span
                  class="app-bar-history-label"
                  :title="desktopHistoryStateLabel"
                >
                  {{ desktopHistoryStateLabel }}
                </span>
                <button
                  class="vf-icon-button app-bar-history-button"
                  :disabled="dirHistoryLoading || !browseCommit"
                  title="前进到较新快照"
                  aria-label="前进到较新快照"
                  @click="desktopHistoryStepForward"
                >
                  <IconChevronRight :size="16" />
                </button>
                <button
                  class="vf-icon-button app-bar-history-button"
                  :disabled="dirHistoryLoading || !browseCommit"
                  title="回到当前版本"
                  aria-label="回到当前版本"
                  @click="desktopHistoryFastForward"
                >
                  <IconChevronsRight :size="16" />
                </button>
              </div>
            </div>
          </div>

          <div class="navbar-end">
            <div class="navbar-item">
              <NotificationCenter />
            </div>

            <div class="navbar-item">
              <ThemeToggle />
            </div>

            <div class="navbar-item">
              <div
                ref="accountMenuRef"
                class="dropdown is-right"
                :class="{ 'is-active': accountMenuOpen }"
              >
                <div class="dropdown-trigger">
                  <button
                    class="app-bar-account"
                    aria-haspopup="true"
                    :aria-expanded="accountMenuOpen ? 'true' : 'false'"
                    :title="auth.user?.username || '账号'"
                    @click="accountMenuOpen = !accountMenuOpen"
                  >
                    <span class="app-bar-avatar" aria-hidden="true">
                      <IconUser v-if="!accountInitial" :size="14" />
                      <template v-else>{{ accountInitial }}</template>
                    </span>
                    <span class="app-bar-account-name">
                      {{ auth.user?.username || "账号" }}
                    </span>
                    <IconChevronDown :size="14" class="app-bar-account-caret" />
                  </button>
                </div>

                <div class="dropdown-menu" role="menu">
                  <div class="dropdown-content">
                    <div class="dropdown-item">
                      <p class="vf-text-muted is-size-7 mb-1">账号</p>
                      <p class="is-size-7 mb-0">{{ desktopProfileLabel }}</p>
                    </div>

                    <hr class="dropdown-divider" />

                    <router-link
                      v-if="auth.user && auth.features?.shareEnabled !== false"
                      class="dropdown-item"
                      to="/shares"
                      @click="closeAccountMenus"
                    >
                      我的分享
                    </router-link>

                    <router-link
                      v-if="auth.enabled && auth.user"
                      class="dropdown-item"
                      to="/settings/tokens"
                      @click="closeAccountMenus"
                    >
                      访问令牌
                    </router-link>

                    <router-link
                      v-if="
                        auth.enabled &&
                        ['admin', 'manager'].includes(auth.user?.role || '')
                      "
                      class="dropdown-item"
                      to="/admin/users"
                      @click="closeAccountMenus"
                    >
                      用户管理
                    </router-link>

                    <router-link
                      v-if="auth.enabled && auth.user?.role === 'admin'"
                      class="dropdown-item"
                      to="/admin/audit"
                      @click="closeAccountMenus"
                    >
                      审计日志
                    </router-link>

                    <router-link
                      v-if="auth.enabled && !auth.user"
                      class="dropdown-item"
                      to="/login"
                      @click="closeAccountMenus"
                    >
                      去登录
                    </router-link>

                    <a
                      v-else-if="auth.enabled && auth.user"
                      class="dropdown-item"
                      href="#"
                      @click.prevent="logoutFromMenu"
                    >
                      退出登录
                    </a>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </nav>

    <section class="section home-content">
      <div class="container is-fluid home-main-container">
        <div class="home-browser-shell">
          <FileBrowser
            :key="auth.activeWorkspace || 'default-workspace'"
            ref="browserRef"
          />
        </div>

        <footer
          class="site-record-footer has-text-centered"
          aria-label="备案信息"
        >
          <a
            class="site-record-link"
            href="https://beian.miit.gov.cn/"
            target="_blank"
            rel="noreferrer"
          >
            苏ICP备2026014694号-1
          </a>
        </footer>
      </div>
    </section>

    <div class="mobile-bottom-bar is-hidden-desktop">
      <div class="container is-fluid">
        <div class="mobile-bottom-bar-inner">
          <div class="mobile-bottom-bar-bottom">
            <div
              ref="actionMenuRef"
              class="dropdown is-up"
              :class="{ 'is-active': actionMenuOpen }"
            >
              <div class="dropdown-trigger">
                <button
                  class="button is-light is-small mobile-bar-button"
                  aria-haspopup="true"
                  :aria-expanded="actionMenuOpen ? 'true' : 'false'"
                  title="操作菜单"
                  aria-label="操作菜单"
                  @click="actionMenuOpen = !actionMenuOpen"
                >
                  <IconMenu2 :size="18" />
                  <span class="mobile-bar-label">更多</span>
                </button>
              </div>
              <div class="dropdown-menu" role="menu">
                <div class="dropdown-content">
                  <a
                    class="dropdown-item"
                    :class="{ 'is-current': actionMode === 'nav' }"
                    href="#"
                    @click.prevent="setActionMode('nav')"
                    >导航</a
                  >
                  <a
                    class="dropdown-item"
                    :class="{ 'is-current': actionMode === 'history' }"
                    href="#"
                    @click.prevent="setActionMode('history')"
                    >历史</a
                  >
                  <a
                    class="dropdown-item"
                    :class="{ 'is-current': actionMode === 'batch' }"
                    href="#"
                    @click.prevent="setActionMode('batch')"
                    >批量</a
                  >
                </div>
              </div>
            </div>

            <div class="mobile-action-panel">
              <!-- 导航模式 -->
              <div
                v-if="actionMode === 'nav'"
                class="buttons are-small mb-0 mobile-action-buttons"
              >
                <button
                  class="button is-light mobile-bar-button"
                  title="上一级"
                  aria-label="上一级"
                  @click="goBack"
                >
                  <IconArrowLeft :size="18" />
                  <span class="mobile-bar-label">上一级</span>
                </button>
                <button
                  class="button is-light mobile-bar-button"
                  title="根目录"
                  aria-label="根目录"
                  @click="goRoot"
                >
                  <IconHome :size="18" />
                  <span class="mobile-bar-label">首页</span>
                </button>
                <button
                  class="button is-light mobile-bar-button"
                  @click="appStore.requestCreateDirectory()"
                  title="新建文件夹"
                  aria-label="新建文件夹"
                >
                  <IconFolderPlus :size="18" />
                  <span class="mobile-bar-label">新建</span>
                </button>
                <button
                  class="button is-link mobile-bar-button"
                  @click="openUploader"
                  title="上传"
                  aria-label="上传"
                >
                  <IconUpload :size="18" />
                  <span class="mobile-bar-label">上传</span>
                </button>
                <button
                  class="button is-light mobile-bar-button"
                  @click="refresh"
                  title="刷新"
                  aria-label="刷新"
                >
                  <IconRefresh :size="18" />
                  <span class="mobile-bar-label">刷新</span>
                </button>
              </div>

              <!-- 历史模式 -->
              <div
                v-else-if="actionMode === 'history'"
                class="mobile-history-panel"
              >
                <div class="mobile-history-hash">
                  <code class="is-size-7">{{ historyHashShort }}</code>
                  <span
                    v-if="historyDateShort"
                    class="is-size-7 vf-text-subtle ml-2"
                    >{{ historyDateShort }}</span
                  >
                </div>
                <div class="buttons are-small mb-0 mobile-action-buttons">
                  <button
                    class="button is-light"
                    :disabled="dirHistoryLoading || !canPrev"
                    @click="historyPrev"
                    title="向前"
                    aria-label="向前"
                  >
                    <IconChevronLeft :size="18" />
                  </button>
                  <button
                    class="button is-light"
                    :disabled="dirHistoryLoading || !canNext"
                    @click="historyNext"
                    title="向后"
                    aria-label="向后"
                  >
                    <IconChevronRight :size="18" />
                  </button>
                  <button
                    class="button is-light"
                    :disabled="dirHistoryLoading || !canLast"
                    @click="historyLast"
                    title="跳到最后"
                    aria-label="跳到最后"
                  >
                    <IconChevronsRight :size="18" />
                  </button>
                </div>
              </div>

              <!-- 批量模式 -->
              <div v-else class="mobile-batch-panel">
                <div class="is-size-7 vf-text-muted mr-2 mobile-batch-count">
                  已选 {{ selectedCount }}
                </div>
                <div class="mobile-batch-actions">
                  <div class="buttons are-small mb-0 mobile-action-buttons">
                    <button
                      class="button is-light"
                      @click="toggleBatch"
                      title="进入/退出批量"
                    >
                      <IconChecklist :size="18" />
                    </button>
                    <button
                      class="button is-light"
                      :disabled="!isBatchMode"
                      @click="selectAll"
                      title="全选"
                    >
                      全选
                    </button>
                    <button
                      class="button is-light"
                      :disabled="!isBatchMode"
                      @click="clearSelection"
                      title="取消选择"
                    >
                      取消
                    </button>
                    <button
                      class="button is-info"
                      :disabled="selectedCount === 0"
                      @click="batchDownload"
                      title="批量下载"
                    >
                      下载
                    </button>
                  </div>

                  <div
                    ref="batchMenuRef"
                    class="dropdown is-up mobile-batch-more"
                    :class="{ 'is-active': batchMenuOpen }"
                  >
                    <div class="dropdown-trigger">
                      <button
                        class="button is-light is-small"
                        aria-haspopup="true"
                        :aria-expanded="batchMenuOpen ? 'true' : 'false'"
                        title="更多批量操作"
                        @click="batchMenuOpen = !batchMenuOpen"
                      >
                        <IconDotsVertical :size="18" />
                      </button>
                    </div>
                    <div class="dropdown-menu" role="menu">
                      <div class="dropdown-content">
                        <a
                          class="dropdown-item"
                          :class="{ 'is-disabled': selectedCount === 0 }"
                          href="#"
                          @click.prevent="runBatchMenuAction('delete')"
                        >
                          删除
                        </a>
                        <a
                          class="dropdown-item"
                          :class="{ 'is-disabled': selectedCount === 0 }"
                          href="#"
                          @click.prevent="runBatchMenuAction('move')"
                        >
                          移动
                        </a>
                        <a
                          class="dropdown-item"
                          :class="{ 'is-disabled': selectedCount !== 1 }"
                          href="#"
                          @click.prevent="runBatchMenuAction('rename')"
                        >
                          重命名
                        </a>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { useRouter } from "vue-router";
import {
  IconMenu2,
  IconChecklist,
  IconDotsVertical,
  IconArrowLeft,
  IconChevronsLeft,
  IconChevronLeft,
  IconChevronDown,
  IconChevronRight,
  IconChevronsRight,
  IconUser,
  IconHome,
  IconUpload,
  IconRefresh,
  IconFolderPlus,
} from "@tabler/icons-vue";
import FileBrowser from "../components/file-browser/FileBrowser.vue";
import NotificationCenter from "../components/common/NotificationCenter.vue";
import ThemeToggle from "../components/common/ThemeToggle.vue";
import { useFilesStore } from "../stores/files.store";
import { useAuthStore } from "../stores/auth.store";
import { filesService } from "../services/files.service";
import { useAppStore } from "../stores/app.store";

type FileBrowserHandle = {
  openUploader: () => void;
  refresh: () => void;
  goBack: () => void;
  goRoot: () => void;
  toggleBatchMode: () => void;
  setSearchQuery: (q: string) => void;
  runSearch: () => Promise<void>;
  clearSearch: () => void;
  batchMode: { value: boolean };
  selectedCount: { value: number };
  selectAllVisible: () => void;
  clearSelection: () => void;
  batchDownload: () => Promise<void>;
  batchDelete: () => Promise<void>;
  batchMove: () => Promise<void>;
  renameSelected: () => Promise<void>;
  searchLoading: { value: boolean };
};

const browserRef = ref<FileBrowserHandle | null>(null);
const mobileMenuOpen = ref(false);
const accountMenuOpen = ref(false);
const accountMenuRef = ref<HTMLElement | null>(null);

const auth = useAuthStore();
const appStore = useAppStore();
const router = useRouter();
async function doLogout() {
  try {
    await auth.logout();
  } finally {
    appStore.info("已退出登录");
    router.push({ name: "login", query: { redirect: "/" } });
  }
}

function closeAccountMenus() {
  accountMenuOpen.value = false;
  mobileMenuOpen.value = false;
}

async function logoutFromMenu() {
  closeAccountMenus();
  await doLogout();
}

const actionMenuOpen = ref(false);
const actionMenuRef = ref<HTMLElement | null>(null);
const actionMode = ref<"nav" | "history" | "batch">("nav");
const batchMenuOpen = ref(false);
const batchMenuRef = ref<HTMLElement | null>(null);
const dirHistoryLoading = ref(false);
const dirHistoryError = ref<string | null>(null);
const dirHistoryCommits = ref<Array<{ hash: string; date?: string }>>([]);
const dirHistoryPath = ref("");
const dirSelectedHash = ref("");
let dirHistoryReqId = 0;

const filesStore = useFilesStore();
const { currentPath, browseCommit } = storeToRefs(filesStore);

const desktopProfileLabel = computed(() => {
  if (!auth.enabled) return "本地模式";
  if (!auth.user) return "认证已启用";
  return `${auth.user.username} · ${auth.user.role}`;
});

function updateVisualViewportBottomOffset() {
  const vv = window.visualViewport;
  if (!vv) {
    document.documentElement.style.setProperty("--vv-bottom", "0px");
    return;
  }

  // iOS Safari 等浏览器的底部工具栏会压住 layout viewport 的 fixed 元素。
  // 用 visualViewport 的可视高度计算被遮挡的底部偏移，并通过 CSS 变量抬起底部操作栏。
  const bottom = Math.max(0, window.innerHeight - (vv.height + vv.offsetTop));
  document.documentElement.style.setProperty("--vv-bottom", `${bottom}px`);
}

onMounted(() => {
  updateVisualViewportBottomOffset();
  window.addEventListener("resize", updateVisualViewportBottomOffset);
  window.visualViewport?.addEventListener(
    "resize",
    updateVisualViewportBottomOffset,
  );
  window.visualViewport?.addEventListener(
    "scroll",
    updateVisualViewportBottomOffset,
  );

  const onDocPointer = (e: MouseEvent | TouchEvent) => {
    const target = e.target as Node | null;
    if (!target) return;

    if (actionMenuOpen.value) {
      const el = actionMenuRef.value;
      if (el && !el.contains(target)) actionMenuOpen.value = false;
    }
    if (batchMenuOpen.value) {
      const el = batchMenuRef.value;
      if (el && !el.contains(target)) batchMenuOpen.value = false;
    }
    if (accountMenuOpen.value) {
      const el = accountMenuRef.value;
      if (el && !el.contains(target)) accountMenuOpen.value = false;
    }
    // 移动端展开的顶栏菜单：点击菜单与汉堡按钮之外也收起
    if (mobileMenuOpen.value) {
      const insideMenu =
        target instanceof Element &&
        (target.closest(".navbar-menu") !== null ||
          target.closest(".navbar-burger") !== null);
      if (!insideMenu) mobileMenuOpen.value = false;
    }
  };

  const onDocKeydown = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    actionMenuOpen.value = false;
    batchMenuOpen.value = false;
    accountMenuOpen.value = false;
    mobileMenuOpen.value = false;
  };

  document.addEventListener("click", onDocPointer, true);
  document.addEventListener("touchstart", onDocPointer, true);
  document.addEventListener("keydown", onDocKeydown);

  onBeforeUnmount(() => {
    document.removeEventListener("click", onDocPointer, true);
    document.removeEventListener("touchstart", onDocPointer, true);
    document.removeEventListener("keydown", onDocKeydown);
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", updateVisualViewportBottomOffset);
  window.visualViewport?.removeEventListener(
    "resize",
    updateVisualViewportBottomOffset,
  );
  window.visualViewport?.removeEventListener(
    "scroll",
    updateVisualViewportBottomOffset,
  );
});

function openUploader() {
  browserRef.value?.openUploader();
}

function refresh() {
  browserRef.value?.refresh();
}

function goBack() {
  browserRef.value?.goBack();
}

function goRoot() {
  browserRef.value?.goRoot();
}

function toggleBatch() {
  browserRef.value?.toggleBatchMode();
}

const isBatchMode = computed(() => browserRef.value?.batchMode?.value ?? false);
const selectedCount = computed(
  () => browserRef.value?.selectedCount?.value ?? 0,
);
function setActionMode(mode: "nav" | "history" | "batch") {
  actionMode.value = mode;
  actionMenuOpen.value = false;
  batchMenuOpen.value = false;

  // 进入批量模式时自动开启批量
  if (mode === "batch" && !isBatchMode.value) {
    toggleBatch();
  }

  // 离开历史模式时回到当前版本（HEAD/worktree）
  if (mode !== "history" && browseCommit.value) {
    filesStore.setBrowseCommit(undefined);
  }

  if (mode === "history") {
    void loadDirHistory();
  }
}

const historyHashShort = computed(() => {
  const h = dirSelectedHash.value || browseCommit.value || "";
  return h ? h.substring(0, 8) : "--------";
});

const historyDateShort = computed(() => {
  const idx = dirSelectedIndex.value;
  const iso = idx >= 0 ? dirHistoryCommits.value[idx]?.date : undefined;
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";

  // 紧凑显示：MM-DD HH:mm（使用本地时区）
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${mm}-${dd} ${hh}:${mi}`;
});

const dirSelectedIndex = computed(() => {
  const h = dirSelectedHash.value;
  if (!h) return -1;
  return dirHistoryCommits.value.findIndex((c) => c.hash === h);
});

const canPrev = computed(() => {
  const idx = dirSelectedIndex.value;
  return idx >= 0 && idx < dirHistoryCommits.value.length - 1;
});

const canNext = computed(() => {
  const idx = dirSelectedIndex.value;
  return idx > 0;
});

const canLast = computed(() => {
  const idx = dirSelectedIndex.value;
  return idx > 0;
});

/** 账号头像用用户名首字符，未登录时回退到用户图标。 */
const accountInitial = computed(() => {
  const name = auth.user?.username || "";
  return name ? name.slice(0, 1).toUpperCase() : "";
});

const desktopHistoryStateLabel = computed(() => {
  if (dirHistoryLoading.value) return "加载历史...";
  if (browseCommit.value) return historyHashShort.value;
  return "当前版本";
});

watch(
  () => currentPath.value,
  () => {
    dirHistoryReqId += 1;
    dirHistoryPath.value = "";
    dirHistoryError.value = null;
    dirHistoryCommits.value = [];
    dirSelectedHash.value = browseCommit.value || "";

    if (browseCommit.value) {
      void loadDirHistory();
    }
  },
);

watch(
  () => browseCommit.value,
  (nextCommit) => {
    dirSelectedHash.value = nextCommit || "";

    if (nextCommit && dirHistoryPath.value !== (currentPath.value || "")) {
      void loadDirHistory();
    }
  },
);

function applyHistoryHash(hash: string | undefined) {
  if (!hash) return;
  dirSelectedHash.value = hash;
  filesStore.setBrowseCommit(hash);
}

function exitBrowseHistory() {
  dirSelectedHash.value = "";
  filesStore.setBrowseCommit(undefined);
}

async function ensureDirHistoryLoaded() {
  if (dirHistoryLoading.value) return false;

  if (
    dirHistoryPath.value !== (currentPath.value || "") ||
    dirHistoryCommits.value.length === 0
  ) {
    await loadDirHistory();
  }

  return dirHistoryCommits.value.length > 0;
}

async function desktopHistoryFastBack() {
  if (!(await ensureDirHistoryLoaded())) return;
  applyHistoryHash(
    dirHistoryCommits.value[dirHistoryCommits.value.length - 1]?.hash,
  );
}

async function desktopHistoryStepBack() {
  if (!(await ensureDirHistoryLoaded())) return;

  if (!browseCommit.value) {
    applyHistoryHash(dirHistoryCommits.value[0]?.hash);
    return;
  }

  const idx = dirSelectedIndex.value;
  const nextHash =
    idx >= 0 ? dirHistoryCommits.value[idx + 1]?.hash : undefined;
  applyHistoryHash(nextHash);
}

function desktopHistoryStepForward() {
  if (!browseCommit.value) return;

  const idx = dirSelectedIndex.value;
  if (idx > 0) {
    applyHistoryHash(dirHistoryCommits.value[idx - 1]?.hash);
    return;
  }

  exitBrowseHistory();
}

function desktopHistoryFastForward() {
  if (!browseCommit.value) return;
  exitBrowseHistory();
}

async function loadDirHistory() {
  const requestPath = currentPath.value || "";
  const reqId = ++dirHistoryReqId;
  dirHistoryLoading.value = true;
  dirHistoryError.value = null;
  try {
    const data = await filesService.getFileHistory(requestPath, 50);
    if (reqId !== dirHistoryReqId) return;
    dirHistoryPath.value = requestPath;
    dirHistoryCommits.value = (data.commits || []).map((c) => ({
      hash: c.hash,
      date: c.date,
    }));

    const preferred =
      browseCommit.value ||
      dirSelectedHash.value ||
      data.currentVersion ||
      data.commits?.[0]?.hash ||
      "";
    const exists =
      preferred && dirHistoryCommits.value.some((c) => c.hash === preferred);
    dirSelectedHash.value = exists
      ? preferred
      : data.currentVersion || data.commits?.[0]?.hash || "";

    if (dirSelectedHash.value) {
      filesStore.setBrowseCommit(dirSelectedHash.value);
    }
  } catch (err) {
    if (reqId !== dirHistoryReqId) return;
    dirHistoryPath.value = requestPath;
    dirHistoryError.value = err instanceof Error ? err.message : "加载历史失败";
    dirHistoryCommits.value = [];
    dirSelectedHash.value = "";
  } finally {
    if (reqId === dirHistoryReqId) dirHistoryLoading.value = false;
  }
}

function historyPrev() {
  const idx = dirSelectedIndex.value;
  if (idx < 0) return;
  const nextHash = dirHistoryCommits.value[idx + 1]?.hash;
  if (!nextHash) return;
  dirSelectedHash.value = nextHash;
  filesStore.setBrowseCommit(nextHash);
}

function historyNext() {
  const idx = dirSelectedIndex.value;
  if (idx <= 0) return;
  const nextHash = dirHistoryCommits.value[idx - 1]?.hash;
  if (!nextHash) return;
  dirSelectedHash.value = nextHash;
  filesStore.setBrowseCommit(nextHash);
}

function historyLast() {
  const lastHash = dirHistoryCommits.value[0]?.hash;
  if (!lastHash) return;
  dirSelectedHash.value = lastHash;
  filesStore.setBrowseCommit(lastHash);
}

async function runBatchMenuAction(action: "delete" | "move" | "rename") {
  if (action === "delete" && selectedCount.value === 0) return;
  if (action === "move" && selectedCount.value === 0) return;
  if (action === "rename" && selectedCount.value !== 1) return;

  batchMenuOpen.value = false;

  if (action === "delete") await batchDelete();
  if (action === "move") await batchMove();
  if (action === "rename") await renameSelected();
}

function selectAll() {
  browserRef.value?.selectAllVisible();
}

function clearSelection() {
  browserRef.value?.clearSelection();
}

async function batchDownload() {
  await browserRef.value?.batchDownload();
}

async function batchDelete() {
  await browserRef.value?.batchDelete();
}

async function batchMove() {
  await browserRef.value?.batchMove();
}

async function renameSelected() {
  await browserRef.value?.renameSelected();
}
</script>

<style scoped>
/* 主流云盘的中性底色：不再使用装饰性渐变，让内容面板成为唯一焦点 */
.home {
  position: relative;
  min-height: 100vh;
  /* 用 clip 而不是 hidden：hidden 会让 .home 变成滚动容器，导致内部的
     position: sticky（批量操作条）失效；clip 只裁剪不建立滚动容器。
     旧浏览器不支持 clip 时退回 hidden（仅失去吸顶效果）。 */
  overflow-x: clip;
  background: var(--vf-canvas);
}

@supports not (overflow: clip) {
  .home {
    overflow-x: hidden;
  }
}

.app-top-bar {
  padding-top: env(safe-area-inset-top);
  background: var(--vf-app-bar);
  border-bottom: 1px solid var(--vf-border-weak);
  box-shadow: none;
}

/* 版本切换：紧凑胶囊，左右各两个箭头，中间显示当前版本 */
.app-bar-history {
  display: inline-flex;
  align-items: center;
  gap: 0.1rem;
  padding: 0.1rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-pill);
  background: var(--vf-surface-sunken);
}

.app-bar-history.is-error {
  border-color: var(--vf-danger);
}

.app-bar-history-label {
  min-width: 5.5rem;
  padding: 0 0.5rem;
  text-align: center;
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--vf-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.app-bar-account {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  height: 2rem;
  padding: 0 0.6rem 0 0.25rem;
  border: 1px solid transparent;
  border-radius: var(--vf-radius-pill);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.82rem;
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    border-color 0.15s ease;
}

.app-bar-account:hover,
.app-bar-account:focus-visible {
  background: var(--vf-surface-hover);
  border-color: var(--vf-border-weak);
}

.app-bar-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.5rem;
  height: 1.5rem;
  border-radius: 50%;
  background: var(--vf-accent);
  color: var(--vf-on-accent);
  font-size: 0.74rem;
  font-weight: 700;
}

.app-bar-account-caret {
  color: var(--vf-text-muted);
}

.mobile-bottom-bar {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 2000;
  background: var(--vf-app-bar);
  border-top: 1px solid var(--vf-border-weak);
  box-shadow: none;
}

.mobile-bottom-bar-inner {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  padding: 0.35rem 0.5rem calc(0.35rem + env(safe-area-inset-bottom));
}

/* 图标 + 文字：触屏没有 hover，底部操作必须自解释 */
.mobile-bar-button {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.1rem;
  min-height: 2.75rem;
  padding: 0.25rem 0.35rem;
  line-height: 1.1;
}

.mobile-bar-label {
  font-size: 0.66rem;
  font-weight: 500;
  letter-spacing: 0.01em;
}

.mobile-bottom-bar-bottom {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.mobile-action-panel {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.mobile-batch-panel {
  width: 100%;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.mobile-history-panel {
  width: 100%;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.5rem;
}

.mobile-history-hash {
  flex: 0 0 auto;
}

.mobile-batch-actions {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  margin-left: auto;
  gap: 0.25rem;
  overflow: visible;
}

.mobile-batch-more {
  flex: 0 0 auto;
}

.mobile-batch-more .dropdown-menu {
  left: auto;
  right: 0;
  max-width: calc(100vw - 1rem);
}

.mobile-batch-count {
  flex: 0 0 auto;
}

/* 底部操作按钮：与桌面工具栏一致的图标按钮语言 */
/* 按钮等分剩余宽度且不换行：固定宽度会让 5 个按钮超出面板宽度并叠在一起 */
.mobile-action-buttons {
  display: flex;
  flex-wrap: nowrap;
  width: 100%;
  /* M3/HIG：相邻触控目标间距 ≥8px（原 2.4px 过紧，几乎相贴） */
  gap: 0.5rem;
}

.mobile-action-buttons .button {
  display: inline-flex;
  flex: 1 1 0;
  align-items: center;
  justify-content: center;
  width: auto;
  min-width: 0;
  height: 2.75rem;
  padding: 0 0.1rem;
  border: none;
  border-radius: var(--vf-radius);
  background: transparent;
  color: var(--vf-text-muted);
}

.mobile-action-buttons .button:hover,
.mobile-action-buttons .button:active {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.mobile-action-buttons .button.is-link {
  background: var(--vf-accent);
  color: var(--vf-on-accent);
}

.mobile-action-buttons .button.is-link:hover {
  background: var(--vf-accent-strong);
  color: var(--vf-on-accent);
}

.mobile-action-buttons {
  flex: 1;
  min-width: 0;
  overflow-x: auto;
  flex-wrap: nowrap;
  white-space: nowrap;
  justify-content: flex-end;
  margin-left: auto;
}

.dropdown-item.is-disabled {
  opacity: 0.45;
  pointer-events: none;
}

.home-content {
  position: relative;
  z-index: 1;
  padding-top: 1.25rem;
  padding-bottom: 6.5rem;
}

.home-main-container {
  min-height: 0;
}

.home-browser-shell {
  position: relative;
  min-width: 0;
}

.site-record-footer {
  padding: 1rem 0 0;
}

.site-record-link {
  font-size: 0.75rem;
  color: var(--vf-text-muted);
}

.site-record-link:hover,
.site-record-link:focus-visible {
  color: var(--vf-accent);
  text-decoration: underline;
}

@media screen and (min-width: 1024px) {
  /* 桌面端固定整屏高度：页面本身不滚动，文件列表在自己的内容区内滚动。
     这样工具栏、列头、状态栏与侧栏/详情面板始终可见（主流网盘的应用外壳）。 */
  .home {
    height: 100dvh;
    min-height: 100dvh;
    overflow: hidden;
  }

  .home-content {
    display: flex;
    flex-direction: column;
    height: 100dvh;
    overflow: hidden;
  }

  .home-main-container {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }

  .home-browser-shell {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }

  .site-record-footer {
    flex: 0 0 auto;
  }

  .mobile-bottom-bar {
    display: none;
  }

  .home-content {
    box-sizing: border-box;
    padding-top: calc(var(--bulma-navbar-height, 3.25rem) + 1.25rem);
    padding-bottom: 2rem;
  }

  .site-record-footer {
    padding-top: 1.25rem;
  }

  :deep(.file-browser-box) {
    border-radius: var(--vf-radius-lg);
    border: 1px solid var(--vf-border-weak);
    background: var(--vf-surface);
    box-shadow: var(--vf-shadow-card);
    padding: 0;
  }

  :deep(.file-browser-box) {
    display: flex;
    flex-direction: column;
  }

  :deep(.desktop-command-bar) {
    margin-bottom: 0;
  }
}

@media screen and (max-width: 1023px) {
  /* 展开的顶栏菜单：整行可点、文字与图标左对齐，并显示被 is-hidden-touch
     隐藏的标签（外观名称、账号名），否则菜单里只剩两个没有说明的图标。 */
  .app-top-bar .navbar-menu.is-active {
    /* Bulma 在移动端给 .navbar-menu 设了 overflow: auto，会裁掉展开的下拉；
       这个菜单只有两行，不需要内部滚动。 */
    overflow: visible;
    border-bottom: 1px solid var(--vf-border-weak);
    box-shadow: var(--vf-shadow-menu);
  }

  .app-top-bar .navbar-menu.is-active .navbar-item {
    display: flex;
    width: 100%;
    padding: 0.35rem 1rem;
  }

  /* ThemeToggle 是子组件：内部元素需要 :deep 才能命中（scoped 属性作用于子组件根节点） */
  .app-top-bar .navbar-menu.is-active .theme-toggle,
  .app-top-bar .navbar-menu.is-active .navbar-item > .dropdown {
    width: 100%;
  }

  .app-top-bar .navbar-menu.is-active :deep(.theme-toggle-button),
  .app-top-bar .navbar-menu.is-active .app-bar-account {
    width: 100%;
    justify-content: flex-start;
    gap: 0.5rem;
    height: 2.2rem;
    padding-left: 0.35rem;
    font-size: 0.9rem;
  }

  .app-top-bar .navbar-menu.is-active :deep(.theme-toggle-label) {
    display: inline !important;
  }

  .app-top-bar .navbar-menu.is-active .navbar-end .navbar-item + .navbar-item {
    border-top: 1px solid var(--vf-border-weak);
  }

  /* 展开的下拉（外观/账号）要盖住下面的行，否则会被后续行遮住 */
  .app-top-bar .navbar-menu.is-active .dropdown.is-active {
    position: relative;
    z-index: 60;
  }

  .app-top-bar .navbar-menu.is-active .dropdown.is-active .dropdown-menu {
    z-index: 60;
  }

  .section {
    padding: 0;
  }

  .home-content {
    padding-left: 0;
    padding-right: 0;
  }

  .mobile-bottom-bar {
    bottom: calc(env(safe-area-inset-bottom) + var(--vv-bottom, 0px));
  }

  .home-content {
    padding-top: calc(
      var(--bulma-navbar-height, 3.25rem) + env(safe-area-inset-top)
    );
    padding-bottom: calc(0.5rem + env(safe-area-inset-bottom));
    padding-bottom: calc(
      6.5rem + env(safe-area-inset-bottom) + var(--vv-bottom, 0px)
    );
  }

  .site-record-footer {
    padding: 0.75rem 0 0.5rem;
  }
}
</style>
