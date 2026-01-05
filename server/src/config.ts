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
  
  // CORS设置
  cors: {
    origin: process.env.CORS_ORIGIN || '*',
    credentials: true,
  },
  
  // 是否启用日志
  enableLogging: process.env.NODE_ENV !== 'production',
};
