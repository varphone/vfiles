import { GitService } from "./git.service.js";

export class GitServiceManager {
  private readonly services = new Map<
    string,
    { service: GitService; ready: Promise<void> }
  >();

  async get(repoPath: string, mode: "worktree" | "bare"): Promise<GitService> {
    const key = `${mode}:${repoPath}`;
    const existing = this.services.get(key);
    if (existing) {
      await existing.ready;
      return existing.service;
    }

    const service = new GitService(repoPath, mode);
    const ready = service.initRepo();
    this.services.set(key, { service, ready });
    await ready;
    return service;
  }
}
