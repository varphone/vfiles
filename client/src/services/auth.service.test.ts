import { beforeEach, describe, expect, it, vi } from "vitest";

const { getMock, postMock, putMock } = vi.hoisted(() => ({
  getMock: vi.fn(),
  postMock: vi.fn(),
  putMock: vi.fn(),
}));

vi.mock("./api.service", () => ({
  apiService: {
    get: getMock,
    post: postMock,
    put: putMock,
  },
}));

import { authService } from "./auth.service";

describe("authService admin compatibility", () => {
  beforeEach(() => {
    getMock.mockReset();
    postMock.mockReset();
    putMock.mockReset();
  });

  it("normalizes the Rust admin users response", async () => {
    getMock.mockResolvedValue({
      users: [
        {
          id: "u1",
          username: "alice",
          email: "alice@example.com",
          role: "user",
          disabled: false,
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
      total_count: 1,
      page: 1,
      page_size: 20,
    });

    const result = await authService.listUsers();

    expect(getMock).toHaveBeenCalledWith("/admin/users");
    expect(result).toEqual({
      success: true,
      data: {
        users: [
          {
            id: "u1",
            username: "alice",
            email: "alice@example.com",
            role: "user",
            disabled: false,
            createdAt: "2026-01-01T00:00:00Z",
          },
        ],
        capabilities: {
          canUpdateEmail: true,
          canRevokeSessions: true,
        },
      },
    });
  });

  it("falls back to the legacy admin users endpoint on 404", async () => {
    getMock.mockRejectedValueOnce({ status: 404 }).mockResolvedValueOnce({
      success: true,
      data: {
        users: [
          {
            id: "u2",
            username: "bob",
            email: "bob@example.com",
            role: "admin",
            disabled: false,
            createdAt: "2026-01-02T00:00:00Z",
          },
        ],
      },
    });

    const result = await authService.listUsers();

    expect(getMock).toHaveBeenNthCalledWith(1, "/admin/users");
    expect(getMock).toHaveBeenNthCalledWith(2, "/auth/users");
    expect(result).toEqual({
      success: true,
      data: {
        users: [
          {
            id: "u2",
            username: "bob",
            email: "bob@example.com",
            role: "admin",
            disabled: false,
            createdAt: "2026-01-02T00:00:00Z",
          },
        ],
        capabilities: {
          canUpdateEmail: true,
          canRevokeSessions: true,
        },
      },
    });
  });

  it("uses the Rust admin update endpoint for role changes", async () => {
    putMock.mockResolvedValue(undefined);

    const result = await authService.setUserRole("user-1", "admin");

    expect(putMock).toHaveBeenCalledWith("/admin/users/user-1", {
      role: "admin",
    });
    expect(result).toEqual({ success: true });
  });

  it("uses the Rust admin update endpoint for email changes", async () => {
    putMock.mockResolvedValue(undefined);

    const result = await authService.setUserEmail("user-1", "new@example.com");

    expect(putMock).toHaveBeenCalledWith("/admin/users/user-1", {
      email: "new@example.com",
    });
    expect(result).toEqual({ success: true });
  });

  it("uses the Rust admin route for revoking user sessions", async () => {
    postMock.mockResolvedValue(undefined);

    const result = await authService.revokeUserSessions("user-1");

    expect(postMock).toHaveBeenCalledWith(
      "/admin/users/user-1/revoke-sessions",
    );
    expect(result).toEqual({
      success: true,
    });
  });

  it("normalizes the Rust session bootstrap response", async () => {
    getMock.mockResolvedValue({
      auth_enabled: true,
      current_user: "u1",
      capabilities: ["upload", "view_history"],
      active_workspace: "ns-1",
      features: {
        auth_enabled: true,
        multi_user: true,
        email_login: false,
        search_content: true,
        share_enabled: true,
        history_enabled: true,
      },
    });

    const result = await authService.getSessionBootstrap();

    expect(getMock).toHaveBeenCalledWith("/session/bootstrap");
    expect(result).toEqual({
      authEnabled: true,
      currentUser: "u1",
      capabilities: ["upload", "view_history"],
      activeWorkspace: "ns-1",
      features: {
        authEnabled: true,
        multiUser: true,
        emailLogin: false,
        searchContent: true,
        shareEnabled: true,
        historyEnabled: true,
      },
    });
  });
});
