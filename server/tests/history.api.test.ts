/// @vitest-environment node
import { test, expect } from "vitest";
import { Hono } from "hono";
import { repoContextMiddleware } from "../src/middleware/repo-context.js";
import { createHistoryRoutes } from "../src/routes/history.routes.js";
import { config } from "../src/config.js";
import child_process from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

// helper: run git command in repo
function git(repoPath: string, args: string[]) {
  return child_process
    .execFileSync("git", args, {
      cwd: repoPath,
      encoding: "utf-8",
    })
    .toString();
}

// helper: init a worktree repo
async function initWorktreeRepo() {
  const tmp = await fs.mkdtemp(path.join(os.tmpdir(), "vfiles-repo-"));
  child_process.execFileSync("git", ["init"], { cwd: tmp, encoding: "utf-8" });
  // initial commit
  await fs.writeFile(path.join(tmp, "README.md"), "repo", "utf-8");
  child_process.execFileSync("git", ["add", "."], {
    cwd: tmp,
    encoding: "utf-8",
  });
  child_process.execFileSync(
    "git",
    [
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "-m",
      "init",
    ],
    { cwd: tmp, encoding: "utf-8" },
  );
  return tmp;
}

// helper: init a bare repo
async function initBareRepo() {
  const tmp = await fs.mkdtemp(path.join(os.tmpdir(), "vfiles-bare-"));
  child_process.execFileSync("git", ["init", "--bare"], {
    cwd: tmp,
    encoding: "utf-8",
  });
  return tmp;
}

async function makeTwoCommitsInWorkdir(workDir: string, filePath: string) {
  await fs.writeFile(path.join(workDir, filePath), "first content", "utf-8");
  child_process.execFileSync("git", ["add", filePath], {
    cwd: workDir,
    encoding: "utf-8",
  });
  child_process.execFileSync(
    "git",
    [
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "-m",
      "test commit 1",
    ],
    { cwd: workDir, encoding: "utf-8" },
  );
  const commit1 = child_process
    .execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: workDir,
      encoding: "utf-8",
    })
    .trim();

  await fs.writeFile(path.join(workDir, filePath), "second content", "utf-8");
  child_process.execFileSync("git", ["add", filePath], {
    cwd: workDir,
    encoding: "utf-8",
  });
  child_process.execFileSync(
    "git",
    [
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "-m",
      "test commit 2",
    ],
    { cwd: workDir, encoding: "utf-8" },
  );
  const commit2 = child_process
    .execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: workDir,
      encoding: "utf-8",
    })
    .trim();
  return { commit1, commit2 };
}

async function getHistoryViaStub(
  repoPath: string,
  isBare: boolean,
  filePath: string,
  limit = 50,
) {
  const RS = "\x1e";
  const US = "\x1f";
  const args = isBare
    ? [
        "--git-dir",
        repoPath,
        "log",
        `--pretty=format:%H${US}%P${US}%an${US}%ae${US}%at${US}%s${RS}`,
        "-n",
        String(limit),
        "--",
        filePath,
      ]
    : [
        "log",
        `--pretty=format:%H${US}%P${US}%an${US}%ae${US}%at${US}%s${RS}`,
        "-n",
        String(limit),
        "--",
        filePath,
      ];
  const raw = child_process.execFileSync("git", args, {
    cwd: isBare ? undefined : repoPath,
    encoding: "utf-8",
  });
  const records = raw
    .toString()
    .split(RS)
    .map((r) => r.trim())
    .filter(Boolean);
  const commitInfos: any[] = [];
  for (const record of records) {
    const [hash, parentsStr, authorName, authorEmail, timestampStr, message] =
      record.split(US);
    const timestamp = Number.parseInt(timestampStr, 10);
    commitInfos.push({
      hash,
      parents: parentsStr ? parentsStr.split(" ").filter(Boolean) : [],
      authorName,
      authorEmail,
      date: timestamp,
      message,
    });
  }
  const currentVersion = commitInfos[0]?.hash || "";
  return {
    commits: commitInfos,
    currentVersion,
    totalCommits: commitInfos.length,
  };
}

async function getDiffViaStub(
  repoPath: string,
  isBare: boolean,
  filePath: string,
  commit: string,
  parent?: string,
) {
  // determine parent if not provided
  let parentHash = parent;
  if (!parentHash) {
    try {
      const rev = child_process
        .execFileSync("git", ["rev-list", "--parents", "-n", "1", commit], {
          cwd: isBare ? undefined : repoPath,
          encoding: "utf-8",
        })
        .trim();
      const parts = rev.split(" ");
      parentHash = parts[1];
    } catch {
      parentHash = undefined;
    }
  }

  const args = isBare
    ? parentHash
      ? ["--git-dir", repoPath, "diff", parentHash, commit, "--", filePath]
      : ["--git-dir", repoPath, "show", `${commit}:${filePath}`]
    : parentHash
      ? ["diff", parentHash, commit, "--", filePath]
      : ["show", `${commit}:${filePath}`];

  const raw = child_process.execFileSync("git", args, {
    cwd: isBare ? undefined : repoPath,
    encoding: "utf-8",
  });
  return raw.toString();
}

