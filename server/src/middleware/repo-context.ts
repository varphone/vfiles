import type { Context, Next } from "hono";
import path from "node:path";
import { config } from "../config.js";

export interface RepoContext {
  repoPath: string;
  repoMode: "worktree" | "bare";
}

function repoPathForUser(username: string, mode: "worktree" | "bare"): string {
  const baseDir = config.multiUser.baseDir;
  const name = username;
  if (mode === "bare") {
    return path.resolve(baseDir, `${name}.git`);
  }
  return path.resolve(baseDir, name);
}

export function repoContextMiddleware() {
  return async (c: Context, next: Next) => {
    const mode = config.repoMode;

    let repoPath = config.repoPath;

    if (config.auth.enabled && config.multiUser.enabled) {
      const user = (c as any).get("authUser") as
        | { username: string }
        | undefined;
      if (user?.username) {
        repoPath = repoPathForUser(user.username, mode);
      }
    }

    (c as any).set("repoContext", {
      repoPath,
      repoMode: mode,
    } satisfies RepoContext);
    return next();
  };
}

export function getRepoContext(c: Context): RepoContext {
  const ctx = (c as any).get("repoContext") as RepoContext | undefined;
  if (ctx?.repoPath && ctx?.repoMode) return ctx;
  return { repoPath: config.repoPath, repoMode: config.repoMode };
}
