/**
 * Unit tests for GitService concurrent access safety
 */
import { describe, test, expect, beforeEach, afterEach } from "vitest";
import { GitService } from "../src/services/git.service.js";
import fs from "node:fs/promises";
import path from "node:path";
import child_process from "node:child_process";
import os from "node:os";

const tmpBase = path.join(os.tmpdir(), "vfiles-git-test-");

// Helper to create a temporary git repo (worktree mode)
async function createTempWorktreeRepo(): Promise<string> {
  const dir = await fs.mkdtemp(tmpBase);
  child_process.execFileSync("git", ["init"], { cwd: dir });
  child_process.execFileSync(
    "git",
    [
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "commit",
      "--allow-empty",
      "-m",
      "init",
    ],
    { cwd: dir },
  );
  return dir;
}

// Helper to create a temporary bare git repo
async function createTempBareRepo(): Promise<string> {
  const dir = await fs.mkdtemp(tmpBase);
  child_process.execFileSync("git", ["init", "--bare"], { cwd: dir });
  // Bare repos need at least one object; create an empty commit via plumbing
  const treeHash = child_process
    .execFileSync("git", ["hash-object", "-t", "tree", "/dev/null"], {
      cwd: dir,
      encoding: "utf-8",
    })
    .trim();
  const commitHash = child_process
    .execFileSync(
      "git",
      [
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit-tree",
        treeHash,
        "-m",
        "init",
      ],
      { cwd: dir, encoding: "utf-8" },
    )
    .trim();
  child_process.execFileSync("git", ["update-ref", "HEAD", commitHash], {
    cwd: dir,
  });
  return dir;
}

// Helper to convert content to Buffer for comparison
function toStr(content: Buffer | string): string {
  return typeof content === "string" ? content : content.toString("utf-8");
}

describe("GitService worktree mode", () => {
  let repoDir: string;
  let gitService: GitService;

  beforeEach(async () => {
    repoDir = await createTempWorktreeRepo();
    gitService = new GitService(repoDir, "worktree");
  });

  afterEach(async () => {
    await fs.rm(repoDir, { recursive: true, force: true });
  });

  test("concurrent saveFile calls should all succeed without corruption", async () => {
    const numFiles = 5;
    const contents = Array.from(
      { length: numFiles },
      (_, i) => `content-${i}-${"X".repeat(100)}`,
    );

    // Save multiple files concurrently
    const promises = contents.map((content, i) =>
      gitService.saveFile(`concurrent-${i}.txt`, content, `commit file ${i}`),
    );

    const commits = await Promise.all(promises);

    // All commits should be valid SHA-1 hashes
    for (const commit of commits) {
      expect(commit).toMatch(/^[0-9a-f]{40}$/i);
    }

    // All commits should be unique (each file creates a new commit)
    const uniqueCommits = new Set(commits);
    expect(uniqueCommits.size).toBe(numFiles);

    // Verify all files exist with correct content
    for (let i = 0; i < numFiles; i++) {
      const filePath = path.join(repoDir, `concurrent-${i}.txt`);
      const savedContent = await fs.readFile(filePath, "utf-8");
      expect(savedContent).toBe(contents[i]);
    }
  }, 30000);

  test("sequential deleteFile calls should all succeed", async () => {
    const numFiles = 3;

    // First create and commit all files
    for (let i = 0; i < numFiles; i++) {
      await gitService.saveFile(
        `delete-${i}.txt`,
        `content-${i}`,
        `create file ${i}`,
      );
    }

    // Verify files exist
    for (let i = 0; i < numFiles; i++) {
      const exists = await fs
        .access(path.join(repoDir, `delete-${i}.txt`))
        .then(() => true)
        .catch(() => false);
      expect(exists).toBe(true);
    }

    // Delete files sequentially (deleteFile returns void, not commit hash)
    for (let i = 0; i < numFiles; i++) {
      await gitService.deleteFile(`delete-${i}.txt`, `delete file ${i}`);
    }

    // All files should be deleted
    for (let i = 0; i < numFiles; i++) {
      const exists = await fs
        .access(path.join(repoDir, `delete-${i}.txt`))
        .then(() => true)
        .catch(() => false);
      expect(exists).toBe(false);
    }

    // Verify git log has delete commits
    const log = child_process
      .execFileSync("git", ["log", "--oneline", "-n", numFiles], {
        cwd: repoDir,
        encoding: "utf-8",
      })
      .trim();
    expect(log.split("\n").length).toBe(numFiles);
  }, 30000);

  test("mixed sequential operations should all succeed", async () => {
    // Create some initial files
    await gitService.saveFile("file-a.txt", "content-a", "create a");
    await gitService.saveFile("file-b.txt", "content-b", "create b");

    // Perform operations sequentially:
    // - Create new file
    const commit1 = await gitService.saveFile(
      "file-new.txt",
      "new-content",
      "create new",
    );
    // - Update existing file
    const commit2 = await gitService.saveFile(
      "file-a.txt",
      "updated-content-a",
      "update a",
    );
    // - Delete existing file (returns void)
    await gitService.deleteFile("file-b.txt", "delete b");

    // saveFile operations should return valid commits
    expect(commit1).toMatch(/^[0-9a-f]{40}$/i);
    expect(commit2).toMatch(/^[0-9a-f]{40}$/i);

    // Verify final state
    const newContent = await fs.readFile(
      path.join(repoDir, "file-new.txt"),
      "utf-8",
    );
    expect(newContent).toBe("new-content");

    const updatedContent = await fs.readFile(
      path.join(repoDir, "file-a.txt"),
      "utf-8",
    );
    expect(updatedContent).toBe("updated-content-a");

    const bExists = await fs
      .access(path.join(repoDir, "file-b.txt"))
      .then(() => true)
      .catch(() => false);
    expect(bExists).toBe(false);
  }, 30000);
});

