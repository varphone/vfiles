import { $} from 'bun';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { FileInfo, CommitInfo, FileHistory, CommitSummary } from '../types/index.js';
import { normalizePathForGit } from '../utils/path-validator.js';
import { config } from '../config.js';

export class GitService {
  private dir: string;
  private mode: 'worktree' | 'bare';
  private writeChain: Promise<unknown> = Promise.resolve();
  private bareWorkTreeDir: string | null = null;
  private gitDirPath: string | null = null;

  private cacheEnabled: boolean;
  private cacheDebug: boolean;
  private cacheTtlMs: number;
  private listFilesCacheMax: number;
  private fileHistoryCacheMax: number;
  private lastCommitCacheMax: number;

  private readonly listFilesCache = new Map<string, { token: string; at: number; value: FileInfo[] }>();
  private readonly fileHistoryCache = new Map<string, { token: string; at: number; value: FileHistory }>();
  private readonly lastCommitCache = new Map<
    string,
    { token: string; at: number; value: CommitSummary | undefined }
  >();
  private readonly inflight = new Map<string, Promise<unknown>>();

  constructor(repoPath: string, mode: 'worktree' | 'bare' = 'worktree') {
    this.dir = path.resolve(repoPath);
    this.mode = mode;

    const qc = config.gitQueryCache;
    this.cacheEnabled = qc?.enabled ?? true;
    this.cacheDebug = qc?.debug ?? false;
    this.cacheTtlMs = Number.isFinite(qc?.ttlMs) ? (qc!.ttlMs as number) : 5 * 60 * 1000;
    this.listFilesCacheMax = Number.isFinite(qc?.listFilesMax) ? (qc!.listFilesMax as number) : 300;
    this.fileHistoryCacheMax = Number.isFinite(qc?.fileHistoryMax) ? (qc!.fileHistoryMax as number) : 300;
    this.lastCommitCacheMax = Number.isFinite(qc?.lastCommitMax) ? (qc!.lastCommitMax as number) : 3000;
  }

  private isImmutableCommitish(commitish: string): boolean {
    // 视为不可变：纯十六进制 hash（至少 7 位）。
    // 分支名/tag 不可判定为不可变，因此不走该分支。
    return /^[0-9a-f]{7,40}$/i.test((commitish || '').trim());
  }

  private async resolveGitDirPath(): Promise<string> {
    if (this.isBare) return this.dir;
    if (this.gitDirPath) return this.gitDirPath;

    const dotGit = path.join(this.dir, '.git');
    try {
      const st = await fs.stat(dotGit);
      if (st.isDirectory()) {
        this.gitDirPath = dotGit;
        return dotGit;
      }

      if (st.isFile()) {
        // worktree 的 .git 可能是一个文本文件，内容类似：gitdir: /path/to/actual/gitdir
        const content = await fs.readFile(dotGit, 'utf-8');
        const m = content.match(/gitdir:\s*(.+)\s*/i);
        if (m && m[1]) {
          this.gitDirPath = path.resolve(this.dir, m[1].trim());
          return this.gitDirPath;
        }
      }
    } catch {
      // ignore
    }

    // 兜底：按标准路径返回
    this.gitDirPath = dotGit;
    return dotGit;
  }

  private async getRepoStateToken(): Promise<string> {
    // 仅使用文件系统信息生成 token，避免在“无变更”时也触发 git 子进程。
    // 该 token 用于缓存失效：只要 HEAD/ref/packed-refs（以及 worktree 的 index）没变，就认为仓库状态未变。
    const gitDir = await this.resolveGitDirPath();

    const parts: string[] = [];

    // HEAD
    let headText = '';
    try {
      headText = (await fs.readFile(path.join(gitDir, 'HEAD'), 'utf-8')).trim();
      parts.push(`HEAD=${headText}`);
    } catch {
      parts.push('HEAD=?');
    }

    // 当前 ref（若 HEAD 指向 refs/...）
    if (headText.startsWith('ref:')) {
      const ref = headText.slice('ref:'.length).trim();
      const refPath = path.join(gitDir, ref);
      try {
        const refText = (await fs.readFile(refPath, 'utf-8')).trim();
        parts.push(`REF=${ref}:${refText}`);
      } catch {
        // ref 可能被 pack 到 packed-refs
        parts.push(`REF=${ref}:packed`);
      }
    }

    // packed-refs
    try {
      const st = await fs.stat(path.join(gitDir, 'packed-refs'));
      parts.push(`PACKED=${st.mtimeMs}:${st.size}`);
    } catch {
      parts.push('PACKED=-');
    }

    // worktree index（bare 没有 index）
    if (!this.isBare) {
      try {
        const st = await fs.stat(path.join(gitDir, 'index'));
        parts.push(`INDEX=${st.mtimeMs}:${st.size}`);
      } catch {
        parts.push('INDEX=-');
      }
    }

    return parts.join('|');
  }