// test for worktree mode
test("history works in worktree repo", async () => {
  const repo = await initWorktreeRepo();
  const filename = `test-history-${Date.now()}-${Math.random().toString(36).slice(2)}.txt`;
  const { commit1, commit2 } = await makeTwoCommitsInWorkdir(repo, filename);

  const gitManager = {
    get: async (rp: string, mode?: any) => ({
      getFileHistory: async (p: string, limit = 50) =>
        getHistoryViaStub(rp, false, p, limit),
      getFileDiff: async (p: string, commit: string, parent?: string) =>
        getDiffViaStub(rp, false, p, commit, parent),
    }),
  } as any;

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: repo, repoMode: "worktree" });
    return next();
  });
  app.route("/api/history", createHistoryRoutes(gitManager));

  const res = await app.fetch(
    new Request(
      `http://localhost/api/history?path=${encodeURIComponent(filename)}`,
    ),
  );
  expect(res.status).toBe(200);
  const body = await res.json();
  expect(body.success).toBe(true);
  const commits = body.data?.commits || [];
  expect(commits.length).toBeGreaterThanOrEqual(2);
  const hashes = commits.map((c: any) => c.hash);
  expect(hashes).toContain(commit2);
  expect(hashes).toContain(commit1);

  // check diff via API: diff commit2 vs commit1
  const diffRes = await app.fetch(
    new Request(
      `http://localhost/api/history/diff?path=${encodeURIComponent(filename)}&commit=${commit2}&parent=${commit1}`,
    ),
  );
  expect(diffRes.status).toBe(200);
  const diffText = await diffRes.text();
  expect(diffText).toContain("+second content");
  expect(diffText).toContain("-first content");

  // cleanup
  try {
    child_process.execFileSync("git", ["rm", "-f", filename], {
      cwd: repo,
      encoding: "utf-8",
    });
    child_process.execFileSync(
      "git",
      [
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "-m",
        "cleanup",
      ],
      { cwd: repo, encoding: "utf-8" },
    );
    await fs.rm(repo, { recursive: true, force: true });
  } catch {
    // ignore
  }
}, 20000);

// test for bare mode
test("history works in bare repo", async () => {
  const bare = await initBareRepo();
  // clone to workdir
  const work = await fs.mkdtemp(path.join(os.tmpdir(), "vfiles-clone-"));
  child_process.execFileSync("git", ["clone", bare, work], {
    encoding: "utf-8",
  });

  const filename = `test-history-${Date.now()}-${Math.random().toString(36).slice(2)}.txt`;
  const { commit1, commit2 } = await makeTwoCommitsInWorkdir(work, filename);
  // push back
  child_process.execFileSync("git", ["push", "origin", "HEAD"], {
    cwd: work,
    encoding: "utf-8",
  });

  const gitManager = {
    get: async (rp: string, mode?: any) => ({
      getFileHistory: async (p: string, limit = 50) =>
        getHistoryViaStub(rp, true, p, limit),
      getFileDiff: async (p: string, commit: string, parent?: string) =>
        getDiffViaStub(rp, true, p, commit, parent),
    }),
  } as any;

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: bare, repoMode: "bare" });
    return next();
  });
  app.route("/api/history", createHistoryRoutes(gitManager));

  const res = await app.fetch(
    new Request(
      `http://localhost/api/history?path=${encodeURIComponent(filename)}`,
    ),
  );
  expect(res.status).toBe(200);
  const body = await res.json();
  expect(body.success).toBe(true);
  const commits = body.data?.commits || [];
  expect(commits.length).toBeGreaterThanOrEqual(2);
  const hashes = commits.map((c: any) => c.hash);
  // commit hashes are in bare as well
  expect(hashes).toContain(commit2);
  expect(hashes).toContain(commit1);

  // check diff via API
  const diffRes = await app.fetch(
    new Request(
      `http://localhost/api/history/diff?path=${encodeURIComponent(filename)}&commit=${commit2}&parent=${commit1}`,
    ),
  );
  expect(diffRes.status).toBe(200);
  const diffText = await diffRes.text();
  expect(diffText).toContain("+second content");
  expect(diffText).toContain("-first content");

  // cleanup
  try {
    // remove via work dir
    child_process.execFileSync("git", ["rm", "-f", filename], {
      cwd: work,
      encoding: "utf-8",
    });
    child_process.execFileSync(
      "git",
      [
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "-m",
        "cleanup",
      ],
      { cwd: work, encoding: "utf-8" },
    );
    child_process.execFileSync("git", ["push", "origin", "HEAD"], {
      cwd: work,
      encoding: "utf-8",
    });
    await fs.rm(work, { recursive: true, force: true });
    await fs.rm(bare, { recursive: true, force: true });
  } catch {
    // ignore
  }
}, 20000);
