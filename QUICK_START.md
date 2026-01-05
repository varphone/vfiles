# VFiles 快速启动指南

## 开发模式

### 方式1: 使用两个终端（推荐）

**终端1 - 启动后端服务器:**
```powershell
cd d:\Workspace\Projects\vfiles
bun run dev
```

**终端2 - 启动前端开发服务器:**
```powershell
cd d:\Workspace\Projects\vfiles
bun run dev:client
```

然后访问: http://localhost:5173

### 方式2: 使用脚本同时启动

创建 `dev.ps1` 文件并运行（已创建在项目根目录）:
```powershell
.\dev.ps1
```

## 生产模式

### 构建和启动

```powershell
# 1. 构建前端
cd d:\Workspace\Projects\vfiles
bun run build

# 2. 启动服务器
bun run start
```

然后访问: http://localhost:3000

## 环境变量配置

可以创建 `.env` 文件来自定义配置:

```env
# 服务器端口
PORT=3000

# Git仓库路径
REPO_PATH=./data

# CORS配置
CORS_ORIGIN=*

# 环境
NODE_ENV=development
```

## 功能测试

### 1. 文件上传
- 点击"上传文件"按钮
- 选择或拖拽文件到上传区域
- 输入提交信息
- 点击上传

### 2. 文件浏览
- 点击文件夹进入
- 使用面包屑导航返回上级目录
- 点击刷新按钮重新加载

### 3. 版本历史
- 点击文件的"历史"按钮
- 查看所有提交记录
- 可以下载任意历史版本

### 4. 文件下载
- 点击文件的"下载"按钮
- 或在历史记录中下载特定版本

## 故障排除

### 端口被占用
如果3000或5173端口被占用，可以修改:
- 后端端口: 修改 `server/src/config.ts` 中的 `port`
- 前端端口: 修改 `client/vite.config.ts` 中的 `server.port`

### Git仓库初始化失败
确保 `data` 目录存在且有写权限:
```powershell
mkdir -p data
```

### 依赖安装问题
重新安装依赖:
```powershell
# 根目录
bun install

# 客户端
cd client
bun install
```

## 项目结构
```
vfiles/
├── server/          # 后端代码
│   └── src/
│       ├── routes/      # API路由
│       ├── services/    # 业务逻辑
│       ├── middleware/  # 中间件
│       └── index.ts     # 入口文件
├── client/          # 前端代码
│   └── src/
│       ├── components/  # Vue组件
│       ├── views/       # 页面
│       ├── stores/      # 状态管理
│       └── services/    # API服务
├── data/            # Git仓库数据
└── docs/            # 文档
```

## 更多信息

- 架构文档: [ARCHITECTURE.md](./ARCHITECTURE.md)
- 实施计划: [IMPLEMENTATION_PLAN.md](./IMPLEMENTATION_PLAN.md)
- README: [README.md](./README.md)
