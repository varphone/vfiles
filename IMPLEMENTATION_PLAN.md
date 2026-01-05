# VFiles 实施计划

## 项目时间线

本项目预计分为6个主要阶段，按照依赖关系和优先级逐步实施。

## 阶段 1: 项目初始化和基础设置 (预计时间: 1-2小时)

### 1.1 创建项目结构
- [x] 初始化Bun项目
- [x] 配置TypeScript
- [x] 创建前后端目录结构
- [x] 配置Vite构建工具

### 1.2 安装依赖包
**前端依赖:**
```json
{
  "vue": "^3.5.26",
  "vue-router": "^4.6.4",
  "pinia": "^3.0.4",
  "bulma": "^1.0.4",
  "@tabler/icons-vue": "^3.36.1",
  "axios": "^1.13.2"
}
```

**后端依赖:**
```json
{
  "hono": "^4.11.3"
}
```

**开发依赖:**
```json
{
  "typescript": "^5.9.3",
  "vite": "^7.3.0",
  "@vitejs/plugin-vue": "^6.0.3",
  "@types/node": "^25.0.3",
  "bun-types": "^1.3.5"
}
```

### 1.3 配置文件
- [x] `tsconfig.json` - TypeScript配置
- [x] `vite.config.ts` - Vite配置
- [ ] `bun.config.ts` - Bun配置（当前未使用）
- [x] `.gitignore` - Git忽略文件

### 交付物
- ✅ 可运行的基础项目框架
- ✅ 所有依赖包已安装
- ✅ 开发服务器可正常启动

---

## 阶段 2: 后端API服务实现 (预计时间: 4-6小时)

### 2.1 服务器基础设置
- [x] 创建Hono应用实例
- [x] 配置CORS中间件
- [x] 配置静态文件服务（生产环境）
- [x] 错误处理中间件
- [x] 日志中间件

**文件:**
- `server/src/index.ts` - 服务器入口
- `server/middleware/` - 中间件目录
- `server/config.ts` - 服务器配置

### 2.2 Git服务层实现
- [x] Git仓库初始化/检查
- [x] 文件读取服务
- [x] 文件写入服务
- [x] 提交历史查询
- [x] 特定版本文件获取

**文件:**
- `server/services/git.service.ts` - Git核心服务
- `server/utils/git-helpers.ts` - Git辅助函数

**核心函数:**
```typescript
class GitService {
  async listFiles(path: string): Promise<FileInfo[]>
  async getFileContent(path: string, commit?: string): Promise<string>
  async saveFile(path: string, content: Buffer, message: string): Promise<string>
  async getFileHistory(path: string, limit?: number): Promise<CommitInfo[]>
  async getCommitDetails(hash: string): Promise<CommitInfo>
  async deleteFile(path: string, message: string): Promise<void>
}
```

### 2.3 API路由实现
- [x] 文件操作路由 (`/api/files/*`)
- [x] 版本历史路由 (`/api/history/*`)
- [x] 下载路由 (`/api/download/*`)
- [x] 搜索路由 (`/api/search`)

**文件:**
- `server/routes/files.routes.ts`
- `server/routes/history.routes.ts`
- `server/routes/download.routes.ts`
- `server/routes/search.routes.ts`

### 2.4 文件上传功能
- [x] 实现文件上传处理
- [x] 多文件上传支持（前端多文件；后端一次上传一个文件）
- [x] 文件大小限制（已在后端上传路由按 config.maxFileSize 强制）
- [x] 文件类型验证（已在后端上传路由按 config.allowedFileTypes 白名单校验）
- [x] 自动Git提交

### 2.5 安全性实现
- [x] 路径遍历防护
- [x] 文件访问白名单（已实现：按 config.allowedPathPrefixes 路径前缀 allowlist，可通过环境变量 ALLOWED_PATH_PREFIXES 配置）
- [x] 请求频率限制（已实现：/api 全局内存限流，可通过 RATE_LIMIT_* 环境变量配置）
- [x] 输入验证（已增强：统一校验 path/commit/hash/limit/message/q 等参数）

**文件:**
- `server/middleware/security.ts`
- `server/utils/path-validator.ts`

### 交付物
- ✅ 完整的REST API服务
- ✅ Git操作核心功能
- ⛔ API文档（OpenAPI格式，尚未生成）
- ⛔ 基本的单元测试（尚未添加）

---

## 阶段 3: 前端核心组件开发 (预计时间: 6-8小时)