describe("GitService bare mode", () => {
  let repoDir: string;
  let gitService: GitService;

  beforeEach(async () => {
    repoDir = await createTempBareRepo();
    gitService = new GitService(repoDir, "bare");
  });

  afterEach(async () => {
    await fs.rm(repoDir, { recursive: true, force: true });
  });

  test("concurrent saveFile calls in bare mode should all succeed", async () => {
    const numFiles = 5;

    // Save multiple files concurrently using Buffer content
    const promises = Array.from({ length: numFiles }, (_, i) => {
      const content = Buffer.from(`bare-content-${i}-${"Y".repeat(50)}`);
      return gitService.saveFile(
        `bare-file-${i}.txt`,
        content,
        `commit bare file ${i}`,
      );
    });

    const commits = await Promise.all(promises);

    // All commits should be valid SHA-1 hashes
    for (const commit of commits) {
      expect(commit).toMatch(/^[0-9a-f]{40}$/i);
    }

    // All commits should be unique
    const uniqueCommits = new Set(commits);
    expect(uniqueCommits.size).toBe(numFiles);

    // Verify all files can be read back
    for (let i = 0; i < numFiles; i++) {
      const content = await gitService.getFileContent(`bare-file-${i}.txt`);
      expect(toStr(content as Buffer)).toBe(
        `bare-content-${i}-${"Y".repeat(50)}`,
      );
    }
  }, 30000);

  test("sequential deleteFile calls in bare mode should all succeed", async () => {
    const numFiles = 3;

    // First create all files
    for (let i = 0; i < numFiles; i++) {
      await gitService.saveFile(
        `bare-delete-${i}.txt`,
        Buffer.from(`content-${i}`),
        `create file ${i}`,
      );
    }

    // Delete files sequentially (deleteFile returns void)
    for (let i = 0; i < numFiles; i++) {
      await gitService.deleteFile(`bare-delete-${i}.txt`, `delete file ${i}`);
    }

    // All files should be gone (getFileContent should throw or return null)
    for (let i = 0; i < numFiles; i++) {
      try {
        const content = await gitService.getFileContent(`bare-delete-${i}.txt`);
        // If no throw, content should be empty or undefined
        expect(content).toBeFalsy();
      } catch {
        // Expected - file doesn't exist
      }
    }
  }, 30000);

  test("stream saveFile in bare mode should work correctly", async () => {
    // Test with a ReadableStream
    const content = "streamed-content-for-bare-mode";
    const encoder = new TextEncoder();
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(encoder.encode(content));
        controller.close();
      },
    });

    const commit = await gitService.saveFile(
      "stream-test.txt",
      stream,
      "stream commit",
    );
    expect(commit).toMatch(/^[0-9a-f]{40}$/i);

    const readBack = await gitService.getFileContent("stream-test.txt");
    expect(toStr(readBack as Buffer)).toBe(content);
  }, 15000);
});

describe("GitService read operations during writes", () => {
  let repoDir: string;
  let gitService: GitService;

  beforeEach(async () => {
    repoDir = await createTempWorktreeRepo();
    gitService = new GitService(repoDir, "worktree");
    // Create some initial files
    await gitService.saveFile("existing.txt", "initial content", "init");
  });

  afterEach(async () => {
    await fs.rm(repoDir, { recursive: true, force: true });
  });

  test("read operations should not be blocked by write operations", async () => {
    // Start a write operation
    const writePromise = gitService.saveFile(
      "new-file.txt",
      "X".repeat(1000),
      "write",
    );

    // Immediately try to read existing file
    const readPromise = gitService.getFileContent("existing.txt");

    // Both should succeed
    const [commit, content] = await Promise.all([writePromise, readPromise]);

    expect(commit).toMatch(/^[0-9a-f]{40}$/i);
    expect(toStr(content as Buffer)).toBe("initial content");
  }, 15000);

  test("listFiles should work during concurrent writes", async () => {
    // Create some files
    await gitService.saveFile("list-test-1.txt", "content1", "create 1");
    await gitService.saveFile("list-test-2.txt", "content2", "create 2");

    // Start write operations
    const writes = [
      gitService.saveFile("list-test-3.txt", "content3", "create 3"),
      gitService.saveFile("list-test-4.txt", "content4", "create 4"),
    ];

    // List files during writes
    const listPromise = gitService.listFiles("/");

    const [, , files] = await Promise.all([...writes, listPromise]);

    // Should have at least the files that existed before the writes started
    const names = files.map((f) => f.name);
    expect(names).toContain("existing.txt");
    expect(names).toContain("list-test-1.txt");
    expect(names).toContain("list-test-2.txt");
  }, 15000);
});
