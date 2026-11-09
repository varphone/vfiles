import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import RecentView from "../src/views/files/RecentView.vue";
import { useAuthStore } from "../src/stores/auth.store";
import type { FileInfo } from "../src/types";

const { getRecentFilesMock, downloadFileMock, getOverviewMock } = vi.hoisted(
  () => ({
    getRecentFilesMock: vi.fn(),
    downloadFileMock: vi.fn(),
    getOverviewMock: vi.fn(),
  }),
);

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getRecentFiles: getRecentFilesMock,
    downloadFile: downloadFileMock,
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
  await router.push("/recent");
  await router.isReady();

  return {
    ...render(RecentView, { global: { plugins: [pinia, router] } }),
    router,
  };
}

function recentFile(overrides: Partial<FileInfo> = {}): FileInfo {
  const updatedAt = new Date().toISOString();
  return {
    id: "docs/report.pdf",
    name: "report.pdf",
    path: "docs/report.pdf",
    kind: "file",
    size_bytes: 2048,
    created_at: updatedAt,
    updated_at: updatedAt,
    ...overrides,
  };
}

beforeEach(() => {
  getRecentFilesMock.mockReset();
  getRecentFilesMock.mockResolvedValue([recentFile()]);
  downloadFileMock.mockReset();
  getOverviewMock.mockReset();
  getOverviewMock.mockResolvedValue({
    file_count: 0,
    directory_count: 0,
    total_bytes: 0,
    categories: [],
    recent_files: [],
  });
});

describe("RecentView.vue", () => {
  it("lists recent files with their size and modified time", async () => {
    const { container } = await renderView();

    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(1),
    );

    expect(getRecentFilesMock).toHaveBeenCalledWith(50);
    expect(
      screen.getByRole("button", { name: "report.pdf" }),
    ).toBeInTheDocument();
    expect(
      container.querySelector("tbody .recent-col-size")?.textContent?.trim(),
    ).toBe("2 KB");
    expect(
      container.querySelector("tbody .recent-col-time")?.textContent?.trim(),
    ).toContain("今天");
  });

  it("opens the containing directory when the file name is clicked", async () => {
    const { router } = await renderView();

    await fireEvent.click(
      await screen.findByRole("button", { name: "report.pdf" }),
    );

    await waitFor(() => expect(router.currentRoute.value.path).toBe("/"));
    expect(router.currentRoute.value.query.path).toBe("docs");
  });

  it("downloads a file from the row action", async () => {
    await renderView();

    await fireEvent.click(
      await screen.findByRole("button", { name: "下载 report.pdf" }),
    );

    expect(downloadFileMock).toHaveBeenCalledWith("docs/report.pdf");
  });

  it("shows the empty state when there are no recent files", async () => {
    getRecentFilesMock.mockResolvedValue([]);
    await renderView();

    expect(await screen.findByText("还没有最近文件")).toBeInTheDocument();
  });

  it("shows an error state and retries", async () => {
    getRecentFilesMock
      .mockRejectedValueOnce(new Error("网络错误"))
      .mockResolvedValueOnce([recentFile({ name: "notes.md" })]);

    const { container } = await renderView();

    await waitFor(() =>
      expect(container.querySelector(".empty-state.is-error")).not.toBeNull(),
    );
    expect(screen.getByText("网络错误")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "notes.md" }),
      ).toBeInTheDocument(),
    );
  });
});
