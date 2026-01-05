import fs from "node:fs/promises";
import path from "node:path";
import { zipSync } from "fflate";

async function pathExists(p: string): Promise<boolean> {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

async function rmIfExists(p: string): Promise<void> {
  if (await pathExists(p)) {
    await fs.rm(p, { recursive: true, force: true });
  }
}

async function copyDir(src: string, dest: string): Promise<void> {
  await fs.mkdir(dest, { recursive: true });
  // Node 16+ supports fs.cp; Bun implements it.
  // Fallback logic is not added to keep script minimal.
  await fs.cp(src, dest, { recursive: true });
}

async function copyFileIfExists(src: string, dest: string): Promise<void> {
  if (!(await pathExists(src))) return;
  await fs.mkdir(path.dirname(dest), { recursive: true });
  await fs.copyFile(src, dest);
}

async function listFilesRecursively(baseDir: string): Promise<string[]> {
  const out: string[] = [];
  async function walk(rel: string) {
    const full = path.join(baseDir, rel);
    const entries = await fs.readdir(full, { withFileTypes: true });
    for (const entry of entries) {
      const childRel = rel ? path.join(rel, entry.name) : entry.name;
      if (entry.isDirectory()) {
        await walk(childRel);
      } else if (entry.isFile()) {
        out.push(childRel);
      } else {
        // ignore symlinks and others
      }
    }
  }
  await walk("");
  return out;
}

function toPosixPath(p: string): string {
  return p.split(path.sep).join("/");
}

async function makeZipFromDir(inputDir: string, outZipPath: string) {
  const relFiles = await listFilesRecursively(inputDir);
  const entries: Record<string, Uint8Array> = {};
  for (const rel of relFiles) {
    const full = path.join(inputDir, rel);
    const buf = await fs.readFile(full);
    entries[toPosixPath(rel)] = new Uint8Array(buf);
  }

  const zipped = zipSync(entries, {
    level: 9,
  });
  await fs.mkdir(path.dirname(outZipPath), { recursive: true });
  await fs.writeFile(outZipPath, zipped);
}

async function run(cmd: string[], cwd: string): Promise<void> {
  const proc = Bun.spawn(cmd, {
    cwd,
    stdout: "inherit",
    stderr: "inherit",
  });
  const code = await proc.exited;
  if (code !== 0) throw new Error(`Command failed (${code}): ${cmd.join(" ")}`);
}

async function makeTarGzFromDir(
  inputDir: string,
  outTarGzPath: string,
): Promise<void> {
  await fs.mkdir(path.dirname(outTarGzPath), { recursive: true });
  await run(["tar", "-czf", outTarGzPath, "-C", inputDir, "."], process.cwd());
}

function normalizeOs(os: string): string {
  if (os === "win32") return "windows";
  if (os === "darwin") return "macos";
  return os;
}

function normalizeArch(arch: string): string {
  if (arch === "x64") return "x64";
  if (arch === "arm64") return "arm64";
  return arch;
}

async function main() {
  const root = process.cwd();
  const distDir = path.join(root, "dist");
  const releaseDir = path.join(root, "release");

  // 1) Build client
  await run(["bun", "run", "build"], root);

  // 2) Compile server
  await fs.mkdir(distDir, { recursive: true });
  await run(
    [
      "bun",
      "build",
      "--compile",
      "server/src/index.ts",
      "--minify",
      "--sourcemap=none",
      "--outfile",
      "dist/vfiles",
    ],
    root,
  );

  const exeWin = path.join(distDir, "vfiles.exe");
  const exePosix = path.join(distDir, "vfiles");
  const exePath = (await pathExists(exeWin)) ? exeWin : exePosix;
  if (!(await pathExists(exePath))) {
    throw new Error(
      "Compiled server binary not found in dist/. Expected dist/vfiles(.exe)",
    );
  }

  // 3) Assemble release directory
  await rmIfExists(releaseDir);
  await fs.mkdir(releaseDir, { recursive: true });

  const releaseBinName = path.basename(exePath);
  const releaseBinPath = path.join(releaseDir, releaseBinName);
  await fs.copyFile(exePath, releaseBinPath);
  if (process.platform !== "win32") {
    await fs.chmod(releaseBinPath, 0o755);
  }

  const clientDistSrc = path.join(root, "client", "dist");
  const clientDistDest = path.join(releaseDir, "client", "dist");
  if (!(await pathExists(clientDistSrc))) {
    throw new Error("client/dist not found. Did the client build succeed?");
  }
  await copyDir(clientDistSrc, clientDistDest);

  // Optional templates
  await copyFileIfExists(
    path.join(root, ".env.example"),
    path.join(releaseDir, ".env.example"),
  );

  // 4) Create GitHub-release-style release asset
  const pkgJson = JSON.parse(
    await fs.readFile(path.join(root, "package.json"), "utf-8"),
  ) as { name?: string; version?: string };
  const baseName = (pkgJson.name || "vfiles")
    .replace(/^@/, "")
    .replaceAll("/", "-");
  const version = pkgJson.version || "0.0.0";
  const os = normalizeOs(process.platform);
  const arch = normalizeArch(process.arch);
  const isWindows = process.platform === "win32";
  const assetName = isWindows
    ? `${baseName}-v${version}-${os}-${arch}.zip`
    : `${baseName}-v${version}-${os}-${arch}.tar.gz`;
  const assetPath = path.join(distDir, assetName);
  if (isWindows) {
    await makeZipFromDir(releaseDir, assetPath);
  } else {
    await makeTarGzFromDir(releaseDir, assetPath);
  }

  console.log(`\nRelease assembled at: ${releaseDir}`);
  console.log(`Binary: ${path.join(releaseDir, releaseBinName)}`);
  console.log(`Client dist: ${clientDistDest}`);
  console.log(`ASSET: ${assetPath}`);
}

await main();
