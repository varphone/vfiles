import path from 'node:path';

// 服务器配置
export const config = {
  // 服务器端口
  port: parseInt(process.env.PORT || '3000'),
  
  // Git仓库路径
  repoPath: process.env.REPO_PATH || path.resolve(process.cwd(), 'data'),
  
  // 上传限制
  maxFileSize: 100 * 1024 * 1024, // 100MB
  
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
