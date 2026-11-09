import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import FavoritesView from "../src/views/files/FavoritesView.vue";
import { useAuthStore } from "../src/stores/auth.store";
import type { FavoriteEntry, FavoritePage } from "../src/types";

const { getFavoritesMock, removeFavoriteMock, getOverviewMock } = vi.hoisted(
  () => ({
    getFavoritesMock: vi.fn(),
    removeFavoriteMock: vi.fn(),
    getOverviewMock: vi.fn(),
  }),
);

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getFavorites: getFavoritesMock,
    removeFavorite: removeFavoriteMock,
    getOverview: getOverviewMock,
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

async function renderView() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const auth = useAuthStore(pinia);
  auth.$patch({
    enabled: true,
    user: { id: "user-1", username: "alice", role: "user" },
    scope: { restricted: false, label: null, roots: [], canWrite: true },
    features: null,
  });

  const router = createRouter({ history: createMemoryHistory(), routes });
  await router.push("/favorites");
  await router.isReady();

  return {
    ...render(FavoritesView, { global: { plugins: [pinia, router] } }),
    router,
  };
}

function favorite(
  path: string,
  name: string,
  kind: FavoriteEntry["kind"] = "file",
): FavoriteEntry {
  return { path, name, kind };
}

function favoritePage(
  items: FavoriteEntry[],
  overrides: Partial<FavoritePage> = {},
): FavoritePage {
  return {
    items,
    total: items.length,
    limit: 50,
    offset: 0,
    has_more: false,
    ...overrides,
  };
}

beforeEach(() => {
  getFavoritesMock.mockReset();
  getFavoritesMock.mockResolvedValue(
    favoritePage([
      favorite("docs/report.pdf", "report.pdf"),
      favorite("notes.md", "notes.md"),
    ]),
  );
  removeFavoriteMock.mockReset();
  removeFavoriteMock.mockResolvedValue(favoritePage([]));
  getOverviewMock.mockReset();
  getOverviewMock.mockResolvedValue({
    file_count: 0,
    directory_count: 0,
    total_bytes: 0,
    categories: [],
    recent_files: [],
  });
});

describe("FavoritesView.vue", () => {
  it("lists favorites with their containing directory", async () => {
    const { container } = await renderView();

    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(2),
    );

    expect(getFavoritesMock).toHaveBeenCalledWith(50, 0);
    expect(
      screen.getByRole("button", { name: "report.pdf" }),
    ).toBeInTheDocument();
    expect(
      container.querySelector("tbody .favorites-location")?.textContent?.trim(),
    ).toBe("/docs");
    expect(screen.getByText("共 2 项")).toBeInTheDocument();
  });

  it("navigates to the containing directory when a favorite is opened", async () => {
    const { router } = await renderView();

    await fireEvent.click(
      await screen.findByRole("button", { name: "report.pdf" }),
    );

    await waitFor(() => expect(router.currentRoute.value.path).toBe("/"));
    expect(router.currentRoute.value.query.path).toBe("docs");
  });

  it("removes a favorite from the list after unfavouriting it", async () => {
    const { container } = await renderView();
    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(2),
    );

    await fireEvent.click(
      screen.getByRole("button", { name: "取消收藏 report.pdf" }),
    );

    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "取消收藏 report.pdf" }),
      ).toBeNull(),
    );
    expect(removeFavoriteMock).toHaveBeenCalledWith("docs/report.pdf");
    expect(container.querySelectorAll("tbody tr")).toHaveLength(1);
    expect(screen.getByText("共 1 项")).toBeInTheDocument();
  });

  it("loads the next page with a 50-item offset", async () => {
    getFavoritesMock
      .mockResolvedValueOnce(
        favoritePage([favorite("docs/report.pdf", "report.pdf")], {
          total: 2,
          has_more: true,
        }),
      )
      .mockResolvedValueOnce(
        favoritePage([favorite("notes.md", "notes.md")], {
          total: 2,
          offset: 1,
          has_more: false,
        }),
      );

    const { container } = await renderView();
    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(1),
    );

    await fireEvent.click(screen.getByRole("button", { name: "加载更多" }));

    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(2),
    );
    expect(getFavoritesMock).toHaveBeenLastCalledWith(50, 1);
  });

  it("shows the empty state when nothing is favourited", async () => {
    getFavoritesMock.mockResolvedValue(favoritePage([]));
    await renderView();

    expect(await screen.findByText("还没有收藏")).toBeInTheDocument();
  });
});
