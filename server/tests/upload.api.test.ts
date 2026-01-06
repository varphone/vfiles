/// @vitest-environment node
import { test, expect } from "vitest";
import { Hono } from "hono";
import { createFilesRoutes } from "../src/routes/files.routes.js";
import { GitServiceManager } from "../src/services/git-service-manager.js";
import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import child_process from "node:child_process";

// helper: run git command in repo
function git(repoPath: string, args: string[]) {
  return child_process
    .execFileSync("git", args, {
      cwd: repoPath,
      encoding: "utf-8",
    })
    .toString()
    .trim();
}

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

function makeStubGitManager(repoPath: string) {
  return {
    get: async (rp: string, mode?: any) => ({
      saveFile: async (filePath: string, content: any, message: string) => {
        const fullPath = path.join(repoPath, filePath);
        await fs.mkdir(path.dirname(fullPath), { recursive: true });
        // handle various content shapes
        if (typeof content === "string") {
          await fs.writeFile(fullPath, content, "utf-8");
        } else if (Buffer.isBuffer(content)) {
          await fs.writeFile(fullPath, content);
        } else if (typeof (content as any).arrayBuffer === "function") {
          const buf = Buffer.from(await (content as any).arrayBuffer());
          await fs.writeFile(fullPath, buf);
        } else if (typeof (content as any).stream === "function") {
          const chunks: Buffer[] = [];
          for await (const ch of (content as any).stream()) {
            chunks.push(Buffer.from(ch));
          }
          await fs.writeFile(fullPath, Buffer.concat(chunks));
        } else {
          await fs.writeFile(fullPath, "");
        }
        child_process.execFileSync("git", ["add", filePath], {
          cwd: repoPath,
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
            message,
          ],
          { cwd: repoPath, encoding: "utf-8" },
        );
        return child_process
          .execFileSync("git", ["rev-parse", "HEAD"], {
            cwd: repoPath,
            encoding: "utf-8",
          })
          .toString()
          .trim();
      },
      commitFile: async (filePath: string, message: string) => {
        child_process.execFileSync("git", ["add", filePath], {
          cwd: repoPath,
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
            message,
          ],
          { cwd: repoPath, encoding: "utf-8" },
        );
        return child_process
          .execFileSync("git", ["rev-parse", "HEAD"], {
            cwd: repoPath,
            encoding: "utf-8",
          })
          .toString()
          .trim();
      },
    }),
  } as any;
}

// simple regex to validate commit sha
const shaRegex = /^[0-9a-f]{40}$/i;

// test single-file upload
test("POST /api/files/upload returns commit SHA and commit exists", async () => {
  const repo = await initWorktreeRepo();

  const gitManager = makeStubGitManager(repo);

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: repo, repoMode: "worktree" });
    return next();
  });
  app.route("/api/files", createFilesRoutes(gitManager as any));

  // Create a File-like object using global File (available in Node 18+); fall back to Blob if not available
  const content = "hello single upload";
  let file: any;
  try {
    file = new File([content], "hello.txt", { type: "text/plain" });
  } catch (e) {
    // Node may not have File; construct a simple object with required fields for Hono's formData parsing
    file = {
      name: "hello.txt",
      size: Buffer.byteLength(content, "utf-8"),
      stream: async function* () {
        yield Buffer.from(content, "utf-8");
      },
    } as any;
  }

  const form = new FormData();
  form.set("file", file as any);

  const req = new Request("http://localhost/api/files/upload", {
    method: "POST",
    body: form as any,
  });

  const res = await app.fetch(req);
  expect(res.status).toBe(200);
  const body = await res.json();
  expect(body.success).toBe(true);
  const commit = body.data?.commit;
  expect(typeof commit).toBe("string");
  expect(shaRegex.test(commit)).toBe(true);

  // verify commit exists in repo (HEAD should equal commit)
  const head = git(repo, ["rev-parse", "HEAD"]);
  expect(head).toBe(commit);

  // cleanup
  try {
    child_process.execFileSync("git", ["rm", "-f", "hello.txt"], {
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

// test chunked upload complete
test("POST /api/files/upload/init -> upload/chunk -> upload/complete returns commit SHA", async () => {
  const repo = await initWorktreeRepo();

  const gitManager = makeStubGitManager(repo);

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: repo, repoMode: "worktree" });
    return next();
  });
  app.route("/api/files", createFilesRoutes(gitManager as any));

  const filename = "chunked.txt";
  const content = "chunked content here";
  const size = Buffer.byteLength(content, "utf-8");

  // init
  const initReq = new Request("http://localhost/api/files/upload/init", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      filename,
      path: "",
      size,
      lastModified: Date.now(),
      mime: "text/plain",
    }),
  });
  const initRes = await app.fetch(initReq);
  expect(initRes.status).toBe(200);
  const initBody = await initRes.json();
  expect(initBody.success).toBe(true);
  const uploadId = initBody.data.uploadId as string;
  expect(/^[0-9a-f]{64}$/i.test(uploadId)).toBe(true);
  const totalChunks = initBody.data.totalChunks as number;
  expect(totalChunks).toBeGreaterThanOrEqual(1);

  // upload chunk 0
  const chunkReq = new Request(
    `http://localhost/api/files/upload/chunk?uploadId=${uploadId}&index=0`,
    {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: Buffer.from(content, "utf-8"),
    },
  );
  const chunkRes = await app.fetch(chunkReq);
  expect(chunkRes.status).toBe(200);
  const chunkBody = await chunkRes.json();
  expect(chunkBody.success).toBe(true);

  // complete
  const completeReq = new Request(
    "http://localhost/api/files/upload/complete",
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ uploadId }),
    },
  );
  const completeRes = await app.fetch(completeReq);
  expect(completeRes.status).toBe(200);
  const completeBody = await completeRes.json();
  expect(completeBody.success).toBe(true);
  const commit = completeBody.data?.commit as string;
  expect(typeof commit).toBe("string");
  expect(shaRegex.test(commit)).toBe(true);

  // verify commit exists (HEAD == commit)
  const head = git(repo, ["rev-parse", "HEAD"]);
  expect(head).toBe(commit);

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
