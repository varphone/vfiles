# VFiles - 基于Web的Git文件管理系统

## 项目概述

VFiles 是一个基于Web的文件管理系统，使用Git作为底层存储和版本控制机制。系统提供直观的用户界面，支持文件浏览、版本历史查看和下载等功能，特别针对移动端用户体验进行优化。

## 技术栈

### 前端技术
- **Vue 3** - 渐进式JavaScript框架，使用Composition API
- **Bulma** - 现代化CSS框架，基于Flexbox，天然支持响应式设计
- **Tabler Icons** - 简洁美观的开源图标库
- **Vite** - 快速的构建工具

### 后端技术
- **Bun** - 高性能JavaScript运行时和包管理器
- **Hono** - 轻量级Web框架，性能优异
- **isomorphic-git** - 纯JavaScript实现的Git客户端

### 版本控制
- **Git** - 文件版本管理核心

## 系统架构

### 整体架构图

```
┌─────────────────────────────────────────────────────┐
│                   客户端层 (Browser)                  │
│  ┌──────────────────────────────────────────────┐   │
│  │         Vue 3 SPA Application                │   │
│  │  ┌────────────┐  ┌─────────────────────┐    │   │
│  │  │  文件浏览器  │  │   版本历史查看器     │    │   │
│  │  └────────────┘  └─────────────────────┘    │   │
│  │  ┌────────────┐  ┌─────────────────────┐    │   │
│  │  │  文件上传   │  │   文件下载管理      │    │   │
│  │  └────────────┘  └─────────────────────┘    │   │
│  │         Bulma CSS + Tabler Icons           │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
                          ▲
                          │ HTTP/REST API
                          ▼
┌─────────────────────────────────────────────────────┐
│                   服务端层 (Bun)                      │
│  ┌──────────────────────────────────────────────┐   │
│  │            Hono Web Server                   │   │
│  │  ┌────────────┐  ┌─────────────────────┐    │   │
│  │  │  文件API    │  │    版本API          │    │   │
│  │  └────────────┘  └─────────────────────┘    │   │
│  │  ┌────────────┐  ┌─────────────────────┐    │   │
│  │  │  上传API    │  │    下载API          │    │   │
│  │  └────────────┘  └─────────────────────┘    │   │
│  └──────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────┐   │
│  │         Git Service Layer                    │   │
│  │      (isomorphic-git wrapper)                │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
                          ▲
                          │ File System Operations
                          ▼
┌─────────────────────────────────────────────────────┐
│                   存储层                              │
│  ┌──────────────────────────────────────────────┐   │
│  │          Git Repository (.git/)              │   │
│  │  - Objects (文件内容)                         │   │
│  │  - Refs (分支和标签)                          │   │
│  │  - Commits (提交历史)                         │   │
│  └──────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────┐   │
│  │          Working Directory                   │   │
│  │          (当前文件系统)                        │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

## 核心功能模块

### 1. 文件浏览模块
**功能描述：**
- 树形结构展示文件和文件夹
- 支持面包屑导航
- 显示文件元信息（大小、修改时间、最后提交信息）
- 文件夹折叠/展开
- 文件预览（文本、图片、Markdown等）

**技术实现：**
- Vue 3 Composition API
- 递归组件实现树形结构
- Bulma的卡片和列表组件
- Tabler Icons图标

### 2. 版本历史模块
**功能描述：**
- 显示文件的完整提交历史
- 查看每个版本的提交信息、作者、时间
- 版本对比功能
- 时间线视图

**技术实现：**
- Git log命令封装
- 分页加载历史记录
- 时间轴组件（Bulma timeline）

### 3. 文件上传模块
**功能描述：**
- 支持单文件和多文件上传
- 拖拽上传
- 上传进度显示
- 自动提交到Git

**技术实现：**
- File API
- FormData上传
- Git add + commit操作
- 进度条组件

### 4. 文件下载模块
**功能描述：**
- 下载当前版本文件
- 下载历史版本文件
- 批量下载（ZIP打包）
- 文件夹下载

**技术实现：**
- Blob API
- Git show命令获取历史版本
- JSZip打包（可选）

### 5. 搜索模块
**功能描述：**
- 文件名搜索
- 文件内容搜索
- 提交信息搜索

**技术实现：**
- Git grep命令
- 前端过滤
- 搜索结果高亮

## 数据模型

### File对象
```typescript
interface FileInfo {
  name: string;
  path: string;
  type: 'file' | 'directory';
  size: number;
  mtime: Date;
  lastCommit?: {
    hash: string;
    message: string;
    author: string;
    date: Date;
  };
}
```

### Commit对象
```typescript
interface CommitInfo {
  hash: string;
  message: string;
  author: {
    name: string;
    email: string;
  };
  date: Date;
  parent: string[];
}
```

### FileHistory对象
```typescript
interface FileHistory {
  commits: CommitInfo[];
  currentVersion: string;
  totalCommits: number;
}
```

## API设计

### RESTful API端点

#### 文件操作
- `GET /api/files` - 获取文件列表
  - Query: `path` (路径), `recursive` (是否递归)
- `GET /api/files/:path` - 获取文件详情
- `GET /api/files/:path/content` - 获取文件内容
- `POST /api/files` - 上传文件
- `DELETE /api/files/:path` - 删除文件
- `PUT /api/files/:path` - 更新文件

#### 版本操作
- `GET /api/history/:path` - 获取文件历史
  - Query: `page`, `limit`
- `GET /api/commit/:hash` - 获取提交详情
- `GET /api/commit/:hash/files/:path` - 获取指定提交的文件内容

#### 下载操作
- `GET /api/download/:path` - 下载当前版本文件
- `GET /api/download/:path?commit=:hash` - 下载指定版本文件
- `POST /api/download/batch` - 批量下载

#### 搜索操作
- `GET /api/search?q=:query` - 搜索文件和内容

## 移动端适配策略

### 响应式设计
- 使用Bulma的列系统和响应式修饰符
- 移动端优先设计（Mobile First）
- 触摸友好的交互元素（大按钮、充足间距）

### 性能优化
- 虚拟滚动（长列表）
- 图片懒加载
- 分页加载历史记录
- API响应缓存

### 移动端特性
- 底部导航栏（更易触达）
- 汉堡菜单
- 下拉刷新
- 手势操作（滑动返回等）

## 安全考虑

### 文件访问控制
- 配置允许访问的目录白名单
- 路径遍历攻击防护
- 文件类型限制

### 上传安全
- 文件大小限制
- 文件类型验证
- 病毒扫描（可选）

### Git操作安全
- 操作权限验证
- 敏感文件保护（.git目录不可直接访问）

## 部署架构

### 开发环境
```bash
bun install
bun run dev  # 启动开发服务器
```

### 生产环境
```bash
bun run build  # 构建前端
bun run start  # 启动生产服务器
```

### Docker部署（可选）
```dockerfile
FROM oven/bun:latest
WORKDIR /app
COPY . .
RUN bun install --production
RUN bun run build
CMD ["bun", "run", "start"]
```

## 扩展性设计

### 插件系统
- 自定义文件预览器
- 第三方存储后端（S3、OSS等）
- 自定义认证方式

### 多仓库支持
- 配置多个Git仓库
- 仓库切换

### 协作功能（未来）
- 多用户系统
- 权限管理
- 冲突解决

## 性能指标

### 目标性能
- 首屏加载时间：< 2s
- API响应时间：< 200ms
- 文件列表渲染：< 100ms
- 支持并发用户：> 100

### 优化策略
- HTTP/2 服务器推送
- 资源压缩（Gzip/Brotli）
- CDN加速（静态资源）
- 数据库缓存（Redis可选）

## 技术选型理由

### 为什么选择Bun？
- 极快的启动速度和运行性能
- 内置TypeScript支持
- 兼容Node.js生态
- 优秀的包管理器

### 为什么选择Vue 3？
- 渐进式框架，易于学习
- Composition API提供更好的代码组织
- 优秀的响应式系统
- 丰富的生态系统

### 为什么选择Bulma？
- 纯CSS框架，无JavaScript依赖
- 基于Flexbox，响应式设计优秀
- 文档完善，易于定制
- 移动端友好

### 为什么选择isomorphic-git？
- 纯JavaScript实现，与Bun完美兼容
- 无需系统Git依赖
- API友好，易于集成
- 支持浏览器和Node.js环境

## 后续优化方向

1. **性能优化**
   - 实现增量加载
   - WebWorker处理大文件
   - IndexedDB本地缓存

2. **功能增强**
   - 实时协作编辑
   - 文件版本对比可视化
   - 分支管理

3. **用户体验**
   - PWA支持（离线访问）
   - 暗色主题
   - 国际化支持

4. **集成能力**
   - WebDAV支持
   - Git远程仓库同步
   - CI/CD集成
