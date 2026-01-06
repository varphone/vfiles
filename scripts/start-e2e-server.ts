import os from "node:os";
import fs from "node:fs";
import path from "node:path";
import { execFileSync, spawn } from "node:child_process";

function log(...args: unknown[]) {
  console.log("[e2e-server]", ...args);
}

async function main() {
  const baseTmp = fs.mkdtempSync(path.join(os.tmpdir(), "vfiles-e2e-"));
  const repoPath = path.join(baseTmp, "data");
  const uploadTempDir = path.join(baseTmp, "uploads");
  fs.mkdirSync(repoPath, { recursive: true });
  fs.mkdirSync(uploadTempDir, { recursive: true });

  log("Initializing worktree repo at", repoPath);
  execFileSync("git", ["init"], { cwd: repoPath, stdio: "inherit" });
  fs.writeFileSync(path.join(repoPath, "README.md"), "repo");
  execFileSync("git", ["add", "."], { cwd: repoPath });
  execFileSync(
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
    { cwd: repoPath },
  );

  log("Building client...");
  execFileSync("bun", ["run", "build"], { stdio: "inherit" });

  log("Starting server with REPO_PATH=", repoPath);
  const env = {
    ...process.env,
    NODE_ENV: "production",
    PORT: "3000",
    REPO_PATH: repoPath,
    REPO_MODE: "worktree",
    ENABLE_GIT_LFS: "false", // Disable Git LFS in e2e tests to avoid synchronous blocking during init
    UPLOAD_TEMP_DIR: uploadTempDir, // Use isolated temp dir for uploads
  };

  const server = spawn("bun", ["run", "start"], {
    stdio: "inherit",
    env,
    shell: false,
  });

  server.on("exit", (code, signal) => {
    log("server exited", { code, signal });
    process.exit(code ?? 0);
  });

  process.on("SIGINT", () => {
    server.kill("SIGINT");
    process.exit(0);
  });
  process.on("SIGTERM", () => {
    server.kill("SIGTERM");
    process.exit(0);
  });
}

main().catch((err) => {
  console.error("[e2e-server] failed to start", err);
  process.exit(1);
});