### 3.1 项目结构设置
```
client/
├── src/
│   ├── components/
│   │   ├── common/          # 通用组件
│   │   ├── file-browser/    # 文件浏览器
│   │   ├── version-history/ # 版本历史
│   │   └── file-uploader/   # 文件上传
│   ├── views/               # 页面视图
│   ├── stores/              # Pinia状态管理
│   ├── services/            # API服务
│   ├── composables/         # 组合式函数
│   ├── router/              # 路由配置
│   └── assets/              # 静态资源
```

### 3.2 状态管理设置
- [x] 创建Pinia store
- [x] 文件列表状态
- [x] 当前路径状态
- [ ] 历史记录缓存（未做缓存，仅按需加载）
- [ ] 用户设置状态（未实现）

**文件:**
- `client/src/stores/files.store.ts`
- `client/src/stores/app.store.ts`

### 3.3 API服务层
- [x] Axios实例配置
- [x] API封装
- [x] 错误处理
- [x] 请求拦截器（基础）
- [x] 响应拦截器（基础）

**文件:**
- `client/src/services/api.service.ts`
- `client/src/services/files.service.ts`

### 3.4 文件浏览器组件
**组件列表:**
- [x] `FileBrowser.vue` - 主容器
- [x] `FileList.vue` - 文件列表（已实现：从 FileBrowser 抽取列表渲染与事件分发）
- [x] `FileItem.vue` - 单个文件项
- [x] `Breadcrumb.vue` - 面包屑导航
- [x] 文件预览（已实现：点击文件弹窗预览当前版本）
- [ ] `ContextMenu.vue` - 右键菜单（未实现）

**功能:**
- 文件/文件夹展示
- 排序（名称、大小、时间）
- 图标展示（根据文件类型）
- 点击导航
- 文件操作（下载、删除、查看历史）

### 3.5 版本历史组件
**组件列表:**
- [x] `VersionHistory.vue` - 主容器
- [x] `CommitList.vue` - 提交列表
- [x] `CommitItem.vue` - 单个提交
- [x] `CommitDetails.vue` - 提交详情
- [x] `Timeline.vue` - 时间轴视图

**功能:**
- [x] 显示提交历史
- [x] 分页加载（加载更多）
- [x] 版本对比（已实现：文本文件 unified diff 视图）
- [x] 恢复到历史版本（已实现：从指定 commit 读取内容并写回生成新提交）
- [x] 版本内容预览（已实现：文本 + 图片在线预览，其它类型提示下载）

### 3.6 文件上传组件
**组件列表:**
- [x] `FileUploader.vue` - 主容器
- [x] `DropZone.vue` - 拖拽区域
- [x] `UploadProgress.vue` - 上传进度
- [x] `UploadQueue.vue` - 上传队列

**功能:**
- [x] 点击选择文件
- [x] 拖拽上传
- [x] 多文件上传（队列顺序执行）
- [x] 进度显示（每个文件；无 total 时显示不确定进度）
- [x] 上传取消（取消当前/取消排队）

### 3.7 通用组件
- [x] `Loading.vue` - 加载指示器
- [x] `ErrorMessage.vue` - 错误提示
- [x] `Modal.vue` - 模态框
- [x] `Notification.vue` - 通知组件
- [x] `SearchBar.vue` - 搜索栏
- [x] `IconButton.vue` - 图标按钮

### 交付物
- ✅ 完整的文件浏览界面
- ✅ 版本历史查看功能
- ✅ 文件上传功能
- ✅ 响应式设计基础

---

## 阶段 4: 移动端优化和响应式设计 (预计时间: 3-4小时)

### 4.1 Bulma响应式布局
- [x] 移动端布局调整（基础）
- [x] 触摸友好的交互元素（基础）
- [ ] 移动端导航菜单（未实现）
- [ ] 底部操作栏（未实现）

### 4.2 移动端特定功能
- [ ] 下拉刷新（未实现）
- [ ] 无限滚动（未实现）
- [ ] 手势操作（未实现）
- [ ] 移动端文件预览优化（未实现）

### 4.3 性能优化
- [ ] 虚拟滚动（长列表）
- [ ] 图片懒加载
- [ ] 路由懒加载
- [ ] 组件按需加载

### 4.4 CSS优化
- [ ] 自定义Bulma变量
- [ ] 移动端样式覆盖
- [ ] 深色模式支持（可选）
- [ ] 动画和过渡效果

**文件:**
- `client/src/assets/styles/main.scss`
- `client/src/assets/styles/mobile.scss`
- `client/src/assets/styles/variables.scss`

### 交付物
- ✅ 完全响应式的界面（基础）
- ⛔ 优秀的移动端体验（仍有提升空间）
- ⛔ 性能优化完成（未做专项优化）

