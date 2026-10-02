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
          webdav_embedded: true,
          webdav_mount: "/dav",
          protocols: [
            {
              id: "http",
              enabled: true,
              bind: "0.0.0.0:8080",
              embedded: false,
              mount_path: null,
              writable: null,
              module: null,
              passive_ports: null,
            },
            {
              id: "webdav",
              enabled: true,
              bind: "0.0.0.0:18080",
              embedded: true,
              mount_path: "/dav",
              writable: null,
              module: null,
              passive_ports: null,
            },
            {
              id: "s3",
              enabled: false,
              bind: "0.0.0.0:9000",
              embedded: true,
              mount_path: null,
              writable: null,
              module: null,
              passive_ports: null,
            },
          ],
          storage: {
            user_count: 2,
            file_count: 3,
            directory_count: 1,
            total_bytes: 1536,
            users: [
              {
                user_id: "u-alice",
                username: "alice",
                role: "user",
                disabled: false,
                file_count: 2,
                directory_count: 1,
                total_bytes: 1024,
              },
              {
                user_id: "u-empty",
                username: "empty",
                role: "manager",
                disabled: true,
                file_count: 1,
                directory_count: 0,
                total_bytes: 512,
              },
            ],
          },
        },
      })),
    },
  };
});

describe("SystemInfo.vue", () => {
  it("shows enabled protocols and per-user storage in a system overview", async () => {
    renderWithProviders(SystemInfo);
    await waitFor(() => expect(screen.getByText("2.2.0")).toBeInTheDocument());
    expect(screen.getByText("linux/x86_64")).toBeInTheDocument();
    // uptime 格式化（3661s = 1 小时 1 分）
    expect(screen.getByText("1 小时 1 分")).toBeInTheDocument();
    expect(screen.getByText("存储与用量")).toBeInTheDocument();
    expect(screen.getByText("1.5 KB")).toBeInTheDocument();
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("1 KB")).toBeInTheDocument();
    expect(screen.getByText("empty")).toBeInTheDocument();
    expect(screen.getByText("2 个文件")).toBeInTheDocument();
    expect(screen.getByText("HTTP API")).toBeInTheDocument();
    expect(screen.getByText("WebDAV")).toBeInTheDocument();
    expect(screen.getByText("S3 兼容存储")).toBeInTheDocument();
    expect(screen.getAllByText("已启用").length).toBeGreaterThan(0);
    expect(screen.getByText("未启用")).toBeInTheDocument();
    expect(screen.getByText(/启动配置/)).toBeInTheDocument();
    // 标准工具条（刷新 + 返回文件）
    expect(screen.getByText("刷新")).toBeInTheDocument();
    expect(screen.getByText("返回文件")).toBeInTheDocument();
  });
});
