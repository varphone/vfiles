import { screen, waitFor } from "@testing-library/vue";
import { describe, it, expect, vi } from "vitest";
import { renderWithProviders } from "./renderWithProviders";
import SystemInfo from "../src/views/SystemInfo.vue";

vi.mock("../src/services/auth.service", async () => {
  const actual = await vi.importActual<
    typeof import("../src/services/auth.service")
  >("../src/services/auth.service");
  return {
    authService: {
      ...actual.authService,
      systemInfo: vi.fn(async () => ({
        success: true,
        data: {
          version: "2.2.0",
          os: "linux",
          arch: "x86_64",
          uptime_secs: 3661,
          started_at: "2026-09-22T19:20:00Z",
          webdav_enabled: true,
          webdav_bind: "0.0.0.0:18080",
        },
      })),
      listUsers: vi.fn(async () => ({
        success: true,
        data: { users: [], total_count: 2, page: 1, page_size: 20 },
      })),
    },
  };
});

describe("SystemInfo.vue (r106 看板)", () => {
  it("renders system, storage and user cards", async () => {
    renderWithProviders(SystemInfo);
    await waitFor(() => expect(screen.getByText("2.2.0")).toBeInTheDocument());
    expect(screen.getByText("linux/x86_64")).toBeInTheDocument();
    // uptime 格式化（3661s = 1 小时 1 分）
    expect(screen.getByText("1 小时 1 分")).toBeInTheDocument();
    // 存储卡标题（组件嵌入 ✓ SidebarOverview 自足）
    expect(screen.getByText("存储与用量")).toBeInTheDocument();
    // 用户统计（listUsers total_count ✓）
    await waitFor(() => expect(screen.getByText("2")).toBeInTheDocument());
    // 标准工具条（刷新 + 返回文件）
    expect(screen.getByText("刷新")).toBeInTheDocument();
    expect(screen.getByText("返回文件")).toBeInTheDocument();
    // WebDAV 接入卡（r115 ✓ 端点与挂载指引）
    expect(screen.getByText("WebDAV 接入")).toBeInTheDocument();
    expect(screen.getByText("端点地址", { selector: "dt" })).toBeInTheDocument();
  });
});
