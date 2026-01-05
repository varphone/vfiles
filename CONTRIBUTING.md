# 贡献指南（CONTRIBUTING）

感谢你对 VFiles 的贡献。

## 开发环境

- Bun
- Git

安装依赖：

```bash
bun install
cd client && bun install
```

## 常用命令

- 开发后端：`bun run dev`
- 开发前端：`bun run dev:client`
- 构建：`bun run build`
- Lint：`bun run lint`
- 格式化：`bun run fmt`

## 分支与提交

- 建议从 `master` 切分支：`feature/*`、`fix/*`、`chore/*` 等
- 提交信息建议使用 Conventional Commits：
  - `feat:` 新功能
  - `fix:` 修复
  - `docs:` 文档
  - `refactor:` 重构
  - `chore:` 工具/构建

## 代码风格

- TypeScript 为主
- 通过 `bun run lint` 保障基本质量
- 提交前建议执行 `bun run fmt`

## 提交流程

1. 新建分支并开发
2. 本地验证：`bun run lint`（可选再跑 `bun run build`）
3. 提交并推送
4. 提交 Pull Request，描述：变更内容/风险点/如何验证