  private async getDirSnapshotToken(dirPath: string): Promise<string> {
    if (this.isBare) return 'DIR=-';
    try {
      const st = await fs.stat(path.join(this.dir, dirPath));
      return `DIR=${st.mtimeMs}:${st.size}`;
    } catch {
      return 'DIR=?';
    }
  }

  private async cached<T>(params: {
    cache: Map<string, { token: string; at: number; value: T }>;
    key: string;
    token: string;
    maxEntries: number;
    compute: () => Promise<T>;
  }): Promise<T> {
    if (!this.cacheEnabled) {
      return await params.compute();
    }

    const now = Date.now();
    const existing = params.cache.get(params.key);
    if (existing && existing.token === params.token && now - existing.at < this.cacheTtlMs) {
      // LRU：touch
      params.cache.delete(params.key);
      params.cache.set(params.key, existing);
      if (this.cacheDebug) {
        console.log(`[git-cache] hit ${params.key}`);
      }
      return existing.value;
    }

    if (this.cacheDebug) {
      console.log(`[git-cache] miss ${params.key}`);
    }

    const inflightKey = `${params.key}@@${params.token}`;
    const inflight = this.inflight.get(inflightKey) as Promise<T> | undefined;
    if (inflight) return inflight;

    const p = (async () => {
      const value = await params.compute();
      params.cache.delete(params.key);
      params.cache.set(params.key, { token: params.token, at: Date.now(), value });
      while (params.cache.size > params.maxEntries) {
        const firstKey = params.cache.keys().next().value as string | undefined;
        if (!firstKey) break;
        params.cache.delete(firstKey);
      }
      return value;
    })();

    this.inflight.set(inflightKey, p);
    try {
      return await p;
    } finally {
      this.inflight.delete(inflightKey);
    }
  }

  private get isBare(): boolean {
    // 兼容性：有些部署会只把 REPO_PATH 指向 data.git（bare gitdir），但忘了设置 REPO_MODE=bare。
    // 在这种情况下，运行依赖 worktree 的命令（如 git rm）会报 "this operation must be run in a work tree"。
    // 这里用路径后缀做兜底判定，避免误用 worktree 流程。
    return this.mode === 'bare' || path.extname(this.dir).toLowerCase() === '.git';
  }

  private async ensureBareWorkTreeDir(): Promise<string | null> {
    if (!this.isBare) return null;
    if (this.bareWorkTreeDir) return this.bareWorkTreeDir;

    const id = Buffer.from(this.dir).toString('hex').slice(0, 12) || 'default';
    const dir = path.join(os.tmpdir(), `vfiles-bare-worktree-${id}`);
    await fs.mkdir(dir, { recursive: true });
    this.bareWorkTreeDir = dir;
    return dir;
  }

  private withWriteLock<T>(fn: () => Promise<T>): Promise<T> {
    const run = async () => {
      try {
        return await fn();
      } finally {
        // 写操作会改变 repo state token；清空缓存避免旧 token 条目堆积
        this.clearCaches();
      }
    };

    const next = this.writeChain.then(run, run);
    this.writeChain = next.then(
      () => undefined,
      () => undefined
    );
    return next;
  }

  private clearCaches(): void {
    this.listFilesCache.clear();
    this.fileHistoryCache.clear();
    this.lastCommitCache.clear();
    this.inflight.clear();
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

  private async updateIndexAddBlob(
    filePath: string,
    blobSha: string,
    mode: string = '100644',
    indexFile?: string
  ): Promise<void> {
    const rel = this.normalizeRelPath(filePath);
    const bareWorkTree = await this.ensureBareWorkTreeDir();
    const proc = Bun.spawn(['git', 'update-index', '--add', '--cacheinfo', mode, blobSha, rel], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'pipe',
      env: {
        ...process.env,
        ...(indexFile ? { GIT_INDEX_FILE: indexFile } : {}),
        ...(bareWorkTree ? { GIT_DIR: this.dir, GIT_WORK_TREE: bareWorkTree } : {}),
      },
    });
    const code = await proc.exited;
    if (code !== 0) {
      const err = await new Response(proc.stderr).text();
      throw new Error(`git update-index 失败: ${err || code}`);
    }
  }