---

## 阶段 5: 高级功能实现 (预计时间: 4-5小时)

### 5.1 搜索功能
- [x] 文件名搜索（后端已实现 /api/search；前端 UI 未实现）
- [x] 文件名搜索 UI（已实现：文件浏览页搜索栏 + 结果列表）
- [x] 文件内容搜索（全文，已实现：后端 git grep + 前端“全文”开关）
- [x] 全文命中片段展示（已实现：返回行号+内容片段，并在结果列表中显示）
- [x] 搜索结果高亮（已实现：高亮文件名命中片段）
- [x] 搜索历史保存（已实现：localStorage 持久化 + 输入框 datalist 自动补全）
- [x] 高级筛选（已实现：类型过滤 + 仅当前目录范围）

**组件:**
- `SearchPanel.vue`
- `SearchResults.vue`
- `SearchFilters.vue`

### 5.2 批量操作
- [x] 多选文件（已实现：批量模式 + 复选框）
- [x] 批量下载（已实现：仅文件，多文件触发浏览器下载）
- [x] 批量删除（已实现：前端循环调用删除接口）
- [x] 批量移动/重命名（已实现：批量移动；重命名为单选操作，后端统一走 move API）

### 5.3 文件预览增强
- [x] Markdown渲染（已实现：历史版本预览支持 Markdown 渲染，禁用原始 HTML 注入）
- [x] 代码高亮（已实现：历史版本预览支持代码高亮；使用 highlight.js 的无主题灰度样式）
- [x] PDF预览（已实现：iframe 内嵌预览）
- [x] 视频播放（已实现：video controls 播放）
- [x] 音频播放（已实现：audio controls 播放）

### 5.4 下载功能增强
- [x] 文件夹打包下载（ZIP）（已实现：后端 /api/download/folder 输出 zip；前端支持文件夹下载与批量下载文件夹）
- [x] 断点续传（已实现：下载接口支持 HTTP Range，当前版本文件可续传；历史版本下载仍为整文件）
- [x] 下载队列管理（已实现：前端下载入队、顺序执行、进度展示、单项取消/全部取消、清空已完成/移除）

### 5.5 版本对比
- [ ] Diff视图
- [ ] 并排对比
- [ ] 语法高亮对比

**依赖:**
- `diff2html` - Diff视图
- `highlight.js` - 代码高亮
- `marked` - Markdown解析

### 交付物
- ✅ 基础搜索系统（文件名 + 全文 + 命中片段 + 高级筛选）
- ✅ 批量操作功能（多选、批量下载/删除/移动、单选重命名）
- ✅ 增强的预览能力（Markdown/代码高亮/PDF/音视频；历史与当前版本预览）
- ⛔ 版本对比功能

---

## 阶段 6: 测试、优化和部署 (预计时间: 3-4小时)

### 6.1 测试
- [ ] 单元测试（Vitest）
  - API服务测试
  - 组件测试
  - 工具函数测试
- [ ] 端到端测试（Playwright）
  - 文件上传流程
  - 文件浏览流程
  - 版本历史流程
- [ ] 移动端测试
  - 多种设备测试
  - 触摸交互测试

### 6.2 性能优化
- [ ] Lighthouse性能评分
- [ ] 打包体积优化
- [ ] 代码分割
- [ ] 资源压缩
- [ ] 缓存策略

### 6.3 文档编写
- [x] README.md - 项目说明（已有基础）
- [ ] API.md - API文档（未编写）
- [ ] DEPLOYMENT.md - 部署指南（未编写）
- [ ] CONTRIBUTING.md - 贡献指南
- [ ] 用户手册

### 6.4 生产构建
- [ ] 环境变量配置
- [ ] 生产构建配置
- [ ] Docker配置（可选）
- [ ] CI/CD配置

### 6.5 部署
- [ ] 本地部署测试
- [ ] 服务器部署
- [ ] 域名配置
- [ ] SSL证书配置
- [ ] 监控和日志

### 交付物
- ⛔ 完整的测试覆盖（未实现）
- ✅ 优化的生产构建（已可构建并由服务端在 production 托管 dist）
- ⛔ 完善的文档（部分完成）
- ✅ 可部署的应用（基础可部署）

---

## 项目文件结构

