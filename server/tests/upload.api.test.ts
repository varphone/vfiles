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

// test concurrent chunk uploads to the same session
test("concurrent chunk uploads to same session should not corrupt data", async () => {
  const repo = await initWorktreeRepo();
  const gitManager = makeStubGitManager(repo);

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: repo, repoMode: "worktree" });
    return next();
  });
  app.route("/api/files", createFilesRoutes(gitManager as any));

  const filename = "concurrent-chunks.txt";

  // init session to get server-side chunk size
  const initRes = await app.fetch(
    new Request("http://localhost/api/files/upload/init", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        filename,
        path: "",
        size: 50 * 1024 * 1024, // 50MB - will result in multiple chunks
        lastModified: Date.now(),
      }),
    }),
  );
  const initBody = await initRes.json();
  expect(initBody.success).toBe(true);
  const uploadId = initBody.data.uploadId as string;
  const chunkSize = initBody.data.chunkSize as number;
  const totalChunks = initBody.data.totalChunks as number;

  // Create correctly sized chunk content
  const chunkContent = Buffer.alloc(chunkSize);
  chunkContent.fill("X");

  // Upload all chunks concurrently (simulating parallel requests)
  const chunkPromises = Array.from({ length: totalChunks }, (_, i) =>
    app.fetch(
      new Request(
        `http://localhost/api/files/upload/chunk?uploadId=${uploadId}&index=${i}`,
        {
          method: "POST",
          headers: { "content-type": "application/octet-stream" },
          body: chunkContent,
        },
      ),
    ),
  );

  const chunkResults = await Promise.all(chunkPromises);

  // All chunk uploads should succeed
  for (let i = 0; i < chunkResults.length; i++) {
    const res = chunkResults[i];
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
  }

  // Complete should succeed
  const completeRes = await app.fetch(
    new Request("http://localhost/api/files/upload/complete", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ uploadId }),
    }),
  );
  expect(completeRes.status).toBe(200);
  const completeBody = await completeRes.json();
  expect(completeBody.success).toBe(true);
  expect(shaRegex.test(completeBody.data?.commit)).toBe(true);

  // Verify file exists (content may differ in last chunk)
  const savedContent = await fs.readFile(path.join(repo, filename));
  expect(savedContent.length).toBe(50 * 1024 * 1024);

  // cleanup
  await fs.rm(repo, { recursive: true, force: true });
}, 60000);

// test concurrent uploads of different files
test("concurrent uploads of different files should all succeed", async () => {
  const repo = await initWorktreeRepo();
  const gitManager = makeStubGitManager(repo);

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: repo, repoMode: "worktree" });
    return next();
  });
  app.route("/api/files", createFilesRoutes(gitManager as any));

  const numFiles = 5;
  const files = Array.from({ length: numFiles }, (_, i) => ({
    filename: `concurrent-file-${i}.txt`,
    content: `Content of file ${i} - ${"Y".repeat(100)}`,
  }));

  // Initialize all upload sessions concurrently
  const initPromises = files.map((f) =>
    app.fetch(
      new Request("http://localhost/api/files/upload/init", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          filename: f.filename,
          path: "",
          size: Buffer.byteLength(f.content, "utf-8"),
          lastModified: Date.now() + Math.random(),
        }),
      }),
    ),
  );

  const initResults = await Promise.all(initPromises);
  const uploadIds: string[] = [];

  for (let i = 0; i < initResults.length; i++) {
    const res = initResults[i];
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
    uploadIds.push(body.data.uploadId);
  }

  // Upload chunks for all files concurrently
  const chunkPromises = files.map((f, i) =>
    app.fetch(
      new Request(
        `http://localhost/api/files/upload/chunk?uploadId=${uploadIds[i]}&index=0`,
        {
          method: "POST",
          headers: { "content-type": "application/octet-stream" },
          body: Buffer.from(f.content, "utf-8"),
        },
      ),
    ),
  );

  const chunkResults = await Promise.all(chunkPromises);
  for (const res of chunkResults) {
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
  }

  // Complete uploads - these need to run serially due to git lock
  // This tests that the server properly serializes git operations
  const commits: string[] = [];
  for (const uploadId of uploadIds) {
    const res = await app.fetch(
      new Request("http://localhost/api/files/upload/complete", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ uploadId }),
      }),
    );
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
    expect(shaRegex.test(body.data?.commit)).toBe(true);
    commits.push(body.data.commit);
  }

  // All commits should be different
  const uniqueCommits = new Set(commits);
  expect(uniqueCommits.size).toBe(numFiles);

  // Verify all files exist with correct content
  for (const f of files) {
    const savedContent = await fs.readFile(
      path.join(repo, f.filename),
      "utf-8",
    );
    expect(savedContent).toBe(f.content);
  }

  // cleanup
  await fs.rm(repo, { recursive: true, force: true });
}, 30000);