  private async updateIndexRemovePath(filePath: string, indexFile?: string): Promise<void> {
    const rel = this.normalizeRelPath(filePath);
    const bareWorkTree = await this.ensureBareWorkTreeDir();
    const proc = Bun.spawn(['git', 'update-index', '--remove', '--', rel], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'pipe',
      env: {
        ...process.env,
        ...(indexFile ? { GIT_INDEX_FILE: indexFile } : {}),
        ...(bareWorkTree ? { GIT_DIR: this.dir, GIT_WORK_TREE: bareWorkTree } : {}),
      },
    });
    const code = await proc.exited;
    if (code !== 0) {
      const err = await new Response(proc.stderr).text();
      throw new Error(`git update-index --remove 失败: ${err || code}`);
    }
  }

  private async getHeadRef(): Promise<string> {
    try {
      const result = await $`git symbolic-ref -q HEAD`.cwd(this.dir).quiet();
      const ref = result.stdout.toString().trim();
      return ref || 'refs/heads/master';
    } catch {
      return 'refs/heads/master';
    }
  }

  private async bareCommitFromIndex(params: {
    indexFile: string;
    message: string;
    authorName: string;
    authorEmail: string;
    parent?: string;
  }): Promise<string> {
    const env = {
      ...process.env,
      GIT_INDEX_FILE: params.indexFile,
      GIT_AUTHOR_NAME: params.authorName,
      GIT_AUTHOR_EMAIL: params.authorEmail,
      GIT_COMMITTER_NAME: params.authorName,
      GIT_COMMITTER_EMAIL: params.authorEmail,
    };

    // write-tree
    const writeTree = Bun.spawn(['git', 'write-tree'], { cwd: this.dir, stdout: 'pipe', stderr: 'pipe', env });
    const writeCode = await writeTree.exited;
    const tree = (await new Response(writeTree.stdout).text()).trim();
    if (writeCode !== 0 || !tree) {
      const err = await new Response(writeTree.stderr).text();
      throw new Error(`git write-tree 失败: ${err || writeCode}`);
    }

    // commit-tree
    const args = ['git', 'commit-tree', tree];
    if (params.parent) {
      args.push('-p', params.parent);
    }
    args.push('-m', params.message);
    const commitTree = Bun.spawn(args, { cwd: this.dir, stdout: 'pipe', stderr: 'pipe', env });
    const commitCode = await commitTree.exited;
    const commit = (await new Response(commitTree.stdout).text()).trim();
    if (commitCode !== 0 || !commit) {
      const err = await new Response(commitTree.stderr).text();
      throw new Error(`git commit-tree 失败: ${err || commitCode}`);
    }

    const headRef = await this.getHeadRef();
    const updateRef = Bun.spawn(['git', 'update-ref', headRef, commit], {
      cwd: this.dir,
      stdout: 'ignore',
      stderr: 'pipe',
      env: process.env,
    });
    const updateCode = await updateRef.exited;
    if (updateCode !== 0) {
      const err = await new Response(updateRef.stderr).text();
      throw new Error(`git update-ref 失败: ${err || updateCode}`);
    }

    return commit;
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

          // bare 下创建空初始提交（不依赖 worktree）
          const indexFile = path.join(this.dir, `.vfiles_index_init_${Date.now()}_${Math.random().toString(16).slice(2)}`);
          try {
            // empty index
            const readEmpty = Bun.spawn(['git', 'read-tree', '--empty'], {
              cwd: this.dir,
              stdout: 'ignore',
              stderr: 'pipe',
              env: { ...process.env, GIT_INDEX_FILE: indexFile },
            });
            const readCode = await readEmpty.exited;
            if (readCode !== 0) {
              const err = await new Response(readEmpty.stderr).text();
              throw new Error(err || 'git read-tree --empty 失败');
            }

            await this.bareCommitFromIndex({
              indexFile,
              message: 'Initial commit',
              authorName: 'VFiles System',
              authorEmail: 'system@vfiles.local',
            });
          } finally {
            try {
              await fs.rm(indexFile, { force: true });
            } catch {
              // ignore
            }
          }
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
    // bare 仓库没有 worktree，不做 install（否则会触发 "must be run in a work tree"）
    if (this.isBare) {
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
      return true;
    }

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
      await this.withWriteLock(async () => {
        const indexFile = path.join(this.dir, `.vfiles_index_${Date.now()}_${Math.random().toString(16).slice(2)}`);
        try {
          // 从 HEAD 载入到临时 index
          const readTree = Bun.spawn(['git', 'read-tree', 'HEAD'], {
            cwd: this.dir,
            stdout: 'ignore',
            stderr: 'pipe',
            env: { ...process.env, GIT_INDEX_FILE: indexFile },
          });
          const rc = await readTree.exited;
          if (rc !== 0) {
            const err = await new Response(readTree.stderr).text();
            throw new Error(err || 'git read-tree HEAD 失败');
          }

          await this.updateIndexAddBlob('.gitattributes', blobSha, '100644', indexFile);

          let parent: string | undefined;
          try {
            const p = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
            parent = p.stdout.toString().trim() || undefined;
          } catch {
            parent = undefined;
          }

          await this.bareCommitFromIndex({
            indexFile,
            message: 'chore: configure git-lfs',
            authorName: 'VFiles System',
            authorEmail: 'system@vfiles.local',
            parent,
          });
        } finally {
          try {
            await fs.rm(indexFile, { force: true });
          } catch {
            // ignore
          }
        }
      });
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
  async listFiles(dirPath: string = '', commitHash?: string): Promise<FileInfo[]> {
    const normalizedDir = dirPath ? normalizePathForGit(dirPath) : '';
    const repoToken = commitHash
      ? (this.isImmutableCommitish(commitHash) ? `COMMIT=${commitHash}` : await this.getRepoStateToken())
      : await this.getRepoStateToken();
    const dirToken = !commitHash && !this.isBare ? await this.getDirSnapshotToken(dirPath) : '';
    const token = dirToken ? `${repoToken}|${dirToken}` : repoToken;
    const cacheKey = `listFiles|${commitHash || (this.isBare ? 'HEAD(bare)' : 'worktree')}|${normalizedDir}`;

    return await this.cached({
      cache: this.listFilesCache,
      key: cacheKey,
      token,
      maxEntries: this.listFilesCacheMax,
      compute: async () => {
    if (commitHash) {
      const normalizedDir = dirPath ? normalizePathForGit(dirPath) : '';
      const treeish = normalizedDir ? `${commitHash}:${normalizedDir}` : commitHash;
      const args = ['git', 'ls-tree', '-z', '-l', treeish];

      const proc = Bun.spawn(args, { cwd: this.dir, stdout: 'pipe', stderr: 'pipe' });
      const code = await proc.exited;
      const outBuf = await new Response(proc.stdout).arrayBuffer();

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
        if (name.startsWith('.git')) continue;

        const typeToken = meta[1];
        const sizeToken = meta[3];

        const relPath = normalizedDir ? `${normalizedDir}/${name}` : name;
        const filePath = relPath.replaceAll('\\', '/');

        // 目录版本浏览：优先保证能列出内容；lastCommit 可先沿用现有逻辑
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

    if (this.isBare) {
      // 从 HEAD 的 tree 列出目录项
      const normalizedDir = dirPath ? normalizePathForGit(dirPath) : '';
      // 注意：`git ls-tree HEAD -- <dir>` 在 <dir> 是 tree 时会返回“dir 自身”这一条目，
      // 这会导致空目录被显示为自己的子目录（dir/dir）。
      // 使用 `HEAD:<dir>` 才能列出目录内容。
      const treeish = normalizedDir ? `HEAD:${normalizedDir}` : 'HEAD';
      const args = ['git', 'ls-tree', '-z', '-l', treeish];

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
        if (name.startsWith('.git')) continue;

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
        // 跳过 .git* 相关文件/目录（例如 .git, .gitignore, .gitattributes）
        if (entry.name.startsWith('.git')) continue;

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
      },
    });
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
   * 获取某个 commit 下指定路径的对象类型（blob/tree）。不存在则返回 null。
   */
  private async getObjectTypeAtCommit(
    filePath: string,
    commitHash: string
  ): Promise<'blob' | 'tree' | null> {
    const spec = `${commitHash}:${normalizePathForGit(filePath)}`;
    const proc = Bun.spawn(['git', 'cat-file', '-t', spec], {
      cwd: this.dir,
      stdout: 'pipe',
      stderr: 'ignore',
    });
    const code = await proc.exited;
    if (code !== 0) return null;
    const text = (await new Response(proc.stdout).text()).trim();
    if (text === 'blob' || text === 'tree') return text;
    return null;
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
        return await this.withWriteLock(async () => {
          const blobSha = await this.writeBlobFromContent(content);
          const indexFile = path.join(this.dir, `.vfiles_index_${Date.now()}_${Math.random().toString(16).slice(2)}`);
          try {
            // 从 HEAD 载入到临时 index
            const readTree = Bun.spawn(['git', 'read-tree', 'HEAD'], {
              cwd: this.dir,
              stdout: 'ignore',
              stderr: 'pipe',
              env: { ...process.env, GIT_INDEX_FILE: indexFile },
            });
            const rc = await readTree.exited;
            if (rc !== 0) {
              const err = await new Response(readTree.stderr).text();
              throw new Error(err || 'git read-tree HEAD 失败');
            }

            await this.updateIndexAddBlob(filePath, blobSha, '100644', indexFile);

            const authorName = author?.name || 'VFiles User';
            const authorEmail = author?.email || 'user@vfiles.local';

            const parentResult = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
            const parent = parentResult.stdout.toString().trim();
            const commit = await this.bareCommitFromIndex({
              indexFile,
              message,
              authorName,
              authorEmail,
              parent,
            });
            return commit;
          } finally {
            try {
              await fs.rm(indexFile, { force: true });
            } catch {
              // ignore
            }
          }
        });
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
        await this.withWriteLock(async () => {
          const rel = this.normalizeRelPath(filePath);
          const indexFile = path.join(this.dir, `.vfiles_index_${Date.now()}_${Math.random().toString(16).slice(2)}`);
          try {
            const readTree = Bun.spawn(['git', 'read-tree', 'HEAD'], {
              cwd: this.dir,
              stdout: 'ignore',
              stderr: 'pipe',
              env: { ...process.env, GIT_INDEX_FILE: indexFile },
            });
            const rc = await readTree.exited;
            if (rc !== 0) {
              const err = await new Response(readTree.stderr).text();
              throw new Error(err || 'git read-tree HEAD 失败');
            }

            const objType = await this.getObjectTypeAtCommit(rel, 'HEAD');
            if (objType === 'blob') {
              await this.updateIndexRemovePath(rel, indexFile);
            } else if (objType === 'tree') {
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
              const text = new TextDecoder().decode(
                new Uint8Array(await new Response(proc.stdout).arrayBuffer())
              );
              const files = text.split('\0').filter(Boolean);
              if (files.length === 0) throw new Error('路径不存在');
              for (const f of files) {
                await this.updateIndexRemovePath(f, indexFile);
              }
            } else {
              throw new Error('路径不存在');
            }

            const authorName = author?.name || 'VFiles User';
            const authorEmail = author?.email || 'user@vfiles.local';
            const parentResult = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
            const parent = parentResult.stdout.toString().trim();
            await this.bareCommitFromIndex({
              indexFile,
              message,
              authorName,
              authorEmail,
              parent,
            });
          } finally {
            try {
              await fs.rm(indexFile, { force: true });
            } catch {
              // ignore
            }
          }
        });
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
        return await this.withWriteLock(async () => {
          const keepRel = path.join(dirPath, '.gitkeep').replaceAll('\\', '/');
          const blobSha = await this.writeBlobFromContent(new Uint8Array(0));
          const indexFile = path.join(this.dir, `.vfiles_index_${Date.now()}_${Math.random().toString(16).slice(2)}`);
          try {
            const readTree = Bun.spawn(['git', 'read-tree', 'HEAD'], {
              cwd: this.dir,
              stdout: 'ignore',
              stderr: 'pipe',
              env: { ...process.env, GIT_INDEX_FILE: indexFile },
            });
            const rc = await readTree.exited;
            if (rc !== 0) {
              const err = await new Response(readTree.stderr).text();
              throw new Error(err || 'git read-tree HEAD 失败');
            }

            await this.updateIndexAddBlob(keepRel, blobSha, '100644', indexFile);

            const authorName = author?.name || 'VFiles User';
            const authorEmail = author?.email || 'user@vfiles.local';
            const parentResult = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
            const parent = parentResult.stdout.toString().trim();
            const commit = await this.bareCommitFromIndex({
              indexFile,
              message,
              authorName,
              authorEmail,
              parent,
            });
            return commit;
          } finally {
            try {
              await fs.rm(indexFile, { force: true });
            } catch {
              // ignore
            }
          }
        });
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
        return await this.withWriteLock(async () => {
          const fromRel = this.normalizeRelPath(fromPath);
          const toRel = this.normalizeRelPath(toPath);
          const indexFile = path.join(this.dir, `.vfiles_index_${Date.now()}_${Math.random().toString(16).slice(2)}`);
          try {
            const readTree = Bun.spawn(['git', 'read-tree', 'HEAD'], {
              cwd: this.dir,
              stdout: 'ignore',
              stderr: 'pipe',
              env: { ...process.env, GIT_INDEX_FILE: indexFile },
            });
            const rc = await readTree.exited;
            if (rc !== 0) {
              const err = await new Response(readTree.stderr).text();
              throw new Error(err || 'git read-tree HEAD 失败');
            }

            // 先尝试作为文件
            const lsFile = Bun.spawn(['git', 'ls-tree', '-z', 'HEAD', '--', fromRel], {
              cwd: this.dir,
              stdout: 'pipe',
              stderr: 'pipe',
            });
            const out = new TextDecoder().decode(
              new Uint8Array(await new Response(lsFile.stdout).arrayBuffer())
            );
            await lsFile.exited;

            if (out && out.includes('\t')) {
              const rec = out.split('\0').filter(Boolean)[0];
              const tab = rec.indexOf('\t');
              const meta = rec.slice(0, tab).trim().split(/\s+/);
              const mode = meta[0] || '100644';
              const sha = meta[2];
              if (!sha) throw new Error('读取源文件失败');

              await this.updateIndexAddBlob(toRel, sha, mode, indexFile);
              await this.updateIndexRemovePath(fromRel, indexFile);
            } else {
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
              const text = new TextDecoder().decode(
                new Uint8Array(await new Response(proc.stdout).arrayBuffer())
              );
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
                const src = name;
                if (!src.startsWith(fromRel)) continue;
                const suffix = src.slice(fromRel.length).replace(/^\//, '');
                const dst = `${toRel}/${suffix}`;
                await this.updateIndexAddBlob(dst, sha, mode, indexFile);
                await this.updateIndexRemovePath(src, indexFile);
              }
            }

            const authorName = author?.name || 'VFiles User';
            const authorEmail = author?.email || 'user@vfiles.local';
            const parentResult = await $`git rev-parse HEAD`.cwd(this.dir).quiet();
            const parent = parentResult.stdout.toString().trim();
            const commit = await this.bareCommitFromIndex({
              indexFile,
              message,
              authorName,
              authorEmail,
              parent,
            });
            return commit;
          } finally {
            try {
              await fs.rm(indexFile, { force: true });
            } catch {
              // ignore
            }
          }
        });
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
    const normalized = normalizePathForGit(filePath);
    const token = await this.getRepoStateToken();
    const cacheKey = `fileHistory|${normalized}|${limit}`;

    return await this.cached({
      cache: this.fileHistoryCache,
      key: cacheKey,
      token,
      maxEntries: this.fileHistoryCacheMax,
      compute: async () => {
        try {
          // 使用控制字符作为分隔符，避免提交信息里出现 "||" 等导致解析错位
          const RS = '\x1e'; // record separator
          const US = '\x1f'; // unit separator

          const cmd = normalized
            ? $`git log --pretty=format:%H${US}%P${US}%an${US}%ae${US}%at${US}%s${RS} -n ${limit} -- ${normalized}`
            : $`git log --pretty=format:%H${US}%P${US}%an${US}%ae${US}%at${US}%s${RS} -n ${limit}`;

          const result = await cmd.cwd(this.dir).quiet();

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
      },
    });
  }

  /**
   * 获取文件的最后一次提交信息
   */
  async getLastCommit(filePath: string): Promise<CommitSummary | undefined> {
    const normalized = normalizePathForGit(filePath);
    const token = await this.getRepoStateToken();
    const cacheKey = `lastCommit|${normalized}`;

    return await this.cached({
      cache: this.lastCommitCache,
      key: cacheKey,
      token,
      maxEntries: this.lastCommitCacheMax,
      compute: async () => {
        try {
          const US = '\x1f';
          const result = await $`git log --pretty=format:%H${US}%an${US}%at${US}%s -n 1 -- ${normalized}`
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
        } catch {
          return undefined;
        }
      },
    });
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
