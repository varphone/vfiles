import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { StoredUser, UserRole, UserStoreData } from "../types/auth.js";

function safeNowIso(): string {
  return new Date().toISOString();
}

function isValidUsername(username: string): boolean {
  return /^[a-zA-Z0-9][a-zA-Z0-9_-]{2,31}$/.test(username);
}

async function pathExists(p: string): Promise<boolean> {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

async function ensureDir(p: string): Promise<void> {
  await fs.mkdir(p, { recursive: true });
}

async function atomicWriteJson(filePath: string, data: unknown): Promise<void> {
  const dir = path.dirname(filePath);
  await ensureDir(dir);
  const tmp = path.join(dir, `.tmp-${crypto.randomUUID()}.json`);
  await fs.writeFile(tmp, JSON.stringify(data, null, 2), "utf-8");
  await fs.rename(tmp, filePath);
}

async function scryptAsync(password: string, salt: Buffer): Promise<Buffer> {
  return await new Promise((resolve, reject) => {
    crypto.scrypt(password, salt, 32, (err, derivedKey) => {
      if (err) reject(err);
      else resolve(derivedKey as Buffer);
    });
  });
}

export async function hashPassword(password: string): Promise<string> {
  const salt = crypto.randomBytes(16);
  const dk = await scryptAsync(password, salt);
  return `scrypt$${salt.toString("base64")}$${dk.toString("base64")}`;
}

export async function verifyPassword(
  password: string,
  stored: string,
): Promise<boolean> {
  const parts = stored.split("$");
  if (parts.length !== 3) return false;
  const [algo, saltB64, hashB64] = parts;
  if (algo !== "scrypt") return false;
  const salt = Buffer.from(saltB64, "base64");
  const expected = Buffer.from(hashB64, "base64");
  const dk = await scryptAsync(password, salt);
  if (dk.length !== expected.length) return false;
  return crypto.timingSafeEqual(dk, expected);
}

export class UserStore {
  private filePath: string;
  private writeChain: Promise<unknown> = Promise.resolve();

  constructor(filePath: string) {
    this.filePath = path.resolve(filePath);
  }

  private async loadUnsafe(): Promise<UserStoreData> {
    if (!(await pathExists(this.filePath))) {
      return { version: 1, users: [] };
    }
    const raw = await fs.readFile(this.filePath, "utf-8");
    const parsed = JSON.parse(raw) as UserStoreData;
    if (!parsed || parsed.version !== 1 || !Array.isArray(parsed.users)) {
      return { version: 1, users: [] };
    }
    return parsed;
  }

  private async saveUnsafe(data: UserStoreData): Promise<void> {
    await atomicWriteJson(this.filePath, data);
  }

  async listUsers(): Promise<Omit<StoredUser, "passwordHash">[]> {
    const data = await this.loadUnsafe();
    return data.users.map(({ passwordHash: _ph, sessionVersion: _sv, ...rest }) => rest);
  }

  async getUserById(id: string): Promise<StoredUser | undefined> {
    const data = await this.loadUnsafe();
    return data.users.find((u) => u.id === id);
  }

  async getUserByUsername(username: string): Promise<StoredUser | undefined> {
    const data = await this.loadUnsafe();
    return data.users.find((u) => u.username === username);
  }

  async createUser(input: {
    username: string;
    password: string;
    role?: UserRole;
  }): Promise<Omit<StoredUser, "passwordHash">> {
    const username = input.username.trim();
    if (!isValidUsername(username)) {
      throw new Error(
        "用户名格式无效（3-32 位，仅字母数字、-、_，且需以字母/数字开头）",
      );
    }
    if (input.password.trim().length < 6) {
      throw new Error("密码长度至少 6 位");
    }

    const role: UserRole = input.role ?? "user";

    const createdAt = safeNowIso();
    const user: StoredUser = {
      id: crypto.randomUUID(),
      username,
      role,
      createdAt,
      passwordHash: await hashPassword(input.password),
      sessionVersion: 0,
    };

    await (this.writeChain = this.writeChain.then(async () => {
      const data = await this.loadUnsafe();
      if (data.users.some((u) => u.username === username)) {
        throw new Error("用户名已存在");
      }
      // 第一个用户自动成为 admin，避免“没有管理员”
      if (data.users.length === 0) {
        user.role = "admin";
      }
      data.users.push(user);
      await this.saveUnsafe(data);
    }));

    // return without passwordHash
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { passwordHash, ...pub } = user;
    return pub;
  }

  async setUserRole(userId: string, role: UserRole): Promise<void> {
    await (this.writeChain = this.writeChain.then(async () => {
      const data = await this.loadUnsafe();
      const user = data.users.find((u) => u.id === userId);
      if (!user) throw new Error("用户不存在");
      user.role = role;
      await this.saveUnsafe(data);
    }));
  }

  async setUserDisabled(userId: string, disabled: boolean): Promise<void> {
    await (this.writeChain = this.writeChain.then(async () => {
      const data = await this.loadUnsafe();
      const user = data.users.find((u) => u.id === userId);
      if (!user) throw new Error("用户不存在");
      user.disabled = disabled;
      // 变更禁用状态时，顺便撤销历史会话
      user.sessionVersion = (user.sessionVersion ?? 0) + 1;
      await this.saveUnsafe(data);
    }));
  }

  async bumpSessionVersion(userId: string): Promise<void> {
    await (this.writeChain = this.writeChain.then(async () => {
      const data = await this.loadUnsafe();
      const user = data.users.find((u) => u.id === userId);
      if (!user) throw new Error("用户不存在");
      user.sessionVersion = (user.sessionVersion ?? 0) + 1;
      await this.saveUnsafe(data);
    }));
  }
}
