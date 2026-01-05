import { Hono } from "hono";
import { GitServiceManager } from "../services/git-service-manager.js";
import { validateRequiredString } from "../utils/validation.js";
import { pathSecurityMiddleware } from "../middleware/security.js";
import { normalizeRequestPath } from "../utils/validation.js";
import { getRepoContext } from "../middleware/repo-context.js";

export function createSearchRoutes(gitManager: GitServiceManager) {
  const app = new Hono();

  /**
   * GET /api/search - 搜索文件
   */
  app.get("/", pathSecurityMiddleware, async (c) => {
    const { repoPath, repoMode } = getRepoContext(c);
    const gitService = await gitManager.get(repoPath, repoMode);

    const queryResult = validateRequiredString(c.req.query("q"), "q", {
      minLength: 1,
      maxLength: 100,
    });
    if (!queryResult.ok) {
      return c.json(
        { success: false, error: queryResult.message },
        queryResult.status,
      );
    }

    const mode = (c.req.query("mode") || "name").toLowerCase();
    const type = (c.req.query("type") || "all").toLowerCase();
    const rawPath = c.req.query("path") || "";
    const basePath = normalizeRequestPath(rawPath);

    try {
      const results =
        mode === "content"
          ? await gitService.searchFileContents(queryResult.value, basePath)
          : await gitService.searchFiles(queryResult.value, basePath);

      const filtered =
        type === "file"
          ? results.filter((r) => r.type === "file")
          : type === "directory"
            ? results.filter((r) => r.type === "directory")
            : results;

      return c.json({
        success: true,
        data: filtered,
      });
    } catch (error) {
      return c.json(
        {
          success: false,
          error: error instanceof Error ? error.message : "搜索失败",
        },
        500,
      );
    }
  });

  return app;
}
