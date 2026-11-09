import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import ViewsSidebar from "../src/components/files/ViewsSidebar.vue";
import { useAuthStore } from "../src/stores/auth.store";

const { getOverviewMock } = vi.hoisted(() => ({
  getOverviewMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getOverview: getOverviewMock,
  },
}));

const Blank = { render: () => null };

const routes = [
  { path: "/", name: "home", component: Blank },
  { path: "/recent", name: "recent", component: Blank },
  { path: "/favorites", name: "favorites", component: Blank },
  { path: "/shares", name: "shares", component: Blank },
  { path: "/activity", name: "activity", component: Blank },
];

type SidebarOptions = {
  restricted?: boolean;
  path?: string;
};

async function renderSidebar(options: SidebarOptions = {}) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const auth = useAuthStore(pinia);
  auth.$patch({
    enabled: true,
    user: { id: "user-1", username: "admin", role: "admin" },
    scope: {
      restricted: options.restricted === true,
      label: options.restricted ? "受限" : null,
      roots: [],
      canWrite: true,
    },
    features: null,
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes,
  });
  await router.push(options.path ?? "/");
  await router.isReady();

  return {
    ...render(ViewsSidebar as any, {
      global: { plugins: [pinia, router] },
    }),
    auth,
    router,
  };
}

beforeEach(() => {
  getOverviewMock.mockReset();
  getOverviewMock.mockResolvedValue({
    file_count: 12,
    directory_count: 3,
    total_bytes: 2048,
    categories: [],
    recent_files: [],
  });
});

describe("ViewsSidebar.vue", () => {
  it("renders the five views and marks the current one", async () => {
    await renderSidebar({ path: "/favorites" });

    for (const label of ["全部文件", "最近", "收藏", "分享", "活动"]) {
      expect(screen.getByRole("link", { name: label })).toBeInTheDocument();
    }

    expect(screen.getByRole("link", { name: "收藏" })).toHaveClass("is-active");
    expect(screen.getByRole("link", { name: "全部文件" })).not.toHaveClass(
      "is-active",
    );
  });

  it("paints the active view as a solid primary block with white text", async () => {
    await renderSidebar({ path: "/favorites" });

    const active = screen.getByRole("link", { name: "收藏" });
    expect(active).toHaveClass("is-active");

    // jsdom 不套用 SFC 样式：核对样式契约（主色实心 + 白字 + 8px 圆角）。
    const source = readFileSync(
      resolve(process.cwd(), "src/components/files/ViewsSidebar.vue"),
      "utf8",
    );
    expect(source).toContain(
      "--vf-sidebar-item-radius: var(--vf-radius-card)",
    );
    expect(source).toContain(
      "--vf-sidebar-item-active-bg: var(--vf-accent)",
    );
    expect(source).toContain(
      "--vf-sidebar-item-active-fg: var(--vf-on-accent)",
    );
    const activeRule = source.match(
      /\.views-sidebar-item\.is-active\s*\{([\s\S]*?)\n\}/,
    )![1];
    expect(activeRule).toContain(
      "background: var(--vf-sidebar-item-active-bg)",
    );
    expect(activeRule).toContain("color: var(--vf-sidebar-item-active-fg)");
    // 8px 圆角来自条目基类（选中态不覆盖它）。
    expect(source).toContain(
      "border-radius: var(--vf-sidebar-item-radius)",
    );
  });

  it("navigates to a view when its item is clicked", async () => {
    const { router } = await renderSidebar();

    await fireEvent.click(screen.getByRole("link", { name: "最近" }));

    await waitFor(() => expect(router.currentRoute.value.path).toBe("/recent"));
    expect(screen.getByRole("link", { name: "最近" })).toHaveClass("is-active");
  });

  it("shows storage usage from the workspace overview", async () => {
    await renderSidebar();

    expect(await screen.findByText("存储用量")).toBeInTheDocument();
    expect(await screen.findByText("2 KB")).toBeInTheDocument();
    expect(screen.getByText("12 文件 · 3 目录")).toBeInTheDocument();
    expect(getOverviewMock).toHaveBeenCalledTimes(1);
  });

  it("hides the storage block for restricted sessions", async () => {
    await renderSidebar({ restricted: true });

    expect(screen.queryByText("存储用量")).toBeNull();
    expect(getOverviewMock).not.toHaveBeenCalled();
  });
});
