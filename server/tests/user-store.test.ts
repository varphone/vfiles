/// @vitest-environment node
import {
  UserStore,
  hashPassword,
  verifyPassword,
} from "../src/services/user-store.js";
import os from "node:os";
import path from "node:path";
import fs from "node:fs/promises";
import crypto from "node:crypto";

function tmpFile() {
  return path.join(os.tmpdir(), `vfiles-users-${crypto.randomUUID()}.json`);
}

describe("UserStore", () => {
  let file: string;
  let store: UserStore;

  beforeEach(async () => {
    file = tmpFile();
    store = new UserStore(file);
  });

  afterEach(async () => {
    try {
      await fs.unlink(file);
    } catch {
      // ignore
    }
  });

  it("create and fetch user by username/email", async () => {
    const u = await store.createUser({
      username: "alice",
      password: "pass1234",
      email: "alice@example.com",
    });
    expect(u.username).toBe("alice");

    const fromName = await store.getUserByUsername("alice");
    expect(fromName).toBeDefined();
    expect(fromName?.username).toBe("alice");

    const fromEmail = await store.getUserByEmail("alice@example.com");
    expect(fromEmail).toBeDefined();
    expect(fromEmail?.username).toBe("alice");
  });

  it("reject duplicate username/email", async () => {
    await store.createUser({
      username: "bob",
      password: "pass1234",
      email: "bob@example.com",
    });
    await expect(
      store.createUser({ username: "bob", password: "pass1234" }),
    ).rejects.toThrow(/用户名已存在/);
    await expect(
      store.createUser({
        username: "charlie",
        password: "pass1234",
        email: "bob@example.com",
      }),
    ).rejects.toThrow(/用户名已存在|邮箱已被使用/);
  });

  it("setUserEmail enforces uniqueness", async () => {
    const a = await store.createUser({
      username: "usera",
      password: "pass1234",
      email: "a@example.com",
    });
    const b = await store.createUser({
      username: "userb",
      password: "pass1234",
    });
    await expect(store.setUserEmail(b.id, "a@example.com")).rejects.toThrow(
      /邮箱已被使用/,
    );
  });

  it("setUserDisabled bumps sessionVersion", async () => {
    const u = await store.createUser({
      username: "userc",
      password: "pass1234",
    });
    const before = await store.getUserById(u.id);
    expect(before?.sessionVersion ?? 0).toBe(0);
    await store.setUserDisabled(u.id, true);
    const after = await store.getUserById(u.id);
    expect(after?.disabled).toBe(true);
    expect(after?.sessionVersion ?? 0).toBeGreaterThan(0);
  });

  it("resetPassword changes hash and bumps sessionVersion", async () => {
    const u = await store.createUser({
      username: "userd",
      password: "oldpass",
    });
    const stored = await store.getUserById(u.id);
    expect(stored).toBeDefined();
    const oldHash = stored!.passwordHash;
    await store.resetPassword(u.id, "newpass");
    const newStored = await store.getUserById(u.id);
    expect(newStored).toBeDefined();
    expect(newStored!.passwordHash).not.toBe(oldHash);
    expect(await verifyPassword("newpass", newStored!.passwordHash)).toBe(true);
    expect(newStored!.sessionVersion ?? 0).toBeGreaterThan(0);
  });
});
