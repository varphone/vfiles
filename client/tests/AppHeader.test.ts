import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import AppHeader from "../src/components/shell/AppHeader.vue";
import { useAuthStore } from "../src/stores/auth.store";

/** 顶栏里的 RouterLink 需要真实路由；组件只关心路径，页面用空组件占位。 */
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

type HeaderOptions = {
  role?: "admin" | "manager" | "user";
  restricted?: boolean;
};

function renderHeader(options: HeaderOptions = {}) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const auth = useAuthStore(pinia);
  auth.$patch({
    enabled: true,
    user: {
      id: "user-1",
      username: options.role === "user" ? "alice" : "admin",
      role: options.role ?? "admin",
    },
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

  return {
    ...render(AppHeader as any, {
      global: { plugins: [pinia, router] },
    }),
    auth,
    router,
  };
}

beforeEach(() => {
  setActivePinia(createPinia());
});

describe("AppHeader.vue", () => {
  it("renders the app icon group and navigates between apps", async () => {
    const { container, router } = renderHeader();

    const appLinks = container.querySelectorAll(".app-header-app");
    expect(appLinks).toHaveLength(4);
    expect(screen.getByRole("link", { name: "文件" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "活动" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "分享" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "管理" })).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("link", { name: "活动" }));

    await waitFor(() =>
      expect(router.currentRoute.value.path).toBe("/activity"),
    );
    expect(screen.getByRole("link", { name: "活动" })).toHaveClass("is-active");
    expect(screen.getByRole("link", { name: "文件" })).not.toHaveClass(
      "is-active",
    );
  });

  it("keeps the files icon active for its sub views", async () => {
    const { router } = renderHeader();

    await router.push("/favorites");

    await waitFor(() =>
      expect(screen.getByRole("link", { name: "文件" })).toHaveClass(
        "is-active",
      ),
    );
  });

  it("hides share and admin icons when they are not available", () => {
    renderHeader({ role: "user" });
    expect(screen.queryByRole("link", { name: "管理" })).toBeNull();
    expect(screen.getByRole("link", { name: "分享" })).toBeInTheDocument();
  });

  it("hides every account-level entry for restricted sessions", async () => {
    renderHeader({ restricted: true });

    expect(screen.queryByRole("link", { name: "分享" })).toBeNull();
    expect(screen.queryByRole("link", { name: "管理" })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "账号菜单" }));
    expect(screen.queryByRole("link", { name: "我的分享" })).toBeNull();
    expect(screen.queryByRole("link", { name: "访问密码" })).toBeNull();
    expect(screen.queryByRole("link", { name: "访问令牌" })).toBeNull();
    expect(screen.queryByRole("link", { name: "管理控制台" })).toBeNull();
  });

  it("opens the unified search from the search button", async () => {
    const { container } = renderHeader();

    await fireEvent.click(screen.getByRole("button", { name: "搜索" }));

    expect(container.querySelector(".modal")).toHaveClass("is-active");
  });

  it("opens the unified search with Ctrl+K / Cmd+K", async () => {
    const { container } = renderHeader();
    const modal = container.querySelector(".modal") as HTMLElement;
    expect(modal).not.toHaveClass("is-active");

    await fireEvent.keyDown(document, { key: "k", ctrlKey: true });

    expect(modal).toHaveClass("is-active");
    await waitFor(() =>
      expect(screen.getByPlaceholderText("搜索文件与文件夹…")).toHaveFocus(),
    );
  });
});
