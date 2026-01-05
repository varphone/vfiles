import path from 'node:path';

// 服务器配置
export const config = {
  // 服务器端口
  port: parseInt(process.env.PORT || '3000'),
  
  // Git仓库路径
  repoPath: process.env.REPO_PATH || path.resolve(process.cwd(), 'data'),
  
  // 上传限制
  maxFileSize: 1024 * 1024 * 1024, // 1GB

  // 分块上传（用于绕过单次请求体大小限制）
  // 单块大小（默认 5MB，可通过 CDN 限制调整）
  uploadChunkSize: parseInt(process.env.UPLOAD_CHUNK_SIZE || String(5 * 1024 * 1024)),
  // 单块最大允许大小（防止客户端随意放大 chunk）
  uploadMaxChunkSize: parseInt(process.env.UPLOAD_MAX_CHUNK_SIZE || String(20 * 1024 * 1024)),
  // 分块上传临时目录（必须在 repoPath 之外，避免被当作文件列表展示/提交）
  uploadTempDir: process.env.UPLOAD_TEMP_DIR || path.resolve(process.cwd(), '.vfiles_uploads'),
  // 上传会话过期时间（默认 24h）
  uploadSessionTtlMs: parseInt(process.env.UPLOAD_SESSION_TTL_MS || String(24 * 60 * 60 * 1000)),

  // 下载缓存（用于历史版本 + Git LFS smudge 后的临时落地，从而支持 Range/断点续传）
  downloadCacheDir:
    process.env.DOWNLOAD_CACHE_DIR || path.resolve(process.cwd(), '.vfiles_download_cache'),
  // 缓存过期时间（默认 6h）
  downloadCacheTtlMs: parseInt(process.env.DOWNLOAD_CACHE_TTL_MS || String(6 * 60 * 60 * 1000)),

  // Git LFS
  enableGitLfs: (process.env.ENABLE_GIT_LFS || 'true').toLowerCase() !== 'false',
  // 以逗号分隔的 glob 列表（默认覆盖常见二进制/大文件类型）
  gitLfsTrackPatterns: (process.env.GIT_LFS_TRACK_PATTERNS ||
    [
      '*.png',
      '*.jpg',
      '*.jpeg',
      '*.gif',
      '*.webp',
      '*.bmp',
      '*.ico',
      '*.pdf',
      '*.zip',
      '*.7z',
      '*.rar',
      '*.tar',
      '*.gz',
      '*.bz2',
      '*.xz',
      '*.mp4',
      '*.mov',
      '*.m4v',
      '*.webm',
      '*.mp3',
      '*.wav',
      '*.flac',
      '*.aac',
      '*.m4a',
      '*.psd',
      '*.dmg',
      '*.apk',
      '*.exe',
      '*.dll',
      '*.bin',
    ].join(',')
  )
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean) as string[],
  
  // 允许的文件类型 (留空表示允许所有)
  allowedFileTypes: [] as string[],

  // 文件访问白名单（路径前缀，基于 repo 根目录的相对路径；留空表示允许全部）
  // 例：['docs', 'images/public'] 只允许访问这些目录及其子路径
  allowedPathPrefixes: (process.env.ALLOWED_PATH_PREFIXES || '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean) as string[],

  // 请求频率限制（内存限流；重启服务后计数重置）
  rateLimit: {
    enabled: (process.env.RATE_LIMIT_ENABLED || 'true').toLowerCase() !== 'false',
    windowMs: parseInt(process.env.RATE_LIMIT_WINDOW_MS || '60000'), // 1分钟
    max: parseInt(process.env.RATE_LIMIT_MAX || '120'), // 每窗口最大请求数/每 IP
  },
  
  // CORS设置
  cors: {
    origin: process.env.CORS_ORIGIN || '*',
    credentials: true,
  },
  
  // 是否启用日志
  enableLogging: process.env.NODE_ENV !== 'production',
};
