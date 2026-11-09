import { fireEvent, screen, waitFor } from "@testing-library/vue";
import { nextTick } from "vue";
import { useAuthStore } from "../src/stores/auth.store";
import { describe, it, expect, vi } from "vitest";
import { renderWithProviders } from "./renderWithProviders";
import AdminUsers from "../src/views/AdminUsers.vue";

const mockUsers = [
  {
    id: "u1",
    username: "alice",
    email: "alice@example.com",
    role: "user" as const,
    disabled: false,
    createdAt: "2026-01-01T00:00:00Z",
  },
];

vi.mock("../src/composables/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  promptDialog: vi.fn(async () => null),
}));

vi.mock("../src/services/auth.service", async () => {
  const actual = await vi.importActual<
    typeof import("../src/services/auth.service")
  >("../src/services/auth.service");
  return {
    authService: {
      ...actual.authService,
      resetUserPassword: vi.fn(async () => ({ success: true })),
      deleteUser: vi.fn(async () => ({ success: true })),
      listUsers: vi.fn(async () => ({
        success: true,
        data: { users: mockUsers },
      })),
      setUserEmail: vi.fn(async () => ({ success: true })),
      setUserDisabled: vi.fn(async () => ({ success: true })),
      setUserRole: vi.fn(async () => ({ success: true })),
      revokeUserSessions: vi.fn(async () => ({ success: true })),
    },
  };
});

import { authService } from "../src/services/auth.service";

