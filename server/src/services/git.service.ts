import { $} from 'bun';
import fs from 'node:fs/promises';
import path from 'node:path';
import { FileInfo, CommitInfo, FileHistory, CommitSummary } from '../types/index.js';
import { normalizePathForGit } from '../utils/path-validator.js';
import { config } from '../config.js';

export class GitService {
  private dir: string;
  private mode: 'worktree' | 'bare';

  constructor(repoPath: string, mode: 'worktree' | 'bare' = 'worktree') {
    this.dir = path.resolve(repoPath);
    this.mode = mode;
  }

  private get isBare(): boolean {
    return this.mode === 'bare';
  }

  private normalizeRelPath(p: string): string {
    return normalizePathForGit(p).replaceAll('\\', '/');
  }

  private async writeBlobFromContent(
    content: Uint8Array | ArrayBuffer | Blob | ReadableStream<Uint8Array>
  ): Promise<string> {
    const proc = Bun.spawn(['git', 'hash-object', '-w', '--stdin'], {
      cwd: this.dir,
      stdin: 'pipe',
      stdout: 'pipe',
      stderr: 'pipe',
    });

    const sink = proc.stdin as unknown as {
      write: (chunk: Uint8Array) => unknown;
      end: () => unknown;
    };

    const stream: ReadableStream<Uint8Array> =
      content instanceof Uint8Array
        ? new ReadableStream<Uint8Array>({
            start(controller) {
              controller.enqueue(content);
              controller.close();
            },
          })
        : content instanceof ArrayBuffer
          ? new ReadableStream<Uint8Array>({
              start(controller) {
                controller.enqueue(new Uint8Array(content));
                controller.close();
              },
            })
          : content instanceof Blob
            ? (content.stream() as ReadableStream<Uint8Array>)
            : (content as ReadableStream<Uint8Array>);

    const reader = stream.getReader();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        if (value && value.byteLength > 0) {
          await sink.write(value);
        }
      }
    } finally {
      try {
        await reader.cancel();
      } catch {
        // ignore
      }
      try {
        await sink.end();
      } catch {
        // ignore
      }
    }

    const code = await proc.exited;
    const stdout = (await new Response(proc.stdout).text()).trim();
    if (code !== 0 || !stdout) {
      const err = await new Response(proc.stderr).text();
      throw new Error(`git hash-object 失败: ${err || code}`);
    }
    return stdout;
  }

  private async updateIndexAddBlob(filePath: string, blobSha: string, mode: string = '100644'): Promise<void> {
    const rel = this.normalizeRelPath(filePath);
    const proc = Bun.spawn(['git', 'update-index', '--add', '--cacheinfo', mode, blobSha, rel], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'pipe',
    });
    const code = await proc.exited;
    if (code !== 0) {
      const err = await new Response(proc.stderr).text();
      throw new Error(`git update-index 失败: ${err || code}`);
    }
  }

  private async updateIndexRemovePath(filePath: string): Promise<void> {
    const rel = this.normalizeRelPath(filePath);
    const proc = Bun.spawn(['git', 'update-index', '--remove', '--', rel], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'pipe',
    });
    const code = await proc.exited;
    if (code !== 0) {
      const err = await new Response(proc.stderr).text();
      throw new Error(`git update-index --remove 失败: ${err || code}`);
    }
  }

  /**
   * 初始化Git仓库（如果不存在）
   */
  async initRepo(): Promise<void> {
    try {
      // 确保仓库目录存在（克隆后 data/ 可能不存在）
      await fs.mkdir(this.dir, { recursive: true });

      if (this.isBare) {
        // bare 仓库：repo 本身就是 gitdir（无工作区）
        const headPath = path.join(this.dir, 'HEAD');
        try {
          await fs.access(headPath);
          console.log('Git bare 仓库已存在');
        } catch {
          console.log('初始化 Git bare 仓库...');
          await $`git init --bare`.cwd(this.dir);

          // bare 下创建空初始提交，确保 HEAD 可用
          await $`git -c user.name="VFiles System" -c user.email="system@vfiles.local" commit --allow-empty -m "Initial commit"`
            .cwd(this.dir)
            .quiet();
        }
      } else {
        // worktree 仓库：检查 .git
        const gitDir = path.join(this.dir, '.git');
        try {
          await fs.access(gitDir);
          console.log('Git仓库已存在');
        } catch {
          // 不存在，初始化新仓库
          console.log('初始化Git仓库...');
          await $`git init`.cwd(this.dir);

          // 创建初始提交
          await this.createInitialCommit();
        }
      }

      // Git LFS（非必需）：尽量启用并写入 .gitattributes
      if (config.enableGitLfs) {
        try {
          const ok = await this.ensureGitLfsInstalled();
          if (ok) {
            await this.ensureGitLfsTrackedPatterns(config.gitLfsTrackPatterns);
          }
        } catch (e) {
          console.warn('Git LFS 初始化失败，将回退为普通 Git：', e);
        }
      }
    } catch (error) {
      console.error('初始化Git仓库失败:', error);
      throw error;
    }
  }

  private async ensureGitLfsInstalled(): Promise<boolean> {
    const proc = Bun.spawn(['git', 'lfs', 'version'], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'ignore',
    });
    const code = await proc.exited;
    if (code !== 0) {
      console.warn('未检测到 git-lfs（git lfs version 失败），将不会启用 LFS');
      return false;
    }

    // 仅对当前仓库安装 hooks（不污染全局）
    const install = Bun.spawn(['git', 'lfs', 'install', '--local'], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'ignore',
    });
    await install.exited;
    return true;
  }

  private async ensureGitLfsTrackedPatterns(patterns: string[]): Promise<void> {
    if (!patterns.length) return;

    // bare 仓库没有工作区文件，无法使用 `git lfs track` 写入 .gitattributes；改为直接更新 index。
    if (this.isBare) {
      const desiredLines = patterns.map((p) => `${p} filter=lfs diff=lfs merge=lfs -text`);

      let existing = '';
      try {
        const result = await $`git show HEAD:.gitattributes`.cwd(this.dir).quiet();
        existing = result.stdout.toString();
      } catch {
        existing = '';
      }

      const existingLines = existing
        .split(/\r?\n/)
        .map((l) => l.trimEnd())
        .filter(Boolean);
      const set = new Set(existingLines);

      let changed = false;
      for (const line of desiredLines) {
        if (!set.has(line)) {
          existingLines.push(line);
          set.add(line);
          changed = true;
        }
      }

      if (!changed) return;

      const newContent = `${existingLines.join('\n')}\n`;
      const blobSha = await this.writeBlobFromContent(new TextEncoder().encode(newContent));
      await this.updateIndexAddBlob('.gitattributes', blobSha, '100644');
      await $`git -c user.name="VFiles System" -c user.email="system@vfiles.local" commit -m "chore: configure git-lfs"`
        .cwd(this.dir)
        .quiet();
      return;
    }

    let changed = false;
    for (const p of patterns) {
      const proc = Bun.spawn(['git', 'lfs', 'track', p], {
        cwd: this.dir,
        stdout: 'ignore',
        stderr: 'ignore',
      });
      const code = await proc.exited;
      if (code === 0) changed = true;
    }

    if (!changed) return;

    // 若 .gitattributes 被更新则提交一次
    const status = await $`git status --porcelain`.cwd(this.dir).quiet();
    if (status.stdout.toString().includes('.gitattributes')) {
      await $`git add .gitattributes`.cwd(this.dir);
      await $`git -c user.name="VFiles System" -c user.email="system@vfiles.local" commit -m "chore: configure git-lfs"`.cwd(this.dir);
    }
  }

  async isLfsPointerAtCommit(filePath: string, commitHash: string): Promise<boolean> {
    const spec = `${commitHash}:${normalizePathForGit(filePath)}`;
    const proc = Bun.spawn(['git', 'show', spec], {
      cwd: this.dir,
      stdout: 'pipe',
      stderr: 'ignore',
    });

    const reader = proc.stdout.getReader();
    try {
      const { value } = await reader.read();
      const head = value ? new TextDecoder().decode(value.subarray(0, 200)) : '';
      return head.startsWith('version https://git-lfs.github.com/spec/v1');
    } catch {
      return false;
    } finally {
      try {
        await reader.cancel();
      } catch {
        // ignore
      }
      try {
        proc.kill();
      } catch {
        // ignore
      }
    }
  }

  getFileContentSmudgedStreamAtCommit(filePath: string, commitHash: string): ReadableStream<Uint8Array> {
    const spec = `${commitHash}:${normalizePathForGit(filePath)}`;
    const show = Bun.spawn(['git', 'show', spec], {
      cwd: this.dir,
      stdout: 'pipe',
      stderr: 'ignore',
    });

    const smudge = Bun.spawn(['git', 'lfs', 'smudge', '--', filePath], {
      cwd: this.dir,
      stdin: 'pipe',
      stdout: 'pipe',
      stderr: 'ignore',
    });

    // pump: show.stdout -> smudge.stdin
    (async () => {
      const reader = show.stdout.getReader();
      const sink = smudge.stdin as unknown as {
        write: (chunk: Uint8Array) => unknown;
        end: () => unknown;
      };
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          if (value) await sink.write(value);
        }
      } finally {
        try {
          await sink.end();
        } catch {
          // ignore
        }
        try {
          await reader.cancel();
        } catch {
          // ignore
        }
      }
    })();

    return smudge.stdout;
  }

  /**
   * 创建初始提交
   */
  private async createInitialCommit(): Promise<void> {
    if (this.isBare) {
      // bare 模式不创建工作区文件
      return;
    }
    const readmePath = path.join(this.dir, 'README.md');
    const readmeContent = '# VFiles 数据目录\n\n此目录用于存储文件管理系统的数据。\n';
    
    await fs.writeFile(readmePath, readmeContent, 'utf-8');
    
    await $`git add README.md`.cwd(this.dir);
    await $`git -c user.name="VFiles System" -c user.email="system@vfiles.local" commit -m "Initial commit"`.cwd(this.dir);
  }

  /**
   * 列出指定路径下的文件和文件夹
   */
  async listFiles(dirPath: string = ''): Promise<FileInfo[]> {
    if (this.isBare) {
      // 从 HEAD 的 tree 列出目录项
      const args = ['git', 'ls-tree', '-z', '-l', 'HEAD'];
      const normalizedDir = dirPath ? normalizePathForGit(dirPath) : '';
      if (normalizedDir) {
        args.push('--', normalizedDir);
      }

      const proc = Bun.spawn(args, { cwd: this.dir, stdout: 'pipe', stderr: 'pipe' });
      const code = await proc.exited;
      const outBuf = await new Response(proc.stdout).arrayBuffer();

      // 空目录/空仓库：ls-tree 可能为空输出
      if (code !== 0) {
        return [];
      }

      const bytes = new Uint8Array(outBuf);
      const text = new TextDecoder().decode(bytes);
      const records = text.split('\0').filter(Boolean);

      const files: FileInfo[] = [];
      for (const rec of records) {
        // 格式：<mode> <type> <sha> <size>\t<name>
        const tab = rec.indexOf('\t');
        if (tab <= 0) continue;
        const meta = rec.slice(0, tab).trim().split(/\s+/);
        const name = rec.slice(tab + 1);
        if (!name) continue;

        const typeToken = meta[1];
        const sizeToken = meta[3];

        const relPath = normalizedDir ? `${normalizedDir}/${name}` : name;
        const filePath = relPath.replaceAll('\\', '/');

        const lastCommit = await this.getLastCommit(filePath);
        const mtime = lastCommit?.date || new Date(0).toISOString();

        files.push({
          name,
          path: filePath,
          type: typeToken === 'tree' ? 'directory' : 'file',
          size: sizeToken && sizeToken !== '-' ? Number.parseInt(sizeToken, 10) : 0,
          mtime,
          lastCommit,
        });
      }

      return files.sort((a, b) => {
        if (a.type !== b.type) return a.type === 'directory' ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
    }

    const fullPath = path.join(this.dir, dirPath);
    const files: FileInfo[] = [];

    try {
      const entries = await fs.readdir(fullPath, { withFileTypes: true });

      for (const entry of entries) {
        // 跳过.git目录
        if (entry.name === '.git') continue;

        const filePath = path.join(dirPath, entry.name);
        const fullFilePath = path.join(fullPath, entry.name);
        const stats = await fs.stat(fullFilePath);

        // 获取最后一次提交信息
        const lastCommit = await this.getLastCommit(filePath);

        files.push({
          name: entry.name,
          path: filePath,
          type: entry.isDirectory() ? 'directory' : 'file',
          size: stats.size,
          mtime: stats.mtime.toISOString(),
          lastCommit,
        });
      }

      // 排序：文件夹在前，然后按名称排序
      return files.sort((a, b) => {
        if (a.type !== b.type) {
          return a.type === 'directory' ? -1 : 1;
        }
        return a.name.localeCompare(b.name);
      });
    } catch (error) {
      console.error('读取文件列表失败:', error);
      return [];
    }
  }

  /**
   * 获取文件内容
   */
  async getFileContent(filePath: string, commitHash?: string): Promise<Buffer> {
    try {
      if (commitHash) {
        // 获取历史版本
        const result = await $`git show ${commitHash}:${normalizePathForGit(filePath)}`.cwd(this.dir).quiet();
        return Buffer.from(result.stdout);
      } else {
        // 获取当前版本
        if (this.isBare) {
          const result = await $`git show HEAD:${normalizePathForGit(filePath)}`.cwd(this.dir).quiet();
          return Buffer.from(result.stdout);
        }

        const fullPath = path.join(this.dir, filePath);
        return await fs.readFile(fullPath);
      }
    } catch (error) {
      throw new Error(`读取文件失败: ${error}`);
    }
  }

  /**
   * 轻量校验：某个 commit 下是否存在该文件（不读取内容）
   */
  async fileExistsAtCommit(filePath: string, commitHash: string): Promise<boolean> {
    const spec = `${commitHash}:${normalizePathForGit(filePath)}`;
    const proc = Bun.spawn(['git', 'cat-file', '-e', spec], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'ignore',
    });
    const code = await proc.exited;
    return code === 0;
  }

  /**
   * 获取历史版本文件内容的流（真正流式，不会把整个文件读入内存）
   */
  getFileContentStreamAtCommit(filePath: string, commitHash: string): ReadableStream<Uint8Array> {
    const spec = `${commitHash}:${normalizePathForGit(filePath)}`;
    const proc = Bun.spawn(['git', 'show', spec], {
      cwd: this.dir,
      stdout: 'pipe',
      stderr: 'ignore',
    });
    return proc.stdout;
  }

  /**
   * 获取历史版本文件大小（用于 Range/断点续传）
   */
  async getFileSizeAtCommit(filePath: string, commitHash: string): Promise<number> {
    const spec = `${commitHash}:${normalizePathForGit(filePath)}`;
    const proc = Bun.spawn(['git', 'cat-file', '-s', spec], {
      cwd: this.dir,
      stdout: 'pipe',
      stderr: 'ignore',
    });
    const text = await new Response(proc.stdout).text();
    const size = Number.parseInt(text.trim(), 10);
    if (!Number.isFinite(size) || size < 0) {
      throw new Error('无法读取历史版本文件大小');
    }
    return size;
  }

  /**
   * 获取历史版本文件内容的区间流（Range）。注意：Git 对 blob 不支持随机 seek，
   * 这里通过流式丢弃前置字节来实现逻辑 Range，以支持断点续传。
   */
  getFileContentRangeStreamAtCommit(
    filePath: string,
    commitHash: string,
    start: number,
    end: number
  ): ReadableStream<Uint8Array> {
    const spec = `${commitHash}:${normalizePathForGit(filePath)}`;
    const proc = Bun.spawn(['git', 'show', spec], {
      cwd: this.dir,
      stdout: 'pipe',
      stderr: 'ignore',
    });

    return new ReadableStream<Uint8Array>({
      start(controller) {
        (async () => {
          const reader = proc.stdout.getReader();
          let toSkip = start;
          let remaining = end - start + 1;

          try {
            while (remaining > 0) {
              const { done, value } = await reader.read();
              if (done) break;
              if (!value || value.byteLength === 0) continue;

              let chunk = value;
              if (toSkip > 0) {
                if (chunk.byteLength <= toSkip) {
                  toSkip -= chunk.byteLength;
                  continue;
                }
                chunk = chunk.subarray(toSkip);
                toSkip = 0;
              }

              if (chunk.byteLength > remaining) {
                controller.enqueue(chunk.subarray(0, remaining));
                remaining = 0;
                break;
              }

              controller.enqueue(chunk);
              remaining -= chunk.byteLength;
            }

            controller.close();
          } catch (e) {
            controller.error(e);
          } finally {
            try {
              await reader.cancel();
            } catch {
              // ignore
            }
            try {
              proc.kill();
            } catch {
              // ignore
            }
          }
        })();
      },
      cancel() {
        try {
          proc.kill();
        } catch {
          // ignore
        }
      },
    });
  }

  /**
   * 保存文件并提交到Git
   */
  async saveFile(
    filePath: string,
    content: Uint8Array | ArrayBuffer | Blob | ReadableStream<Uint8Array>,
    message: string,
    author?: { name: string; email: string }
  ): Promise<string> {
    try {
      if (this.isBare) {
        const blobSha = await this.writeBlobFromContent(content);
        await this.updateIndexAddBlob(filePath, blobSha, '100644');
      } else {
        const fullPath = path.join(this.dir, filePath);
        const dirPath = path.dirname(fullPath);

        // 确保目录存在
        await fs.mkdir(dirPath, { recursive: true });

        // 写入文件（用 Bun.write 支持流式写入，避免把大文件整体读入内存）
        await Bun.write(fullPath, content as any);

        // 添加到Git
        await $`git add ${normalizePathForGit(filePath)}`.cwd(this.dir);
      }

      // 提交
      const authorName = author?.name || 'VFiles User';
      const authorEmail = author?.email || 'user@vfiles.local';
      
      await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(this.dir);

      // 获取最新提交hash
      const result = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
      return result.stdout.toString().trim();
    } catch (error) {
      throw new Error(`保存文件失败: ${error}`);
    }
  }

  /**
   * 提交一个已存在/已写入磁盘的文件（不会再次写入内容）
   */
  async commitFile(
    filePath: string,
    message: string,
    author?: { name: string; email: string }
  ): Promise<string> {
    try {
      if (this.isBare) {
        throw new Error('bare 模式不支持 commitFile（请使用 saveFile 直接写入并提交）');
      }
      // 添加到Git
      await $`git add ${normalizePathForGit(filePath)}`.cwd(this.dir);

      // 提交
      const authorName = author?.name || 'VFiles User';
      const authorEmail = author?.email || 'user@vfiles.local';
      await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(
        this.dir
      );

      // 获取最新提交hash
      const result = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
      return result.stdout.toString().trim();
    } catch (error) {
      throw new Error(`提交文件失败: ${error}`);
    }
  }

  /**
   * 删除文件
   */
  async deleteFile(
    filePath: string,
    message: string,
    author?: { name: string; email: string }
  ): Promise<void> {
    try {
      if (this.isBare) {
        // 文件：直接从 index 移除；目录：移除其下全部文件
        const rel = this.normalizeRelPath(filePath);
        const existsAsFile = await this.fileExistsAtCommit(rel, 'HEAD');
        if (existsAsFile) {
          await this.updateIndexRemovePath(rel);
        } else {
          // 目录：列出其下所有文件
          const proc = Bun.spawn(['git', 'ls-tree', '-r', '-z', '--name-only', 'HEAD', '--', rel], {
            cwd: this.dir,
            stdout: 'pipe',
            stderr: 'pipe',
          });
          const code = await proc.exited;
          if (code !== 0) {
            const err = await new Response(proc.stderr).text();
            throw new Error(err || '路径不存在');
          }
          const text = new TextDecoder().decode(new Uint8Array(await new Response(proc.stdout).arrayBuffer()));
          const files = text.split('\0').filter(Boolean);
          if (files.length === 0) {
            throw new Error('路径不存在');
          }
          for (const f of files) {
            await this.updateIndexRemovePath(f);
          }
        }

        const authorName = author?.name || 'VFiles User';
        const authorEmail = author?.email || 'user@vfiles.local';
        await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(this.dir);
        return;
      }

      const fullPath = path.join(this.dir, filePath);

      const st = await fs.stat(fullPath);
      if (st.isDirectory()) {
        // 删除目录（递归）
        await fs.rm(fullPath, { recursive: true, force: false });
        await $`git rm -r -- ${normalizePathForGit(filePath)}`.cwd(this.dir);
      } else {
        // 删除文件
        await fs.unlink(fullPath);
        await $`git rm -- ${normalizePathForGit(filePath)}`.cwd(this.dir);
      }

      // 提交
      const authorName = author?.name || 'VFiles User';
      const authorEmail = author?.email || 'user@vfiles.local';
      
      await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(this.dir);
    } catch (error) {
      throw new Error(`删除文件失败: ${error}`);
    }
  }

  /**
   * 创建目录并提交到 Git（通过 .gitkeep 让目录可跟踪）
   */
  async createDirectory(
    dirPath: string,
    message: string,
    author?: { name: string; email: string }
  ): Promise<string> {
    try {
      if (this.isBare) {
        const keepRel = path.join(dirPath, '.gitkeep').replaceAll('\\', '/');
        // 空 blob
        const blobSha = await this.writeBlobFromContent(new Uint8Array(0));
        await this.updateIndexAddBlob(keepRel, blobSha, '100644');

        const authorName = author?.name || 'VFiles User';
        const authorEmail = author?.email || 'user@vfiles.local';
        await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(this.dir);
        const result = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
        return result.stdout.toString().trim();
      }

      const fullDir = path.join(this.dir, dirPath);

      // 若已存在则报错，避免误提交
      try {
        await fs.stat(fullDir);
        throw new Error('目录已存在');
      } catch (err: any) {
        if (err?.code !== 'ENOENT') {
          throw err;
        }
      }

      await fs.mkdir(fullDir, { recursive: true });

      const keepRel = path.join(dirPath, '.gitkeep');
      const keepFull = path.join(this.dir, keepRel);
      await fs.writeFile(keepFull, '', 'utf-8');

      await $`git add -- ${normalizePathForGit(keepRel)}`.cwd(this.dir);

      const authorName = author?.name || 'VFiles User';
      const authorEmail = author?.email || 'user@vfiles.local';
      await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(
        this.dir
      );

      const result = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
      return result.stdout.toString().trim();
    } catch (error) {
      throw new Error(`创建目录失败: ${error}`);
    }
  }

  /**
   * 移动/重命名文件或目录（git mv）
   */
  async movePath(
    fromPath: string,
    toPath: string,
    message: string,
    author?: { name: string; email: string }
  ): Promise<string> {
    try {
      if (this.isBare) {
        const fromRel = this.normalizeRelPath(fromPath);
        const toRel = this.normalizeRelPath(toPath);

        // 先尝试作为文件
        const lsFile = Bun.spawn(['git', 'ls-tree', '-z', 'HEAD', '--', fromRel], {
          cwd: this.dir,
          stdout: 'pipe',
          stderr: 'pipe',
        });
        const out = new TextDecoder().decode(new Uint8Array(await new Response(lsFile.stdout).arrayBuffer()));
        await lsFile.exited;

        if (out && out.includes('\t')) {
          const rec = out.split('\0').filter(Boolean)[0];
          const tab = rec.indexOf('\t');
          const meta = rec.slice(0, tab).trim().split(/\s+/);
          const mode = meta[0] || '100644';
          const sha = meta[2];
          if (!sha) throw new Error('读取源文件失败');

          await this.updateIndexAddBlob(toRel, sha, mode);
          await this.updateIndexRemovePath(fromRel);
        } else {
          // 目录：递归列出其下全部文件（包含 sha）
          const proc = Bun.spawn(['git', 'ls-tree', '-r', '-z', 'HEAD', '--', fromRel], {
            cwd: this.dir,
            stdout: 'pipe',
            stderr: 'pipe',
          });
          const code = await proc.exited;
          if (code !== 0) {
            const err = await new Response(proc.stderr).text();
            throw new Error(err || '路径不存在');
          }
          const text = new TextDecoder().decode(new Uint8Array(await new Response(proc.stdout).arrayBuffer()));
          const records = text.split('\0').filter(Boolean);
          if (records.length === 0) throw new Error('路径不存在');

          for (const rec of records) {
            const tab = rec.indexOf('\t');
            if (tab <= 0) continue;
            const meta = rec.slice(0, tab).trim().split(/\s+/);
            const mode = meta[0] || '100644';
            const sha = meta[2];
            const name = rec.slice(tab + 1);
            if (!sha || !name) continue;

            // name 为 fromRel 下的相对路径或完整路径（git 返回的是相对仓库根的路径）
            const src = name;
            if (!src.startsWith(fromRel)) continue;
            const suffix = src.slice(fromRel.length).replace(/^\//, '');
            const dst = `${toRel}/${suffix}`;
            await this.updateIndexAddBlob(dst, sha, mode);
            await this.updateIndexRemovePath(src);
          }
        }

        const authorName = author?.name || 'VFiles User';
        const authorEmail = author?.email || 'user@vfiles.local';
        await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(this.dir);
        const result = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
        return result.stdout.toString().trim();
      }

      const fromFull = path.join(this.dir, fromPath);
      const toFull = path.join(this.dir, toPath);

      // 确保源存在
      await fs.stat(fromFull);

      // 确保目标父目录存在
      const toDir = path.dirname(toFull);
      await fs.mkdir(toDir, { recursive: true });

      await $`git mv ${normalizePathForGit(fromPath)} ${normalizePathForGit(toPath)}`.cwd(this.dir);

      const authorName = author?.name || 'VFiles User';
      const authorEmail = author?.email || 'user@vfiles.local';
      await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(
        this.dir
      );

      const result = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
      return result.stdout.toString().trim();
    } catch (error) {
      throw new Error(`移动/重命名失败: ${error}`);
    }
  }

  /**
   * 获取文件的提交历史
   */
  async getFileHistory(filePath: string, limit: number = 50): Promise<FileHistory> {
    try {
      // 使用控制字符作为分隔符，避免提交信息里出现 "||" 等导致解析错位
      const RS = '\x1e'; // record separator
      const US = '\x1f'; // unit separator

      const result = await $`git log --pretty=format:%H${US}%P${US}%an${US}%ae${US}%at${US}%s${RS} -n ${limit} -- ${normalizePathForGit(filePath)}`
        .cwd(this.dir)
        .quiet();

      const raw = result.stdout.toString();
      const records = raw
        .split(RS)
        .map((r: string) => r.trim())
        .filter(Boolean);

      const commitInfos: CommitInfo[] = [];
      for (const record of records) {
        const [hash, parentsStr, authorName, authorEmail, timestampStr, message] = record.split(US);
        if (!hash) continue;

        const timestamp = Number.parseInt(timestampStr, 10);
        if (!Number.isFinite(timestamp)) {
          continue;
        }

        const parents = (parentsStr || '').split(' ').filter(Boolean);

        commitInfos.push({
          hash,
          message: message ?? '',
          author: {
            name: authorName ?? '',
            email: authorEmail ?? '',
          },
          date: new Date(timestamp * 1000).toISOString(),
          parent: parents,
        });
      }

      return {
        commits: commitInfos,
        currentVersion: commitInfos[0]?.hash || '',
        totalCommits: commitInfos.length,
      };
    } catch (error) {
      console.error('获取文件历史失败:', error);
      return {
        commits: [],
        currentVersion: '',
        totalCommits: 0,
      };
    }
  }

  /**
   * 获取文件的最后一次提交信息
   */
  async getLastCommit(filePath: string): Promise<CommitSummary | undefined> {
    try {
      const US = '\x1f';
      const result = await $`git log --pretty=format:%H${US}%an${US}%at${US}%s -n 1 -- ${normalizePathForGit(filePath)}`
        .cwd(this.dir)
        .quiet();

      const line = result.stdout.toString().trim();
      if (!line) return undefined;

      const [hash, author, timestampStr, message] = line.split(US);
      const timestamp = Number.parseInt(timestampStr, 10);
      if (!hash || !Number.isFinite(timestamp)) return undefined;

      return {
        hash,
        message: message ?? '',
        author: author ?? '',
        date: new Date(timestamp * 1000).toISOString(),
      };
    } catch (error) {
      return undefined;
    }
  }

  /**
   * 获取某个版本的变更(diff)，用于文本文件最小 diff 视图。
   * - 若提供 parentHash，则返回 parent..commit 的 unified diff
   * - 否则返回该 commit 对该文件的 patch（git show）
   */
  async getFileDiff(filePath: string, commitHash: string, parentHash?: string): Promise<string> {
    try {
      const normalized = normalizePathForGit(filePath);

      if (parentHash) {
        const result = await $`git diff --unified=3 ${parentHash} ${commitHash} -- ${normalized}`
          .cwd(this.dir)
          .quiet();
        return result.stdout.toString();
      }

      const result = await $`git show --pretty=format: --unified=3 ${commitHash} -- ${normalized}`
        .cwd(this.dir)
        .quiet();
      return result.stdout.toString();
    } catch (error) {
      throw new Error(`获取 diff 失败: ${error}`);
    }
  }

  /**
   * 获取提交详情
   */
  async getCommitDetails(hash: string): Promise<CommitInfo> {
    try {
      const US = '\x1f';
      const result = await $`git log --pretty=format:%H${US}%P${US}%an${US}%ae${US}%at${US}%s -n 1 ${hash}`
        .cwd(this.dir)
        .quiet();

      const line = result.stdout.toString().trim();
      const [commitHash, parentsStr, authorName, authorEmail, timestampStr, message] = line.split(US);

      const timestamp = Number.parseInt(timestampStr, 10);
      if (!commitHash || !Number.isFinite(timestamp)) {
        throw new Error('提交信息解析失败');
      }

      const parents = (parentsStr || '').split(' ').filter(Boolean);

      return {
        hash: commitHash,
        message: message ?? '',
        author: {
          name: authorName ?? '',
          email: authorEmail ?? '',
        },
        date: new Date(timestamp * 1000).toISOString(),
        parent: parents,
      };
    } catch (error) {
      throw new Error(`获取提交详情失败: ${error}`);
    }
  }

  /**
   * 搜索文件
   */
  async searchFiles(query: string, basePath: string = ''): Promise<FileInfo[]> {
    const allFiles: FileInfo[] = [];

    const scanDirectory = async (dirPath: string = ''): Promise<void> => {
      const files = await this.listFiles(dirPath);

      for (const file of files) {
        if (file.name.toLowerCase().includes(query.toLowerCase())) {
          allFiles.push(file);
        }

        if (file.type === 'directory') {
          await scanDirectory(file.path);
        }
      }
    };

    await scanDirectory(basePath);
    return allFiles;
  }

  /**
   * 全文搜索（最小实现）：使用 git grep 找到包含关键词的文件，返回 FileInfo 列表。
   * 注意：使用 --fixed-strings 防止把用户输入当作正则。
   */
  async searchFileContents(query: string, basePath: string = ''): Promise<FileInfo[]> {
    // git grep 输出使用 / 作为分隔符；若指定 basePath 则限制在该目录范围内
    const MAX_FILES = 50;
    const MAX_MATCHES_PER_FILE = 5;
    const MAX_LINE_LENGTH = 240;

    const pathSpec = basePath ? normalizePathForGit(basePath) : '';

    let stdout = '';
    try {
      const result = pathSpec
        ? await $`git grep -n -I -m ${MAX_MATCHES_PER_FILE} --fixed-strings --ignore-case ${query} -- ${pathSpec}`
            .cwd(this.dir)
            .quiet()
        : await $`git grep -n -I -m ${MAX_MATCHES_PER_FILE} --fixed-strings --ignore-case ${query} --`
            .cwd(this.dir)
            .quiet();
      stdout = result.stdout.toString();
    } catch (error: any) {
      // git grep：无匹配时 exit code = 1（不是错误）
      const exitCode = typeof error?.exitCode === 'number' ? error.exitCode : undefined;
      if (exitCode === 1) {
        return [];
      }
      throw error;
    }

    type ContentMatch = { line: number; text: string };
    const matchesByFile = new Map<string, ContentMatch[]>();

    const lines = stdout
      .split(/\r?\n/)
      .map((s) => s.trimEnd())
      .filter(Boolean);

    for (const line of lines) {
      const first = line.indexOf(':');
      if (first <= 0) continue;
      const second = line.indexOf(':', first + 1);
      if (second <= first + 1) continue;

      const filePath = line.slice(0, first);
      if (!matchesByFile.has(filePath) && matchesByFile.size >= MAX_FILES) {
        continue;
      }

      const lineNoStr = line.slice(first + 1, second);
      const lineNo = Number.parseInt(lineNoStr, 10);
      if (!Number.isFinite(lineNo)) continue;

      const textRaw = line.slice(second + 1);
      const text = textRaw.length > MAX_LINE_LENGTH ? `${textRaw.slice(0, MAX_LINE_LENGTH)}…` : textRaw;

      const arr = matchesByFile.get(filePath) ?? [];
      if (arr.length < MAX_MATCHES_PER_FILE) {
        arr.push({ line: lineNo, text });
        matchesByFile.set(filePath, arr);
      }
    }

    const infos: FileInfo[] = [];
    for (const [filePath, matches] of matchesByFile) {
      try {
        const lastCommit = await this.getLastCommit(filePath);

        let size = 0;
        let mtime = lastCommit?.date || new Date(0).toISOString();
        if (this.isBare) {
          try {
            size = await this.getFileSizeAtCommit(filePath, 'HEAD');
          } catch {
            // ignore
          }
        } else {
          const fullPath = path.join(this.dir, filePath);
          const stats = await fs.stat(fullPath);
          size = stats.size;
          mtime = stats.mtime.toISOString();
        }

        infos.push({
          name: path.basename(filePath),
          path: filePath,
          type: 'file',
          size,
          mtime,
          lastCommit,
          matches,
        });
      } catch {
        // ignore missing/racing files
      }
    }

    return infos;
  }
}
