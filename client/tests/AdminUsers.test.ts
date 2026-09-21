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
    role: "user",
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
    const { findByText, findByPlaceholderText, getAllByText, getByLabelText } =
      renderWithProviders(AdminUsers);

    await findByText("用户管理");
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

  it("filters users by keyword and by role", async () => {
    const { findByText, getByLabelText } = renderWithProviders(AdminUsers);
    await findByText("alice");

    const search = getByLabelText("搜索用户名或邮箱");
    await fireEvent.update(search, "zzz");
    await waitFor(() =>
      expect(document.body.textContent).toContain("没有找到匹配的用户"),
    );

    await fireEvent.click(screen.getByText("清除筛选"));
    await waitFor(() => expect(screen.getByText("alice")).toBeInTheDocument());

    // 角色筛选 chips 只在存在多种角色时出现
    expect(document.querySelectorAll(".admin-filter").length).toBe(0);
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
  it("offers the same 刷新 / 返回文件 actions as 我的分享 and 审计日志", async () => {
    const { findByText, container } = renderWithProviders(AdminUsers);
    await findByText("用户管理");

    const actions = Array.from(
      container.querySelectorAll<HTMLButtonElement>(
        ".admin-header-actions button",
      ),
    ).map((button) => button.textContent?.trim());

    expect(actions).toContain("刷新");
    expect(actions).toContain("返回文件");
  });

  it("refreshes when 刷新 is clicked", async () => {
    const listUsers = vi.mocked(authService.listUsers);
    listUsers.mockClear();

    const { findByText, getByText } = renderWithProviders(AdminUsers);
    await findByText("用户管理");
    await waitFor(() => expect(listUsers).toHaveBeenCalledTimes(1));

    await fireEvent.click(getByText("刷新"));
    await waitFor(() => expect(listUsers).toHaveBeenCalledTimes(2));
  });
});
