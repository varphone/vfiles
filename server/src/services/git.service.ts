import { $} from 'bun';
import fs from 'node:fs/promises';
import path from 'node:path';
import { FileInfo, CommitInfo, FileHistory, CommitSummary } from '../types/index.js';
import { normalizePathForGit } from '../utils/path-validator.js';

export class GitService {
  private dir: string;

  constructor(repoPath: string) {
    this.dir = path.resolve(repoPath);
  }

  /**
   * 初始化Git仓库（如果不存在）
   */
  async initRepo(): Promise<void> {
    try {
      // 确保仓库目录存在（克隆后 data/ 可能不存在）
      await fs.mkdir(this.dir, { recursive: true });

      // 检查是否已存在Git仓库
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
    } catch (error) {
      console.error('初始化Git仓库失败:', error);
      throw error;
    }
  }

  /**
   * 创建初始提交
   */
  private async createInitialCommit(): Promise<void> {
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
        const fullPath = path.join(this.dir, filePath);
        return await fs.readFile(fullPath);
      }
    } catch (error) {
      throw new Error(`读取文件失败: ${error}`);
    }
  }

  /**
   * 保存文件并提交到Git
   */
  async saveFile(
    filePath: string,
    content: Buffer,
    message: string,
    author?: { name: string; email: string }
  ): Promise<string> {
    try {
      const fullPath = path.join(this.dir, filePath);
      const dirPath = path.dirname(fullPath);

      // 确保目录存在
      await fs.mkdir(dirPath, { recursive: true });

      // 写入文件
      await fs.writeFile(fullPath, content);

      // 添加到Git
      await $`git add ${normalizePathForGit(filePath)}`.cwd(this.dir);

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
   * 删除文件
   */
  async deleteFile(
    filePath: string,
    message: string,
    author?: { name: string; email: string }
  ): Promise<void> {
    try {
      const fullPath = path.join(this.dir, filePath);

      // 删除文件
      await fs.unlink(fullPath);

      // 从Git中移除
      await $`git rm ${normalizePathForGit(filePath)}`.cwd(this.dir);

      // 提交
      const authorName = author?.name || 'VFiles User';
      const authorEmail = author?.email || 'user@vfiles.local';
      
      await $`git -c user.name="${authorName}" -c user.email="${authorEmail}" commit -m "${message}"`.cwd(this.dir);
    } catch (error) {
      throw new Error(`删除文件失败: ${error}`);
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
  async searchFiles(query: string): Promise<FileInfo[]> {
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

    await scanDirectory();
    return allFiles;
  }

  /**
   * 全文搜索（最小实现）：使用 git grep 找到包含关键词的文件，返回 FileInfo 列表。
   * 注意：使用 --fixed-strings 防止把用户输入当作正则。
   */
  async searchFileContents(query: string): Promise<FileInfo[]> {
    // git grep 输出使用 / 作为分隔符
    const result = await $`git grep -I -l --fixed-strings --ignore-case ${query} --`
      .cwd(this.dir)
      .quiet();

    const files = result.stdout
      .toString()
      .split(/\r?\n/)
      .map((s: string) => s.trim())
      .filter(Boolean);

    const infos: FileInfo[] = [];
    for (const filePath of files) {
      try {
        const fullPath = path.join(this.dir, filePath);
        const stats = await fs.stat(fullPath);
        const lastCommit = await this.getLastCommit(filePath);

        infos.push({
          name: path.basename(filePath),
          path: filePath,
          type: 'file',
          size: stats.size,
          mtime: stats.mtime.toISOString(),
          lastCommit,
        });
      } catch {
        // ignore missing/racing files
      }
    }

    return infos;
  }
}
