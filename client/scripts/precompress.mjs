// 构建期预压缩：为 dist 下的文本资源生成 .br / .gz 兄弟文件。
//
// 服务端（vfiles-http）在有对应文件且客户端支持时直接返回预压缩副本，
// 从而省去每个请求的运行时压缩 CPU，并能带上准确的 Content-Length。
// 非文本资源（图片、字体、.map 等）跳过，避免无意义产物。
import { promises as fs } from "node:fs";
import path from "node:path";
import {
  brotliCompressSync,
  gzipSync,
  constants as zlibConstants,
} from "node:zlib";

const DIST_DIR = path.resolve(process.cwd(), "dist");
const MIN_SIZE = 1024;
const COMPRESSIBLE_EXTENSIONS = new Set([
  ".js",
  ".mjs",
  ".css",
  ".html",
  ".json",
  ".svg",
  ".txt",
  ".webmanifest",
]);

async function walk(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await walk(full)));
    } else if (entry.isFile()) {
      files.push(full);
    }
  }
  return files;
}

async function main() {
  let processed = 0;
  let rawBytes = 0;
  let brBytes = 0;
  let gzBytes = 0;

  for (const file of await walk(DIST_DIR)) {
    const ext = path.extname(file).toLowerCase();
    if (!COMPRESSIBLE_EXTENSIONS.has(ext)) continue;
    if (file.endsWith(".br") || file.endsWith(".gz")) continue;

    const source = await fs.readFile(file);
    if (source.byteLength < MIN_SIZE) continue;

    const brotli = brotliCompressSync(source, {
      params: {
        [zlibConstants.BROTLI_PARAM_QUALITY]: 11,
        [zlibConstants.BROTLI_PARAM_SIZE_HINT]: source.byteLength,
      },
    });
    const gzip = gzipSync(source, { level: 9 });

    // 只有确实更小才落盘，避免出现比原文还大的“压缩”文件。
    if (brotli.byteLength < source.byteLength) {
      await fs.writeFile(`${file}.br`, brotli);
      brBytes += brotli.byteLength;
    }
    if (gzip.byteLength < source.byteLength) {
      await fs.writeFile(`${file}.gz`, gzip);
      gzBytes += gzip.byteLength;
    }

    processed += 1;
    rawBytes += source.byteLength;
  }

  const kb = (bytes) => `${(bytes / 1024).toFixed(1)} KB`;
  console.log(
    `precompress: ${processed} files, raw ${kb(rawBytes)} -> br ${kb(brBytes)} / gz ${kb(gzBytes)}`,
  );
}

main().catch((error) => {
  console.error("precompress failed:", error);
  process.exit(1);
});
