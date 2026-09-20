import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useAuthStore } from "../src/stores/auth.store";

const { registerMock, loginMock } = vi.hoisted(() => ({
  registerMock: vi.fn(async () => ({ id: "u1", username: "alice", role: "user" })),
  loginMock: vi.fn(async () => ({
    user: { id: "u1", username: "alice", role: "user" },
    token: "session-token",
  })),
}));

vi.mock("../src/services/auth.service", () => ({
  authService: {
    register: registerMock,
    login: loginMock,
    me: vi.fn(),
    getSessionBootstrap: vi.fn(),
    logout: vi.fn(),
  },
}));

describe("auth store", () => {
  beforeEach(() => {
    registerMock.mockClear();
    loginMock.mockClear();
    setActivePinia(createPinia());
  });

  it("logs in immediately after successful registration", async () => {
    const store = useAuthStore();

    await store.register("alice", "secret123");

    expect(registerMock).toHaveBeenCalledWith({
      username: "alice",
      password: "secret123",
      email: undefined,
    });
    expect(loginMock).toHaveBeenCalledWith({
      username: "alice",
      password: "secret123",
    });
    expect(store.user).toEqual({
      id: "u1",
      username: "alice",
      role: "user",
    });
    expect(store.initialized).toBe(true);
  });
});
