import path from "node:path";

// 服务器配置
export const config = {
  // 服务器端口
  port: parseInt(process.env.PORT || "3000"),

  // Git仓库路径
  repoPath: process.env.REPO_PATH || path.resolve(process.cwd(), "data"),

  // 仓库模式：
  // - worktree：repoPath 为工作目录（含 .git），文件在磁盘上可见（可能产生“工作区文件 + git 对象库”双份占用）
  // - bare：repoPath 为 bare gitdir（无工作区文件），文件仅存于 git 对象库，避免双份占用
  repoMode: ((process.env.REPO_MODE || "worktree").toLowerCase() === "bare"
    ? "bare"
    : "worktree") as "worktree" | "bare",

  // 上传限制
  maxFileSize: 1024 * 1024 * 1024, // 1GB

  // 分块上传（用于绕过单次请求体大小限制）
  // 单块大小（默认 5MB，可通过 CDN 限制调整）
  uploadChunkSize: parseInt(
    process.env.UPLOAD_CHUNK_SIZE || String(5 * 1024 * 1024),
  ),
  // 单块最大允许大小（防止客户端随意放大 chunk）
  uploadMaxChunkSize: parseInt(
    process.env.UPLOAD_MAX_CHUNK_SIZE || String(20 * 1024 * 1024),
  ),
  // 分块上传临时目录（必须在 repoPath 之外，避免被当作文件列表展示/提交）
  uploadTempDir:
    process.env.UPLOAD_TEMP_DIR ||
    path.resolve(process.cwd(), ".vfiles_uploads"),
  // 上传会话过期时间（默认 24h）
  uploadSessionTtlMs: parseInt(
    process.env.UPLOAD_SESSION_TTL_MS || String(24 * 60 * 60 * 1000),
  ),

  // 下载缓存（用于历史版本 + Git LFS smudge 后的临时落地，从而支持 Range/断点续传）
  downloadCacheDir:
    process.env.DOWNLOAD_CACHE_DIR ||
    path.resolve(process.cwd(), ".vfiles_download_cache"),
  // 缓存过期时间（默认 6h）
  downloadCacheTtlMs: parseInt(
    process.env.DOWNLOAD_CACHE_TTL_MS || String(6 * 60 * 60 * 1000),
  ),

  // Git 查询缓存（减少在仓库未变更时重复调用 git 子进程）
  gitQueryCache: {
    enabled:
      (process.env.GIT_QUERY_CACHE_ENABLED || "true").toLowerCase() !== "false",
    // 默认 5 分钟；token 未变且未过期则命中
    ttlMs: parseInt(
      process.env.GIT_QUERY_CACHE_TTL_MS || String(5 * 60 * 1000),
    ),
    // listFiles 结果缓存条目上限
    listFilesMax: parseInt(process.env.GIT_QUERY_CACHE_LIST_FILES_MAX || "300"),
    // getFileHistory 结果缓存条目上限
    fileHistoryMax: parseInt(
      process.env.GIT_QUERY_CACHE_FILE_HISTORY_MAX || "300",
    ),
    // getLastCommit 结果缓存条目上限
    lastCommitMax: parseInt(
      process.env.GIT_QUERY_CACHE_LAST_COMMIT_MAX || "3000",
    ),
    // debug=true 时输出 hit/miss 日志（可能比较啰嗦）
    debug:
      (process.env.GIT_QUERY_CACHE_DEBUG || "false").toLowerCase() === "true",
  },

  // Git LFS
  enableGitLfs:
    (process.env.ENABLE_GIT_LFS || "true").toLowerCase() !== "false",
  // 以逗号分隔的 glob 列表（默认覆盖常见二进制/大文件类型）
  gitLfsTrackPatterns: (
    process.env.GIT_LFS_TRACK_PATTERNS ||
    [
      // 图像
      "*.png",
      "*.jpg",
      "*.jpeg",
      "*.gif",
      "*.webp",
      "*.bmp",
      "*.ico",
      "*.heic",
      "*.heif",
      "*.avif",
      "*.tiff",
      "*.tif",
      "*.svg", // SVG 可能很大（复杂矢量图）
      "*.raw",
      "*.cr2",
      "*.nef",
      "*.arw",
      // 视频
      "*.mp4",
      "*.mov",
      "*.m4v",
      "*.webm",
      "*.avi",
      "*.mkv",
      "*.wmv",
      "*.flv",
      "*.3gp",
      "*.ts",
      "*.mts",
      // 音频
      "*.mp3",
      "*.wav",
      "*.flac",
      "*.aac",
      "*.m4a",
      "*.ogg",
      "*.opus",
      "*.wma",
      "*.ape",
      // 压缩包
      "*.zip",
      "*.7z",
      "*.rar",
      "*.tar",
      "*.gz",
      "*.bz2",
      "*.xz",
      "*.zst",
      "*.lz4",
      "*.br",
      "*.tgz",
      "*.tbz2",
      // 文档
      "*.pdf",
      "*.docx",
      "*.xlsx",
      "*.pptx",
      "*.epub",
      "*.mobi",
      // 设计文件
      "*.psd",
      "*.ai",
      "*.sketch",
      "*.fig",
      "*.xd",
      // 可执行/二进制
      "*.exe",
      "*.dll",
      "*.so",
      "*.dylib",
      "*.bin",
      "*.apk",
      "*.aab",
      "*.ipa",
      "*.dmg",
      "*.iso",
      "*.msi",
      // 字体
      "*.ttf",
      "*.otf",
      "*.woff",
      "*.woff2",
      "*.eot",
      // 数据库/数据
      "*.db",
      "*.sqlite",
      "*.sqlite3",
      // 其他二进制
      "*.wasm",
      "*.pyc",
      "*.pyo",
      "*.class",
      "*.jar",
      "*.war",
      // 3D/游戏资源
      "*.fbx",
      "*.obj",
      "*.gltf",
      "*.glb",
      "*.blend",
      "*.unity",
      "*.unitypackage",
    ].join(",")
  )
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean) as string[],

  // 已压缩/二进制文件类型（Git 不进行 delta 压缩，节省 CPU）
  // 这些文件本身已经是压缩的（视频、图片、压缩包等），再压缩没有意义
  binaryFilePatterns: (
    process.env.BINARY_FILE_PATTERNS ||
    [
      // 图像
      "*.png",
      "*.jpg",
      "*.jpeg",
      "*.gif",
      "*.webp",
      "*.bmp",
      "*.ico",
      "*.heic",
      "*.heif",
      "*.avif",
      "*.tiff",
      "*.tif",
      // 视频
      "*.mp4",
      "*.mov",
      "*.m4v",
      "*.webm",
      "*.avi",
      "*.mkv",
      "*.wmv",
      "*.flv",
      "*.3gp",
      // 音频
      "*.mp3",
      "*.wav",
      "*.flac",
      "*.aac",
      "*.m4a",
      "*.ogg",
      "*.opus",
      "*.wma",
      // 压缩包
      "*.zip",
      "*.7z",
      "*.rar",
      "*.tar",
      "*.gz",
      "*.bz2",
      "*.xz",
      "*.zst",
      "*.lz4",
      "*.br",
      // 文档
      "*.pdf",
      "*.docx",
      "*.xlsx",
      "*.pptx",
      "*.epub",
      // 设计文件
      "*.psd",
      "*.ai",
      "*.sketch",
      "*.fig",
      // 可执行文件
      "*.exe",
      "*.dll",
      "*.so",
      "*.dylib",
      "*.bin",
      "*.apk",
      "*.dmg",
      "*.iso",
      "*.msi",
      // 其他二进制
      "*.wasm",
      "*.pyc",
      "*.class",
    ].join(",")
  )
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean) as string[],

  // 允许的文件类型 (留空表示允许所有)
  allowedFileTypes: [] as string[],

  // 文件访问白名单（路径前缀，基于 repo 根目录的相对路径；留空表示允许全部）
  // 例：['docs', 'images/public'] 只允许访问这些目录及其子路径
  allowedPathPrefixes: (process.env.ALLOWED_PATH_PREFIXES || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean) as string[],

  // 请求频率限制（内存限流；重启服务后计数重置）
  rateLimit: {
    enabled:
      (process.env.RATE_LIMIT_ENABLED || "true").toLowerCase() !== "false",
    windowMs: parseInt(process.env.RATE_LIMIT_WINDOW_MS || "60000"), // 1分钟
    max: parseInt(process.env.RATE_LIMIT_MAX || "120"), // 每窗口最大请求数/每 IP
  },

  // CORS设置
  cors: {
    origin: process.env.CORS_ORIGIN || "*",
    credentials: true,
    // 暴露 Content-Length 和 Content-Range 头，让前端能读取下载进度
    exposeHeaders: ["Content-Length", "Content-Range"],
  },

  // 是否启用日志
  enableLogging: process.env.NODE_ENV !== "production",

  // 用户系统（v1.1.0）
  auth: {
    enabled: (process.env.ENABLE_AUTH || "false").toLowerCase() === "true",
    secret: process.env.AUTH_SECRET || "",
    cookieName: process.env.AUTH_COOKIE_NAME || "vfiles_session",
    cookieSecure: (() => {
      const raw = (process.env.AUTH_COOKIE_SECURE || "").toLowerCase();
      if (raw === "true") return true;
      if (raw === "false") return false;
      return undefined;
    })(),
    tokenTtlSeconds: parseInt(
      process.env.AUTH_TOKEN_TTL_SECONDS || String(7 * 24 * 60 * 60),
    ),
    storagePath:
      process.env.AUTH_STORAGE_PATH ||
      path.resolve(process.cwd(), ".vfiles_auth", "users.json"),
    allowRegister:
      (process.env.AUTH_ALLOW_REGISTER || "true").toLowerCase() !== "false",
    // 登录额外限流：按 IP + username（与全局 /api 限流叠加）
    loginRateLimit: {
      enabled:
        (process.env.AUTH_LOGIN_RATE_LIMIT_ENABLED || "true").toLowerCase() !==
        "false",
      windowMs: parseInt(
        process.env.AUTH_LOGIN_RATE_LIMIT_WINDOW_MS || String(5 * 60 * 1000),
      ),
      max: parseInt(process.env.AUTH_LOGIN_RATE_LIMIT_MAX || "10"),
    },

    // 忘记密码 / 邮箱验证码登录
    passwordResetTokenTtlSeconds: parseInt(
      process.env.AUTH_PASSWORD_RESET_TOKEN_TTL_SECONDS || String(30 * 60),
    ),
    emailLoginCodeTtlSeconds: parseInt(
      process.env.AUTH_EMAIL_LOGIN_CODE_TTL_SECONDS || String(10 * 60),
    ),
  },

  // 邮件系统（v1.1.2）
  email: {
    enabled: (process.env.EMAIL_ENABLED || "false").toLowerCase() === "true",
    host: process.env.SMTP_HOST || "",
    port: parseInt(process.env.SMTP_PORT || "587"),
    secure: (process.env.SMTP_SECURE || "false").toLowerCase() === "true",
    user: process.env.SMTP_USER || "",
    pass: process.env.SMTP_PASS || "",
    from: process.env.SMTP_FROM || "",
    publicBaseUrl: process.env.PUBLIC_BASE_URL || "",
  },

  // 多用户隔离（v1.1.0）：每个用户使用独立仓库路径
  multiUser: {
    enabled:
      (process.env.MULTI_USER_ENABLED || "false").toLowerCase() === "true",
    baseDir:
      process.env.REPO_BASE_DIR || path.resolve(process.cwd(), ".vfiles_repos"),
  },

  // 临时分享链接（v1.2.0）
  share: {
    enabled: (process.env.SHARE_ENABLED || "true").toLowerCase() !== "false",
    // 默认有效期（秒），默认 7 天
    defaultTtlSeconds: parseInt(
      process.env.SHARE_DEFAULT_TTL_SECONDS || String(7 * 24 * 60 * 60),
    ),
    // 最大有效期（秒），默认 30 天
    maxTtlSeconds: parseInt(
      process.env.SHARE_MAX_TTL_SECONDS || String(30 * 24 * 60 * 60),
    ),
  },
};