```
vfiles/
├── client/                          # 前端代码
│   ├── src/
│   │   ├── components/
│   │   │   ├── common/              # Loading/Modal/Notification
│   │   │   ├── file-browser/        # FileBrowser/FileItem/Breadcrumb
│   │   │   ├── file-uploader/       # FileUploader
│   │   │   └── version-history/     # VersionHistory（含 TODO：版本预览）
│   │   ├── router/
│   │   ├── services/
│   │   ├── stores/
│   │   ├── types/
│   │   ├── views/
│   │   ├── App.vue
│   │   └── main.ts
│   ├── index.html
│   ├── package.json
│   └── vite.config.ts
├── server/                          # 后端代码
│   └── src/
│       ├── middleware/
│       ├── routes/
│       ├── services/
│       ├── types/
│       ├── utils/
│       ├── config.ts
│       └── index.ts
├── data/                            # 运行时数据目录（已在主仓库中忽略）
├── .gitattributes
├── .gitignore
├── ARCHITECTURE.md
├── IMPLEMENTATION_PLAN.md
├── QUICK_START.md
├── README.md
└── package.json
```

---

## 开发规范

### 代码规范
- 使用TypeScript严格模式
- 遵循ESLint规则
- 使用Prettier格式化
- Git提交信息遵循Conventional Commits

### 命名规范
- 组件：PascalCase (FileBrowser.vue)
- 文件：kebab-case (file-browser.ts)
- 变量/函数：camelCase (getUserInfo)
- 常量：UPPER_SNAKE_CASE (API_BASE_URL)
- 类型/接口：PascalCase (FileInfo)

### Git工作流
- main分支：生产环境
- develop分支：开发环境
- feature/*：功能分支
- bugfix/*：修复分支

### 提交规范
```
feat: 新功能
fix: 修复bug
docs: 文档更新
style: 代码格式
refactor: 重构
test: 测试
chore: 构建工具或辅助工具变动
```

---

## 关键技术决策

### 1. 为什么调用系统 git 命令而不是 isomorphic-git？
- ✅ 直接复用系统 git，行为与开发者习惯一致
- ✅ 对 Git 历史/下载等功能实现更直观（`git show`/`git log`）
- ✅ 在 Bun 环境下兼容性更稳定
- ❌ 依赖机器已安装 git（Windows 需安装 Git for Windows）
- **决策：** 当前实现使用系统 `git`；未来如需“零依赖安装”可再评估引入 isomorphic-git 作为可选后端

### 2. 单体应用还是前后端分离？
- **决策：** 前后端分离但部署在一起
- 原因：开发独立性、可维护性、但部署简单

### 3. 状态管理使用Pinia还是Vuex？
- **决策：** Pinia
- 原因：更好的TypeScript支持、更简洁的API、Vue 3推荐

### 4. CSS框架选择
- **决策：** Bulma
- 原因：纯CSS、无JS依赖、响应式优秀、易于定制

---

## 风险和挑战

### 技术风险
1. **isomorphic-git性能问题**
   - 风险：大仓库性能不佳
   - 缓解：实现缓存、分页、考虑混合方案

2. **大文件处理**
   - 风险：上传/下载大文件可能超时
   - 缓解：分片上传、流式下载、文件大小限制

3. **并发控制**
   - 风险：多用户同时操作可能冲突
   - 缓解：文件锁、冲突检测、事务处理

### 业务风险
1. **存储空间**
   - 风险：Git历史累积占用大量空间
   - 缓解：定期清理、浅克隆、LFS支持

2. **权限管理**
   - 风险：初版无权限系统
   - 缓解：后续版本添加、暂时使用网络隔离

---

## 里程碑

- **M1 (Day 1-2):** 基础框架搭建完成，开发环境可运行
- **M2 (Day 3-5):** 后端API完成，核心Git功能实现
- **M3 (Day 6-9):** 前端主要功能完成，基本可用
- **M4 (Day 10-12):** 移动端优化和高级功能完成
- **M5 (Day 13-15):** 测试、优化、文档完成，可部署

---

## 后续迭代计划

### v1.1 - 用户系统
- 用户注册/登录
- 权限管理
- 多用户隔离

### v1.2 - 协作功能
- 实时协作编辑
- 评论系统
- 通知系统

### v1.3 - 集成功能
- WebDAV支持
- S3存储后端
- 远程Git仓库同步

### v2.0 - 企业版
- SSO集成
- 审计日志
- 高可用部署
- 性能监控

---

## 总结

本实施计划提供了一个清晰的路线图，按照6个阶段逐步构建VFiles系统。每个阶段都有明确的目标和交付物，确保项目能够有序推进。

重点关注：
1. ✅ 移动端优先设计
2. ✅ Git原生集成
3. ✅ 性能和用户体验
4. ✅ 可扩展架构

按照此计划，预计可在2-3周内完成MVP版本。
