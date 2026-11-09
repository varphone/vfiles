import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import ActivityView from "../src/views/ActivityView.vue";
import { useAuthStore } from "../src/stores/auth.store";
import type { AuditLogEntry } from "../src/types";

const { getActivityMock, getOverviewMock } = vi.hoisted(() => ({
  getActivityMock: vi.fn(),
  getOverviewMock: vi.fn(),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: {
    getActivity: getActivityMock,
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
  await router.push("/activity");
  await router.isReady();

  return {
    ...render(ActivityView, { global: { plugins: [pinia, router] } }),
    router,
  };
}

function todayAt(hour = 10): string {
  const now = new Date();
  return new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
    hour,
    0,
  ).toISOString();
}

function yesterdayAt(hour = 12): string {
  const now = new Date();
  return new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() - 1,
    hour,
    0,
  ).toISOString();
}

function activityEntry(overrides: Partial<AuditLogEntry> = {}): AuditLogEntry {
  return {
    id: "a1",
    created_at: todayAt(),
    user_id: "u1",
    username: "alice",
    action: "file.upload",
    result: "success",
    target: "docs/report.pdf",
    ip: null,
    device: null,
    user_agent: null,
    detail: null,
    ...overrides,
  };
}

beforeEach(() => {
  getActivityMock.mockReset();
  getActivityMock.mockResolvedValue({
    items: [activityEntry()],
    total: 1,
  });
  getOverviewMock.mockReset();
  getOverviewMock.mockResolvedValue({
    file_count: 0,
    directory_count: 0,
    total_bytes: 0,
    categories: [],
    recent_files: [],
  });
});

describe("ActivityView.vue", () => {
  it("groups entries by day and maps action names", async () => {
    getActivityMock.mockResolvedValue({
      items: [
        activityEntry({
          id: "a1",
          action: "file.upload",
          target: "docs/report.pdf",
          created_at: todayAt(10),
        }),
        activityEntry({
          id: "a2",
          action: "file.delete",
          target: "old.txt",
          result: "failure",
          created_at: yesterdayAt(12),
        }),
        activityEntry({
          id: "a3",
          action: "login.success",
          target: null,
          created_at: new Date(2024, 0, 5, 9, 0).toISOString(),
        }),
      ],
      total: 3,
    });

    const { container } = await renderView();
    await waitFor(() =>
      expect(container.querySelectorAll(".activity-item")).toHaveLength(3),
    );

    expect(getActivityMock).toHaveBeenCalledWith({
      filter: "all",
      limit: 30,
      offset: 0,
    });
    expect(screen.getByRole("heading", { name: "今天" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "昨天" })).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "2024年1月5日" }),
    ).toBeInTheDocument();
    for (const label of ["上传", "删除", "登录"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(
      container.querySelector(".activity-avatar")?.textContent?.trim(),
    ).toBe("A");
    // 接口只返回本人记录，页面不应出现「他人活动」相关文案
    expect(screen.queryByText(/他人/)).toBeNull();
  });

  it("shows the empty state when there is no activity", async () => {
    getActivityMock.mockResolvedValue({ items: [], total: 0 });
    await renderView();

    expect(await screen.findByText("还没有活动记录")).toBeInTheDocument();
  });

  it("refetches with the files filter when a chip is selected", async () => {
    getActivityMock
      .mockResolvedValueOnce({ items: [activityEntry()], total: 1 })
      .mockResolvedValueOnce({ items: [], total: 0 });

    const { container } = await renderView();
    await waitFor(() =>
      expect(container.querySelectorAll(".activity-item")).toHaveLength(1),
    );

    await fireEvent.click(screen.getByRole("button", { name: "文件变更" }));

    await waitFor(() =>
      expect(getActivityMock).toHaveBeenLastCalledWith({
        filter: "files",
        limit: 30,
        offset: 0,
      }),
    );
    expect(screen.getByRole("button", { name: "文件变更" })).toHaveClass(
      "is-active",
    );
    expect(await screen.findByText("还没有活动记录")).toBeInTheDocument();
  });

  it("loads the next page with a 30-item offset", async () => {
    getActivityMock
      .mockResolvedValueOnce({
        items: [
          activityEntry({ id: "a1" }),
          activityEntry({ id: "a2", action: "file.rename" }),
        ],
        total: 3,
      })
      .mockResolvedValueOnce({
        items: [
          activityEntry({
            id: "a3",
            action: "directory.create",
            target: "docs/archive",
            created_at: new Date(2024, 0, 5, 9, 0).toISOString(),
          }),
        ],
        total: 3,
      });

    const { container } = await renderView();
    await waitFor(() =>
      expect(container.querySelectorAll(".activity-item")).toHaveLength(2),
    );

    await fireEvent.click(screen.getByRole("button", { name: "加载更多" }));

    await waitFor(() =>
      expect(container.querySelectorAll(".activity-item")).toHaveLength(3),
    );
    expect(getActivityMock).toHaveBeenLastCalledWith({
      filter: "all",
      limit: 30,
      offset: 2,
    });
    expect(screen.getByText("新建目录")).toBeInTheDocument();
  });

  it("shows an error state and retries", async () => {
    getActivityMock
      .mockRejectedValueOnce(new Error("网络错误"))
      .mockResolvedValueOnce({ items: [activityEntry()], total: 1 });

    const { container } = await renderView();

    await waitFor(() =>
      expect(container.querySelector(".empty-state.is-error")).not.toBeNull(),
    );
    expect(screen.getByText("网络错误")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() =>
      expect(container.querySelectorAll(".activity-item")).toHaveLength(1),
    );
  });
});
