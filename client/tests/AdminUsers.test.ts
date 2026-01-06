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

vi.mock("../src/services/auth.service", async () => {
  const actual = await vi.importActual("../src/services/auth.service");
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
  it("loads and displays users, saves email and revokes sessions", async () => {
    const { findByText, getByPlaceholderText, getAllByText } =
      renderWithProviders(AdminUsers);

    await findByText("用户管理");
    await findByText("alice");

    const emailInput = getByPlaceholderText(
      "user@example.com",
    ) as HTMLInputElement;
    expect(emailInput.value).toBe("alice@example.com");

    // change email and click 保存
    emailInput.value = "new@example.com";
    emailInput.dispatchEvent(new Event("input"));

    const saveButtons = getAllByText("保存");
    expect(saveButtons.length).toBeGreaterThan(0);

    await saveButtons[0].dispatchEvent(new MouseEvent("click"));

    // setUserEmail should be called with user id
    expect(
      (authService.setUserEmail as any).mock.calls.length,
    ).toBeGreaterThanOrEqual(1);
    const [calledId, calledEmail] = (authService.setUserEmail as any).mock
      .calls[0];
    expect(calledId).toBe("u1");
    // revoke sessions
    vi.stubGlobal("confirm", () => true);
    const revokeButtons = getAllByText("强制下线");
    await revokeButtons[0].dispatchEvent(new MouseEvent("click"));
    expect(
      (authService.revokeUserSessions as any).mock.calls.length,
    ).toBeGreaterThanOrEqual(1);
  });
});
