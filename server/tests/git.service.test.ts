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

// 辅助函数：计算 SHA256
async function computeSha256(content: Buffer): Promise<string> {
  const hasher = new Bun.CryptoHasher("sha256");
  hasher.update(content);
  return hasher.digest("hex");
}

// 辅助函数：验证 LFS 指针格式
function isLfsPointer(content: string): boolean {
  return content.startsWith("version https://git-lfs.github.com/spec/v1");
}

// 辅助函数：从 LFS 指针中提取 OID 和大小
function parseLfsPointer(
  content: string,
): { oid: string; size: number } | null {
  const oidMatch = content.match(/oid sha256:([a-f0-9]{64})/);
  const sizeMatch = content.match(/size (\d+)/);
  if (oidMatch && sizeMatch) {
    return { oid: oidMatch[1], size: parseInt(sizeMatch[1], 10) };
  }
  return null;
}

// 辅助函数：验证 LFS 对象文件是否存在
async function lfsObjectExists(repoDir: string, oid: string): Promise<boolean> {
  const lfsPath = path.join(
    repoDir,
    "lfs",
    "objects",
    oid.slice(0, 2),
    oid.slice(2, 4),
    oid,
  );
  try {
    await fs.stat(lfsPath);
    return true;
  } catch {
    return false;
  }
}

// 辅助函数：读取 LFS 对象文件内容
async function readLfsObject(repoDir: string, oid: string): Promise<Buffer> {
  const lfsPath = path.join(
    repoDir,
    "lfs",
    "objects",
    oid.slice(0, 2),
    oid.slice(2, 4),
    oid,
  );
  return await fs.readFile(lfsPath);
}

/**
 * LFS 文件读写测试
 * 注意：这些测试针对 bare 仓库模式，因为 LFS 逻辑主要在 bare 模式下生效
 *
 * LFS 工作流程：
 * 1. 写入时：内容被存储到 lfs/objects/{oid[0:2]}/{oid[2:4]}/{oid}，Git 中存储 LFS 指针
 * 2. 读取时：getFileContent 返回 LFS 指针（需要 smudge 获取实际内容）
 */