describe("AdminUsers.vue", () => {
  it("loads users, edits the email inline and revokes sessions", async () => {
    const {
      findByRole,
      findByText,
      findByPlaceholderText,
      getAllByText,
      getByLabelText,
    } = renderWithProviders(AdminUsers);

    expect(
      document.querySelector(
        ".admin-shell .admin-main-inner > .vf-page-header",
      ),
    ).toBeInTheDocument();
    await findByRole("heading", { name: "用户" });
    await findByText("alice");

    // 只有管理员（或被授权者）能看到编辑入口
    const auth = useAuthStore();
    auth.user = {
      id: "root",
      username: "root",
      role: "admin",
      email: "root@example.com",
    } as any;
    await nextTick();
    // 邮箱默认是只读文本，不再整列都是输入框
    expect(getAllByText("alice@example.com").length).toBeGreaterThan(0);

    // 点铅笔进入行内编辑
    await fireEvent.click(getByLabelText("修改 alice 的邮箱"));
    const emailInput = (await findByPlaceholderText(
      "user@example.com",
    )) as HTMLInputElement;
    expect(emailInput.value).toBe("alice@example.com");

    await fireEvent.update(emailInput, "new@example.com");
    await fireEvent.click(getAllByText("保存")[0]);

    await waitFor(() =>
      expect(
        (authService.setUserEmail as any).mock.calls.length,
      ).toBeGreaterThanOrEqual(1),
    );
    const [calledId, calledEmail] = (authService.setUserEmail as any).mock
      .calls[0];
    expect(calledId).toBe("u1");
    expect(calledEmail).toBe("new@example.com");

    vi.stubGlobal("confirm", () => true);
    await fireEvent.click(getAllByText("强制下线")[0]);
    expect(
      (authService.revokeUserSessions as any).mock.calls.length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("asks the server for keyword and role filters", async () => {
    const listUsers = vi.mocked(authService.listUsers);
    listUsers.mockImplementation(async (query: any = {}) => {
      const users = query.q
        ? []
        : mockUsers.filter((user) => !query.role || user.role === query.role);
      return {
        success: true,
        data: {
          users,
          capabilities: { canUpdateEmail: true, canRevokeSessions: true },
          total_count: users.length,
        },
      };
    });

    const { findByText, getByLabelText } = renderWithProviders(AdminUsers);
    await findByText("alice");

    const search = getByLabelText("搜索用户名或邮箱");
    await fireEvent.update(search, "zzz");
    await waitFor(
      () =>
        expect(listUsers).toHaveBeenCalledWith(
          expect.objectContaining({ q: "zzz", page: 1 }),
        ),
      { timeout: 3000 },
    );
    await waitFor(() =>
      expect(document.body.textContent).toContain("没有找到匹配的用户"),
    );

    await fireEvent.click(screen.getByText("清除筛选"));
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());

    listUsers.mockResolvedValue({
      success: true,
      data: {
        users: mockUsers,
        capabilities: { canUpdateEmail: true, canRevokeSessions: true },
        total_count: mockUsers.length,
      },
    });
    listUsers.mockImplementation(async (query: any = {}) => {
      const users = mockUsers.filter(
        (user) => !query.role || user.role === query.role,
      );
      return {
        success: true,
        data: {
          users,
          capabilities: { canUpdateEmail: true, canRevokeSessions: true },
          total_count: users.length,
        },
      };
    });
  });

  it("renders created time in the shared relative format", async () => {
    const today = new Date().toISOString();
    (authService.listUsers as any).mockResolvedValueOnce({
      success: true,
      data: { users: [{ ...mockUsers[0], createdAt: today }] },
    });

    const { findByText } = renderWithProviders(AdminUsers);
    await findByText("alice");

    expect(document.querySelector(".admin-date")?.textContent?.trim()).toMatch(
      /^今天 /,
    );
    expect(document.querySelector(".admin-date")?.getAttribute("title")).toBe(
      today,
    );
  });
});

describe("AdminUsers.vue header actions", () => {
  it("keeps 刷新 in the page header and 返回文件 in the admin shell", async () => {
    const { findByRole, getByRole } = renderWithProviders(AdminUsers);
    await findByRole("heading", { name: "用户" });

    expect(getByRole("button", { name: /刷新/ })).toBeInTheDocument();
    expect(getByRole("link", { name: /返回文件/ })).toBeInTheDocument();
  });

  it("refreshes when 刷新 is clicked", async () => {
    const listUsers = vi.mocked(authService.listUsers);
    listUsers.mockClear();

    const { findByRole, getByRole } = renderWithProviders(AdminUsers);
    await findByRole("heading", { name: "用户" });
    await waitFor(() =>
      expect(listUsers).toHaveBeenCalledWith(
        expect.objectContaining({ page: 1, pageSize: 20 }),
      ),
    );
    const before = listUsers.mock.calls.length;

    await fireEvent.click(getByRole("button", { name: /刷新/ }));
    await waitFor(() =>
      expect(listUsers.mock.calls.length).toBeGreaterThan(before),
    );
  });

  it("resets a user password through the modal (r107')", async () => {
    renderWithProviders(AdminUsers);
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());
    // 稳式（#34 ✓ 行钮 admin-action / Modal 钮 is-primary 同文双匹配 ✗）
    await fireEvent.click(
      screen.getByText("重置密码", { selector: "button.admin-action" }),
    );
    const input = await screen.findByLabelText("新密码");
    await fireEvent.update(input, "abcdef123");
    const { authService } = await import("../src/services/auth.service");
    await fireEvent.click(
      screen.getByText("重置密码", { selector: "button.is-primary" }),
    );
    await waitFor(() =>
      expect(authService.resetUserPassword).toHaveBeenCalledWith(
        "u1",
        "abcdef123",
      ),
    );
  });

  it("deletes a user behind confirmation (r107')", async () => {
    renderWithProviders(AdminUsers);
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());
    await fireEvent.click(screen.getByText("删除"));
    const { authService } = await import("../src/services/auth.service");
    await waitFor(() =>
      expect(authService.deleteUser).toHaveBeenCalledWith("u1"),
    );
  });

  it("sorts users by column header (r118)", async () => {
    renderWithProviders(AdminUsers);
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());
    const header = screen.getByTitle("按用户名排序");
    expect(header.closest("th")).toHaveAttribute("aria-sort", "none");
    await fireEvent.click(header);
    expect(header.closest("th")).toHaveAttribute("aria-sort", "ascending");
    await fireEvent.click(header);
    expect(header.closest("th")).toHaveAttribute("aria-sort", "descending");
  });
});

describe("AdminUsers.vue user drawer", () => {
  it("opens a row and lists the account sessions", async () => {
    const listUserSessions = vi.fn(async () => [
      {
        id: "s1",
        issuedAt: "2026-01-01T00:00:00Z",
        lastSeenAt: "2026-01-01T01:00:00Z",
        expiresAt: "2026-02-01T00:00:00Z",
        revokedAt: null,
        userAgent: "Chrome",
        ipAddr: "127.0.0.1",
        credentialId: null,
        active: true,
      },
    ]);
    (authService as any).listUserSessions = listUserSessions;

    const { findByText, findByRole } = renderWithProviders(AdminUsers);
    await findByText("alice");
    await fireEvent.click(screen.getByText("alice"));

    await waitFor(() =>
      expect(listUserSessions).toHaveBeenCalledWith("u1"),
    );
    const drawer = await findByRole("dialog");
    expect(drawer.textContent).toContain("最近会话");
    await waitFor(() => expect(drawer.textContent).toContain("Chrome"));
    expect(drawer.textContent).toContain("127.0.0.1");
  });
});
