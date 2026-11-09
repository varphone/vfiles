import { render, screen, waitFor, within } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import Home from "../src/views/Home.vue";
import { useAuthStore } from "../src/stores/auth.store";

const {
  getFilesPageMock,
  getOverviewMock,
  getSharedPathsMock,
  getFileHistoryMock,
} = vi.hoisted(() => ({
  getFilesPageMock: vi.fn(),
  getOverviewMock: vi.fn(),
  getSharedPathsMock: vi.fn(),
  getFileHistoryMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  MAX_MOVE_BATCH_ITEMS: 500,
  MAX_TRANSFER_PATHS: 500,
  filesService: {
    getFilesPage: getFilesPageMock,
    getOverview: getOverviewMock,
    getSharedPaths: getSharedPathsMock,
    getFileHistory: getFileHistoryMock,
    getDirectoriesPage: vi.fn(async () => ({
      items: [],
      total: 0,
      limit: 200,
      offset: 0,
      has_more: false,
    })),
    searchFiles: vi.fn(async () => ({
      items: [],
      hasMore: false,
      limit: 100,
      offset: 0,
    })),
  },
}));

const Blank = { render: () => null };

const routes = [
  { path: "/", name: "home", component: Blank },
  { path: "/recent", name: "recent", component: Blank },
  { path: "/favorites", name: "favorites", component: Blank },
  { path: "/activity", name: "activity", component: Blank },
  { path: "/shares", name: "shares", component: Blank },
  { path: "/admin", name: "admin", component: Blank },
  { path: "/login", name: "login", component: Blank },
  { path: "/settings/passwords", name: "passwords", component: Blank },
  { path: "/settings/tokens", name: "tokens", component: Blank },
];

/** 视图偏好存进 localStorage，测试环境（bun + jsdom）的实现不完整，这里显式提供。 */
function installLocalStorageStub() {
  const backing = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (key: string) => (backing.has(key) ? backing.get(key)! : null),
      setItem: (key: string, value: string) => backing.set(key, String(value)),
      removeItem: (key: string) => backing.delete(key),
      clear: () => backing.clear(),
      key: (index: number) => Array.from(backing.keys())[index] ?? null,
      get length() {
        return backing.size;
      },
    },
    configurable: true,
    writable: true,
  });
}

/** 桌面视口：FileBrowser 的 isMobile 为 false，版本切换控件应出现在面包屑行。 */
function stubBrowserApis() {
  vi.stubGlobal("matchMedia", (q: string) => ({
    matches: false,
    media: q,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
}

async function renderHome() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const auth = useAuthStore(pinia);
  auth.$patch({
    enabled: true,
    user: { id: "user-1", username: "admin", role: "admin" },
    scope: { restricted: false, label: null, roots: [], canWrite: true },
    features: null,
  });

  const router = createRouter({ history: createMemoryHistory(), routes });
  await router.push("/");
  await router.isReady();

  return {
    ...render(Home, { global: { plugins: [pinia, router] } }),
    router,
    auth,
  };
}

beforeEach(() => {
  installLocalStorageStub();
  localStorage.setItem(
    "vfiles:file-browser:view",
    JSON.stringify({
      mode: "list",
      sortField: "name",
      sortDirection: "asc",
      foldersFirst: true,
      thumbnailSize: 144,
      detailsVisible: false,
      libraryCards: false,
    }),
  );
  stubBrowserApis();
  getFilesPageMock.mockReset();
  getFilesPageMock.mockResolvedValue({
    items: [],
    total: 0,
    limit: 200,
    offset: 0,
    has_more: false,
  });
  getSharedPathsMock.mockReset();
  getSharedPathsMock.mockResolvedValue([]);
  getOverviewMock.mockReset();
  getOverviewMock.mockResolvedValue({
    file_count: 0,
    directory_count: 0,
    total_bytes: 0,
    categories: [],
    recent_files: [],
  });
  getFileHistoryMock.mockReset();
  getFileHistoryMock.mockResolvedValue({
    commits: [],
    totalCommits: 0,
    nextCursor: null,
    currentVersion: undefined,
  });
});

describe("Home.vue", () => {
  it("renders the page-level view sidebar with all five entries", async () => {
    await renderHome();

    const sidebar = await screen.findByRole("navigation", { name: "视图" });
    for (const label of ["全部文件", "最近", "收藏", "分享", "活动"]) {
      expect(
        within(sidebar).getByRole("link", { name: label }),
      ).toBeInTheDocument();
    }
    expect(
      within(sidebar).getByRole("link", { name: "全部文件" }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("keeps the directory tree out of the file page and history in the breadcrumb row", async () => {
    const { container } = await renderHome();

    await waitFor(() => expect(getFilesPageMock).toHaveBeenCalled());

    // 旧 FileBrowser 侧栏（快捷导航 / 目录树 / 概览）与旧 Home 的 .section 外壳整体解绑。
    for (const legacyClass of [
      "section",
      "browser-sidebar",
      "browser-nav-item",
      "directory-tree",
      "sidebar-overview",
    ]) {
      expect(container.querySelector(`.${legacyClass}`)).toBeNull();
    }

    // AppHeader #center 注入也已移除。
    expect(container.querySelector(".app-header-center")).toBeNull();

    // 版本切换位于面包屑行内的右侧工具组。
    const breadcrumbBar = container.querySelector(".breadcrumb-bar");
    const history = breadcrumbBar?.querySelector(".file-history");
    expect(history).not.toBeNull();
    expect(
      within(history as HTMLElement).getByRole("button", {
        name: "后退到更早快照",
      }),
    ).toBeInTheDocument();
    expect(
      within(history as HTMLElement).getByText("当前版本"),
    ).toBeInTheDocument();
  });
});