describe("GitService LFS support (bare mode)", () => {
  let repoDir: string;
  let gitService: GitService;

  beforeEach(async () => {
    repoDir = await createTempBareRepo();
    gitService = new GitService(repoDir, "bare");
  });

  afterEach(async () => {
    await fs.rm(repoDir, { recursive: true, force: true });
  });

  // 测试不同文件类型的 LFS 处理
  describe("LFS file type detection", () => {
    // 图像类型
    const imageTypes = [
      { ext: "png", mime: "image/png" },
      { ext: "jpg", mime: "image/jpeg" },
      { ext: "jpeg", mime: "image/jpeg" },
      { ext: "gif", mime: "image/gif" },
      { ext: "webp", mime: "image/webp" },
      { ext: "svg", mime: "image/svg+xml" },
    ];

    for (const { ext, mime } of imageTypes) {
      test(`should handle .${ext} files as LFS`, async () => {
        const content = Buffer.from(`fake ${ext} content - ${"X".repeat(100)}`);
        const filename = `test-image.${ext}`;

        const commit = await gitService.saveFile(
          filename,
          content,
          `add ${ext} file`,
        );
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);

        // 读回 LFS 指针
        const readBack = await gitService.getFileContent(filename);
        const pointer = toStr(readBack as Buffer);

        // 验证是 LFS 指针格式
        expect(isLfsPointer(pointer)).toBe(true);

        // 解析指针并验证
        const parsed = parseLfsPointer(pointer);
        expect(parsed).not.toBeNull();
        expect(parsed!.size).toBe(content.length);

        // 验证 LFS 对象存在
        expect(await lfsObjectExists(repoDir, parsed!.oid)).toBe(true);

        // 验证 LFS 对象内容
        const lfsContent = await readLfsObject(repoDir, parsed!.oid);
        expect(lfsContent.equals(content)).toBe(true);
      }, 15000);
    }

    // 视频类型
    const videoTypes = ["mp4", "mov", "webm", "avi", "mkv"];

    for (const ext of videoTypes) {
      test(`should handle .${ext} video files as LFS`, async () => {
        const content = Buffer.from(
          `fake ${ext} video data - ${"V".repeat(200)}`,
        );
        const filename = `test-video.${ext}`;

        const commit = await gitService.saveFile(
          filename,
          content,
          `add ${ext} video`,
        );
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);

        const readBack = await gitService.getFileContent(filename);
        const pointer = toStr(readBack as Buffer);

        expect(isLfsPointer(pointer)).toBe(true);
        const parsed = parseLfsPointer(pointer);
        expect(parsed).not.toBeNull();
        expect(parsed!.size).toBe(content.length);

        // 验证 LFS 对象内容
        const lfsContent = await readLfsObject(repoDir, parsed!.oid);
        expect(lfsContent.equals(content)).toBe(true);
      }, 15000);
    }

    // 音频类型
    const audioTypes = ["mp3", "wav", "flac", "aac", "ogg"];

    for (const ext of audioTypes) {
      test(`should handle .${ext} audio files as LFS`, async () => {
        const content = Buffer.from(
          `fake ${ext} audio data - ${"A".repeat(150)}`,
        );
        const filename = `test-audio.${ext}`;

        const commit = await gitService.saveFile(
          filename,
          content,
          `add ${ext} audio`,
        );
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);

        const readBack = await gitService.getFileContent(filename);
        const pointer = toStr(readBack as Buffer);

        expect(isLfsPointer(pointer)).toBe(true);
        const parsed = parseLfsPointer(pointer);
        expect(parsed).not.toBeNull();
        expect(parsed!.size).toBe(content.length);

        const lfsContent = await readLfsObject(repoDir, parsed!.oid);
        expect(lfsContent.equals(content)).toBe(true);
      }, 15000);
    }

    // 压缩包类型
    const archiveTypes = ["zip", "7z", "rar", "tar", "gz"];

    for (const ext of archiveTypes) {
      test(`should handle .${ext} archive files as LFS`, async () => {
        const content = Buffer.from(
          `fake ${ext} archive data - ${"Z".repeat(100)}`,
        );
        const filename = `test-archive.${ext}`;

        const commit = await gitService.saveFile(
          filename,
          content,
          `add ${ext} archive`,
        );
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);

        const readBack = await gitService.getFileContent(filename);
        const pointer = toStr(readBack as Buffer);

        expect(isLfsPointer(pointer)).toBe(true);
        const parsed = parseLfsPointer(pointer);
        expect(parsed).not.toBeNull();
        expect(parsed!.size).toBe(content.length);

        const lfsContent = await readLfsObject(repoDir, parsed!.oid);
        expect(lfsContent.equals(content)).toBe(true);
      }, 15000);
    }

    // 非 LFS 类型（纯文本）应正常存储
    test("should handle .txt files as regular Git objects (non-LFS)", async () => {
      const content = Buffer.from("plain text content");
      const filename = "plain.txt";

      const commit = await gitService.saveFile(filename, content, "add txt");
      expect(commit).toMatch(/^[0-9a-f]{40}$/i);

      // 非 LFS 文件应该直接返回内容，不是 LFS 指针
      const readBack = await gitService.getFileContent(filename);
      const text = toStr(readBack as Buffer);
      expect(isLfsPointer(text)).toBe(false);
      expect(text).toBe(content.toString());
    }, 15000);
  });

  // 测试不同文件大小的 LFS 处理
  describe("LFS file size handling", () => {
    const sizes = [
      { name: "tiny", size: 100 }, // 100 bytes
      { name: "small", size: 1024 }, // 1 KB
      { name: "medium", size: 10 * 1024 }, // 10 KB
      { name: "large", size: 100 * 1024 }, // 100 KB
      { name: "xlarge", size: 1024 * 1024 }, // 1 MB
    ];

    for (const { name, size } of sizes) {
      test(`should handle ${name} (${size} bytes) PNG file`, async () => {
        // 创建指定大小的内容
        const content = Buffer.alloc(size);
        // 填充一些可识别的模式
        for (let i = 0; i < size; i++) {
          content[i] = i % 256;
        }

        const filename = `size-test-${name}.png`;

        const commit = await gitService.saveFile(
          filename,
          content,
          `add ${name} file`,
        );
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);

        // 验证 LFS 指针和对象
        const readBack = (await gitService.getFileContent(filename)) as Buffer;
        const pointer = toStr(readBack);
        expect(isLfsPointer(pointer)).toBe(true);

        const parsed = parseLfsPointer(pointer);
        expect(parsed).not.toBeNull();
        expect(parsed!.size).toBe(size);

        // 读取 LFS 对象并验证大小和内容
        const lfsContent = await readLfsObject(repoDir, parsed!.oid);
        expect(lfsContent.length).toBe(size);

        // 验证内容完整性
        for (let i = 0; i < Math.min(size, 1000); i++) {
          expect(lfsContent[i]).toBe(i % 256);
        }
      }, 30000);
    }

    // 测试 5MB 大文件
    test("should handle 5MB PNG file", async () => {
      const size = 5 * 1024 * 1024;
      const content = Buffer.alloc(size);
      // 填充随机数据以模拟真实二进制
      for (let i = 0; i < size; i++) {
        content[i] = (i * 17 + 31) % 256;
      }

      const filename = "large-5mb.png";

      const commit = await gitService.saveFile(
        filename,
        content,
        "add 5MB file",
      );
      expect(commit).toMatch(/^[0-9a-f]{40}$/i);

      const readBack = (await gitService.getFileContent(filename)) as Buffer;
      const pointer = toStr(readBack);
      expect(isLfsPointer(pointer)).toBe(true);

      const parsed = parseLfsPointer(pointer);
      expect(parsed).not.toBeNull();
      expect(parsed!.size).toBe(size);

      // 验证 LFS 对象
      const lfsContent = await readLfsObject(repoDir, parsed!.oid);
      expect(lfsContent.length).toBe(size);

      // 抽样验证内容
      expect(lfsContent[0]).toBe(31);
      expect(lfsContent[1000]).toBe((1000 * 17 + 31) % 256);
      expect(lfsContent[size - 1]).toBe(((size - 1) * 17 + 31) % 256);
    }, 60000);
  });

  // 测试 LFS 文件的并发写入
  describe("LFS concurrent writes", () => {
    test("concurrent LFS file uploads should all succeed", async () => {
      const numFiles = 5;

      // 创建不同类型的 LFS 文件
      const files = [
        {
          name: "concurrent-1.png",
          content: Buffer.from("png content 1 " + "P".repeat(100)),
        },
        {
          name: "concurrent-2.jpg",
          content: Buffer.from("jpg content 2 " + "J".repeat(100)),
        },
        {
          name: "concurrent-3.mp4",
          content: Buffer.from("mp4 content 3 " + "M".repeat(100)),
        },
        {
          name: "concurrent-4.zip",
          content: Buffer.from("zip content 4 " + "Z".repeat(100)),
        },
        {
          name: "concurrent-5.mp3",
          content: Buffer.from("mp3 content 5 " + "A".repeat(100)),
        },
      ];

      // 并发上传
      const promises = files.map((f) =>
        gitService.saveFile(f.name, f.content, `add ${f.name}`),
      );

      const commits = await Promise.all(promises);

      // 所有提交应成功
      for (const commit of commits) {
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);
      }

      // 所有提交应唯一
      const uniqueCommits = new Set(commits);
      expect(uniqueCommits.size).toBe(numFiles);

      // 验证所有文件的 LFS 对象
      for (const f of files) {
        const readBack = await gitService.getFileContent(f.name);
        const pointer = toStr(readBack as Buffer);
        expect(isLfsPointer(pointer)).toBe(true);

        const parsed = parseLfsPointer(pointer);
        expect(parsed).not.toBeNull();
        expect(parsed!.size).toBe(f.content.length);

        const lfsContent = await readLfsObject(repoDir, parsed!.oid);
        expect(lfsContent.equals(f.content)).toBe(true);
      }
    }, 30000);

    test("concurrent uploads of same content should deduplicate LFS objects", async () => {
      // 使用相同内容但不同文件名
      const content = Buffer.from(
        "identical content for dedup test " + "D".repeat(200),
      );
      const expectedOid = await computeSha256(content);

      const files = ["dedup-1.png", "dedup-2.png", "dedup-3.png"];

      // 并发上传相同内容
      const promises = files.map((name) =>
        gitService.saveFile(name, content, `add ${name}`),
      );

      const commits = await Promise.all(promises);

      // 所有提交应成功
      for (const commit of commits) {
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);
      }

      // 验证所有文件指向相同的 OID
      const oids: string[] = [];
      for (const name of files) {
        const readBack = await gitService.getFileContent(name);
        const pointer = toStr(readBack as Buffer);
        expect(isLfsPointer(pointer)).toBe(true);

        const parsed = parseLfsPointer(pointer);
        expect(parsed).not.toBeNull();
        oids.push(parsed!.oid);
      }

      // 所有 OID 应该相同
      expect(new Set(oids).size).toBe(1);
      expect(oids[0]).toBe(expectedOid);

      // 检查 LFS 对象目录：相同内容应只存储一份
      const lfsDir = path.join(repoDir, "lfs", "objects");
      let lfsObjectCount = 0;
      try {
        const dirs = await fs.readdir(lfsDir);
        for (const d1 of dirs) {
          if (d1.startsWith(".")) continue;
          const subDir = path.join(lfsDir, d1);
          const subdirs = await fs.readdir(subDir);
          for (const d2 of subdirs) {
            const objDir = path.join(subDir, d2);
            const objects = await fs.readdir(objDir);
            lfsObjectCount += objects.filter((o) => !o.startsWith(".")).length;
          }
        }
      } catch {
        // LFS 目录可能不存在
      }

      // 由于内容相同，应该只有一个 LFS 对象
      expect(lfsObjectCount).toBe(1);
    }, 30000);
  });

  // 测试 LFS 文件的流式写入
  describe("LFS stream writes", () => {
    test("should handle ReadableStream input for LFS files", async () => {
      const content = "streamed PNG content for LFS " + "S".repeat(500);
      const contentBuffer = Buffer.from(content);
      const encoder = new TextEncoder();

      const stream = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode(content));
          controller.close();
        },
      });

      const commit = await gitService.saveFile(
        "stream-lfs.png",
        stream,
        "stream LFS upload",
      );
      expect(commit).toMatch(/^[0-9a-f]{40}$/i);

      const readBack = await gitService.getFileContent("stream-lfs.png");
      const pointer = toStr(readBack as Buffer);
      expect(isLfsPointer(pointer)).toBe(true);

      const parsed = parseLfsPointer(pointer);
      expect(parsed).not.toBeNull();
      expect(parsed!.size).toBe(contentBuffer.length);

      const lfsContent = await readLfsObject(repoDir, parsed!.oid);
      expect(lfsContent.equals(contentBuffer)).toBe(true);
    }, 15000);

    test("should handle chunked ReadableStream for large LFS files", async () => {
      const chunkSize = 64 * 1024; // 64KB chunks
      const numChunks = 10;
      const totalSize = chunkSize * numChunks;

      // 创建分块流
      let chunkIndex = 0;
      const stream = new ReadableStream<Uint8Array>({
        pull(controller) {
          if (chunkIndex >= numChunks) {
            controller.close();
            return;
          }
          const chunk = new Uint8Array(chunkSize);
          // 填充可识别模式
          chunk.fill(chunkIndex % 256);
          controller.enqueue(chunk);
          chunkIndex++;
        },
      });

      const commit = await gitService.saveFile(
        "chunked-lfs.mp4",
        stream,
        "chunked LFS upload",
      );
      expect(commit).toMatch(/^[0-9a-f]{40}$/i);

      const readBack = (await gitService.getFileContent(
        "chunked-lfs.mp4",
      )) as Buffer;
      const pointer = toStr(readBack);
      expect(isLfsPointer(pointer)).toBe(true);

      const parsed = parseLfsPointer(pointer);
      expect(parsed).not.toBeNull();
      expect(parsed!.size).toBe(totalSize);

      // 验证 LFS 对象内容
      const lfsContent = await readLfsObject(repoDir, parsed!.oid);
      expect(lfsContent.length).toBe(totalSize);

      // 验证每个块的内容
      for (let i = 0; i < numChunks; i++) {
        const offset = i * chunkSize;
        expect(lfsContent[offset]).toBe(i % 256);
        expect(lfsContent[offset + chunkSize - 1]).toBe(i % 256);
      }
    }, 30000);
  });

  // 测试 LFS 文件的删除
  describe("LFS file deletion", () => {
    test("should delete LFS tracked files", async () => {
      const content = Buffer.from("LFS content to delete " + "D".repeat(100));
      const filename = "delete-me.png";

      // 创建文件
      await gitService.saveFile(filename, content, "add file");

      // 验证存在并是 LFS 指针
      const before = await gitService.getFileContent(filename);
      const pointer = toStr(before as Buffer);
      expect(isLfsPointer(pointer)).toBe(true);

      // 删除文件
      await gitService.deleteFile(filename, "delete LFS file");

      // 验证已删除
      try {
        await gitService.getFileContent(filename);
        expect.fail("Should have thrown");
      } catch (e) {
        // Expected
      }
    }, 15000);
  });

  // 测试混合 LFS 和非 LFS 文件
  describe("Mixed LFS and non-LFS files", () => {
    test("should handle mixed file types correctly", async () => {
      // 注意：*.ts 在 LFS 模式中代表 MPEG Transport Stream 视频格式
      // 所以使用 .txt, .css, .html 等确定不在 LFS 列表中的扩展名
      const files = [
        {
          name: "readme.txt",
          content: Buffer.from("# README\nThis is text"),
          isLfs: false,
        },
        {
          name: "image.png",
          content: Buffer.from("PNG binary data " + "P".repeat(100)),
          isLfs: true,
        },
        {
          name: "style.css",
          content: Buffer.from("body { margin: 0; }"),
          isLfs: false,
        },
        {
          name: "video.mp4",
          content: Buffer.from("MP4 binary data " + "V".repeat(100)),
          isLfs: true,
        },
        {
          name: "config.json",
          content: Buffer.from('{"key": "value"}'),
          isLfs: false,
        },
        {
          name: "index.html",
          content: Buffer.from("<html></html>"),
          isLfs: false,
        },
      ];

      // 顺序写入
      for (const f of files) {
        const commit = await gitService.saveFile(
          f.name,
          f.content,
          `add ${f.name}`,
        );
        expect(commit).toMatch(/^[0-9a-f]{40}$/i);
      }

      // 验证所有文件内容
      for (const f of files) {
        const readBack = await gitService.getFileContent(f.name);
        const text = toStr(readBack as Buffer);

        if (f.isLfs) {
          // LFS 文件应返回指针
          expect(isLfsPointer(text)).toBe(true);
          const parsed = parseLfsPointer(text);
          expect(parsed).not.toBeNull();
          expect(parsed!.size).toBe(f.content.length);

          // 验证 LFS 对象
          const lfsContent = await readLfsObject(repoDir, parsed!.oid);
          expect(lfsContent.equals(f.content)).toBe(true);
        } else {
          // 非 LFS 文件应返回原始内容
          expect(isLfsPointer(text)).toBe(false);
          expect(text).toBe(f.content.toString());
        }
      }
    }, 30000);
  });
});
