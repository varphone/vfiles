# VFiles - 基于Web的Git文件管理系统

一个现代化的文件管理系统，使用Git作为版本控制，提供直观的Web界面用于浏览、管理和下载文件及其历史版本。

## ✨ 特性

- 📁 **文件浏览** - 直观的文件和文件夹浏览界面
- 📜 **版本历史** - 查看完整的文件修改历史
- ⬇️ **历史下载** - 下载任意历史版本的文件
- 📱 **移动优化** - 响应式设计，完美支持移动设备
- 🚀 **高性能** - 基于Bun和Vue 3构建，快速响应
- 🎨 **现代UI** - 使用Bulma CSS框架和Tabler图标

## 🛠️ 技术栈

- **前端**: Vue 3, Bulma, Tabler Icons, Vite
- **后端**: Bun, Hono, isomorphic-git
- **版本控制**: Git

## 📦 安装

### 前置要求

- [Bun](https://bun.sh/) >= 1.0.0

### 安装步骤

1. 克隆项目

```bash
git clone <repository-url>
cd vfiles
```

2. 安装依赖

```bash
# 安装根目录依赖
bun install

# 安装客户端依赖
cd client
bun install
cd ..
```

3. 创建数据目录

```bash
mkdir -p data
```

## 🚀 使用

### 开发模式

终端1 - 启动后端服务器:

```bash
bun run dev
```

终端2 - 启动前端开发服务器:

```bash
bun run dev:client
```

访问 http://localhost:5173

### 生产模式

1. 构建前端

```bash
bun run build
```

2. 启动服务器

```bash
bun run start
```

访问 http://localhost:3000

## 📖 文档

- [架构设计](./ARCHITECTURE.md) - 系统架构和设计文档
- [实施计划](./IMPLEMENTATION_PLAN.md) - 详细的开发计划
- [API 文档](./API.md) - 后端 API 说明
- [部署指南](./DEPLOYMENT.md) - 本地/服务器部署与环境变量
- [贡献指南](./CONTRIBUTING.md) - 开发与提交规范
- [用户手册](./USER_GUIDE.md) - 常用功能使用说明

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

## 📄 许可

MIT License
