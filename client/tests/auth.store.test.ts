import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useAuthStore } from "../src/stores/auth.store";

const {
  registerMock,
  loginMock,
  meMock,
  bootstrapMock,
  logoutMock,
} = vi.hoisted(() => ({
  registerMock: vi.fn(async () => ({
    id: "u1",
    username: "alice",
    role: "user",
  })),
  loginMock: vi.fn(async () => ({
    user: { id: "u1", username: "alice", role: "user" },
    token: "session-token",
  })),
  meMock: vi.fn(),
  bootstrapMock: vi.fn(),
  logoutMock: vi.fn(),
}));

vi.mock("../src/services/auth.service", () => ({
  authService: {
    register: registerMock,
    login: loginMock,
    me: meMock,
    getSessionBootstrap: bootstrapMock,
    logout: logoutMock,
  },
}));

describe("auth store", () => {
  beforeEach(() => {
    registerMock.mockClear();
    loginMock.mockClear();
    meMock.mockReset();
    bootstrapMock.mockReset();
    bootstrapMock.mockResolvedValue(null);
    logoutMock.mockReset();
    logoutMock.mockResolvedValue({ success: true });
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


describe("auth store session scope", () => {
  it("stores the restricted scope returned by /auth/me", async () => {
    const store = useAuthStore();
    meMock.mockResolvedValue({
      success: true,
      data: {
        enabled: true,
        allowRegister: true,
        user: { id: "u1", username: "alice", role: "user" },
        scope: {
          restricted: true,
          label: "read-docs",
          roots: ["docs"],
          canWrite: false,
        },
      },
    });

    await store.fetchMe();

    expect(store.scope).toEqual({
      restricted: true,
      label: "read-docs",
      roots: ["docs"],
      canWrite: false,
    });
    expect(store.isRestricted).toBe(true);
    expect(store.scopeRoots).toEqual(["docs"]);
    expect(store.canWrite).toBe(false);
  });

  it("allows writes for a restricted read-write scope", async () => {
    const store = useAuthStore();
    meMock.mockResolvedValue({
      success: true,
      data: {
        enabled: true,
        allowRegister: true,
        user: { id: "u1", username: "alice", role: "user" },
        scope: {
          restricted: true,
          label: "write-docs",
          roots: ["docs", "images"],
          canWrite: true,
        },
      },
    });

    await store.fetchMe();

    expect(store.scopeRoots).toEqual(["docs", "images"]);
    expect(store.canWrite).toBe(true);
  });

  it("stores the scope returned by login and clears it on logout", async () => {
    const store = useAuthStore();
    loginMock.mockResolvedValue({
      user: { id: "u1", username: "alice", role: "admin" },
      token: "session-token",
      scope: {
        restricted: true,
        label: "read-docs",
        roots: ["docs"],
        canWrite: false,
      },
    } as never);

    await store.login("alice", "secret123");
    expect(store.isRestricted).toBe(true);
    expect(store.canWrite).toBe(false);

    await store.logout();
    expect(store.scope).toBeNull();
    expect(store.isRestricted).toBe(false);
    expect(store.scopeRoots).toEqual([]);
    expect(store.canWrite).toBe(true);
  });

  it("treats a full-scope session as unrestricted", async () => {
    const store = useAuthStore();
    meMock.mockResolvedValue({
      success: true,
      data: {
        enabled: true,
        allowRegister: true,
        user: { id: "u1", username: "alice", role: "admin" },
        scope: {
          restricted: false,
          label: null,
          roots: [],
          canWrite: true,
        },
      },
    });

    await store.fetchMe();

    expect(store.isRestricted).toBe(false);
    expect(store.scopeRoots).toEqual([]);
    expect(store.canWrite).toBe(true);
  });
});
