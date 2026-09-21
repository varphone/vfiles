import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createRouter, createMemoryHistory } from "vue-router";
import AuditLogs from "../src/views/AuditLogs.vue";
import type { AuditLogEntry } from "../src/types";

const { listLogsMock, listActionsMock } = vi.hoisted(() => ({
  listLogsMock: vi.fn(),
  listActionsMock: vi.fn(async () => ["login.success", "file.upload"]),
}));

vi.mock("../src/services/files.service", () => ({
  SEARCH_PAGE_SIZE: 100,
  filesService: {
    listAuditLogs: listLogsMock,
    listAuditActions: listActionsMock,
  },
}));

function entry(overrides: Partial<AuditLogEntry> = {}): AuditLogEntry {
  return {
    id: "a1",
    created_at: new Date().toISOString(),
    user_id: "u1",
    username: "alice",
    action: "login.success",
    result: "success",
    target: null,
    ip: "203.0.113.7",
    device: "Windows · Chrome",
    user_agent: "Mozilla/5.0",
    detail: "登录成功",
    ...overrides,
  };
}

function renderPage() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/", component: { template: "<div/>" } }],
  });
  return render(AuditLogs, { global: { plugins: [pinia, router] } });
}

describe("AuditLogs.vue", () => {
  beforeEach(() => {
    listLogsMock.mockReset();
    listLogsMock.mockResolvedValue({
      items: [entry()],
      total: 1,
      limit: 50,
      offset: 0,
    });
    listActionsMock.mockClear();
  });

  it("lists entries with time, user, action, result, ip and device", async () => {
    const { container } = renderPage();

    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(1),
    );
    const row = container.querySelector("tbody tr")!;
    expect(row.querySelector(".audit-user")?.textContent?.trim()).toBe("alice");
    // 动作标识映射为中文
    expect(row.querySelector(".audit-action")?.textContent?.trim()).toBe(
      "登录成功",
    );
    expect(row.querySelector(".audit-result")?.textContent?.trim()).toBe(
      "成功",
    );
    expect(row.querySelector(".audit-ip")?.textContent?.trim()).toBe(
      "203.0.113.7",
    );
    expect(row.querySelector(".audit-device")?.textContent?.trim()).toBe(
      "Windows · Chrome",
    );
  });

  it("states that the log is read-only", async () => {
    const { container } = renderPage();

    await waitFor(() =>
      expect(container.querySelector(".audit-readonly")).not.toBeNull(),
    );
    expect(container.querySelector(".audit-readonly")?.textContent).toContain(
      "不能修改或删除",
    );
  });

  it("passes filters to the service and resets the offset", async () => {
    const { container } = renderPage();
    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(1),
    );

    await fireEvent.update(
      screen.getByLabelText("搜索用户名或 IP"),
      "203.0.113",
    );
    await fireEvent.change(screen.getByLabelText("按结果筛选"), {
      target: { value: "failure" },
    });
    await fireEvent.click(screen.getByText("筛选"));

    await waitFor(() =>
      expect(listLogsMock).toHaveBeenLastCalledWith(
        expect.objectContaining({
          keyword: "203.0.113",
          result: "failure",
          offset: 0,
        }),
      ),
    );
  });

  it("shows the empty state and clears filters", async () => {
    listLogsMock.mockResolvedValue({
      items: [],
      total: 0,
      limit: 50,
      offset: 0,
    });
    const { container } = renderPage();

    await waitFor(() =>
      expect(container.querySelector(".empty-state-title")?.textContent).toBe(
        "暂无审计记录",
      ),
    );
    expect(container.querySelector(".audit-readonly")).not.toBeNull();
  });

  it("shows an error state with retry", async () => {
    listLogsMock.mockRejectedValue(new Error("没有权限执行该操作"));
    const { container } = renderPage();

    await waitFor(() =>
      expect(
        container.querySelector(".empty-state.is-error")?.textContent,
      ).toContain("没有权限执行该操作"),
    );
    expect(
      container.querySelector(".empty-state-actions button"),
    ).not.toBeNull();
  });

  it("offers a CSV export link carrying the current filters", async () => {
    const { container } = renderPage();
    await waitFor(() =>
      expect(container.querySelectorAll("tbody tr")).toHaveLength(1),
    );

    const exportLink = container.querySelector<HTMLAnchorElement>(
      'a[href^="/api/audit/logs.csv"]',
    )!;
    expect(exportLink).not.toBeNull();
    expect(exportLink.getAttribute("download")).not.toBeNull();
    expect(exportLink.getAttribute("href")).toBe("/api/audit/logs.csv");

    // 应用筛选后导出链接应带上条件
    await fireEvent.update(
      screen.getByLabelText("搜索用户名或 IP"),
      "203.0.113",
    );
    await fireEvent.change(screen.getByLabelText("按结果筛选"), {
      target: { value: "failure" },
    });
    await fireEvent.click(screen.getByText("筛选"));

    await waitFor(() =>
      expect(
        container
          .querySelector<HTMLAnchorElement>('a[href^="/api/audit/logs.csv"]')!
          .getAttribute("href"),
      ).toBe("/api/audit/logs.csv?keyword=203.0.113&result=failure"),
    );
  });

  it("disables the export link when there is nothing to export", async () => {
    listLogsMock.mockResolvedValue({
      items: [],
      total: 0,
      limit: 50,
      offset: 0,
    });
    const { container } = renderPage();

    await waitFor(() =>
      expect(container.querySelector(".empty-state-title")).not.toBeNull(),
    );
    const exportLink = container.querySelector<HTMLAnchorElement>(
      'a[href^="/api/audit/logs.csv"]',
    )!;
    expect(exportLink.getAttribute("aria-disabled")).toBe("true");
    expect(exportLink.classList.contains("is-disabled")).toBe(true);
  });

  it("paginates when the total exceeds one page", async () => {
    listLogsMock.mockResolvedValue({
      items: [entry()],
      total: 120,
      limit: 50,
      offset: 0,
    });
    const { container } = renderPage();

    await waitFor(() =>
      expect(container.querySelector(".audit-pager")).not.toBeNull(),
    );
    expect(
      container.querySelector(".audit-pager-info")?.textContent?.trim(),
    ).toBe("第 1 / 3 页");

    await fireEvent.click(screen.getByText("下一页"));
    await waitFor(() =>
      expect(listLogsMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ offset: 50 }),
      ),
    );
  });
});