// test resume upload after partial upload
test("resume upload should correctly identify received chunks", async () => {
  const repo = await initWorktreeRepo();
  const gitManager = makeStubGitManager(repo);

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: repo, repoMode: "worktree" });
    return next();
  });
  app.route("/api/files", createFilesRoutes(gitManager as any));

  const filename = "resume-test.txt";
  // Use 25MB file to get 5 chunks (with default 5MB chunk size)
  const totalSize = 25 * 1024 * 1024;

  // First init
  const initRes1 = await app.fetch(
    new Request("http://localhost/api/files/upload/init", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        filename,
        path: "",
        size: totalSize,
        lastModified: 12345,
      }),
    }),
  );
  const initBody1 = await initRes1.json();
  expect(initBody1.success).toBe(true);
  const uploadId = initBody1.data.uploadId;
  const chunkSize = initBody1.data.chunkSize as number;
  const totalChunks = initBody1.data.totalChunks as number;
  expect(totalChunks).toBe(5);

  // Create chunk content
  const chunkContent = Buffer.alloc(chunkSize);
  chunkContent.fill("Z");

  // Upload only chunks 0, 2, 4 (skip 1 and 3)
  const uploadedIndices = [0, 2, 4];
  for (const i of uploadedIndices) {
    const res = await app.fetch(
      new Request(
        `http://localhost/api/files/upload/chunk?uploadId=${uploadId}&index=${i}`,
        {
          method: "POST",
          headers: { "content-type": "application/octet-stream" },
          body: chunkContent,
        },
      ),
    );
    expect(res.status).toBe(200);
  }

  // Re-init (resume) - should return received chunks
  const initRes2 = await app.fetch(
    new Request("http://localhost/api/files/upload/init", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        filename,
        path: "",
        size: totalSize,
        lastModified: 12345, // same lastModified = same uploadId
      }),
    }),
  );
  const initBody2 = await initRes2.json();
  expect(initBody2.success).toBe(true);
  expect(initBody2.data.uploadId).toBe(uploadId);
  expect(initBody2.data.resumable).toBe(true);
  expect(initBody2.data.received.sort()).toEqual(uploadedIndices.sort());

  // Upload missing chunks 1 and 3
  for (const i of [1, 3]) {
    const res = await app.fetch(
      new Request(
        `http://localhost/api/files/upload/chunk?uploadId=${uploadId}&index=${i}`,
        {
          method: "POST",
          headers: { "content-type": "application/octet-stream" },
          body: chunkContent,
        },
      ),
    );
    expect(res.status).toBe(200);
  }

  // Complete should now succeed
  const completeRes = await app.fetch(
    new Request("http://localhost/api/files/upload/complete", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ uploadId }),
    }),
  );
  expect(completeRes.status).toBe(200);
  const completeBody = await completeRes.json();
  expect(completeBody.success).toBe(true);

  // cleanup
  await fs.rm(repo, { recursive: true, force: true });
}, 60000);

// test duplicate chunk upload (idempotency)
test("duplicate chunk upload should be idempotent", async () => {
  const repo = await initWorktreeRepo();
  const gitManager = makeStubGitManager(repo);

  const app = new Hono();
  app.use("/api/*", (c, next) => {
    (c as any).set("repoContext", { repoPath: repo, repoMode: "worktree" });
    return next();
  });
  app.route("/api/files", createFilesRoutes(gitManager as any));

  const filename = "idempotent.txt";
  const content = "idempotent content";
  const size = Buffer.byteLength(content, "utf-8");

  // init
  const initRes = await app.fetch(
    new Request("http://localhost/api/files/upload/init", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ filename, path: "", size, lastModified: 1 }),
    }),
  );
  const uploadId = (await initRes.json()).data.uploadId;

  // Upload same chunk multiple times concurrently
  const duplicateUploads = Array.from({ length: 5 }, () =>
    app.fetch(
      new Request(
        `http://localhost/api/files/upload/chunk?uploadId=${uploadId}&index=0`,
        {
          method: "POST",
          headers: { "content-type": "application/octet-stream" },
          body: Buffer.from(content, "utf-8"),
        },
      ),
    ),
  );

  const results = await Promise.all(duplicateUploads);

  // All should succeed
  for (const res of results) {
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
  }

  // Complete should succeed and file should have correct content
  const completeRes = await app.fetch(
    new Request("http://localhost/api/files/upload/complete", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ uploadId }),
    }),
  );
  expect(completeRes.status).toBe(200);

  const savedContent = await fs.readFile(path.join(repo, filename), "utf-8");
  expect(savedContent).toBe(content);

  // cleanup
  await fs.rm(repo, { recursive: true, force: true });
}, 30000);
