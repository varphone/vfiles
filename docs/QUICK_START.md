# VFiles 快速启动

本指南面向第一次在本机跑通 VFiles 的场景，基线为当前 Rust 后端。

## 前置要求

- Rust 工具链
- Git
- Bun：仅用于构建前端

## 最短启动路径

### 1. 准备配置

```bash
cp .env.example .env
```

至少把 `VFILES_AUTH_COOKIE_SECRET` 改成一段长度足够的随机字符串。

### 2. 构建前端

```bash
cd client && bun install && bun run build
cd ..
```

### 3. 构建服务端

```bash
cargo build -p vfiles-bin --release
```

### 4. 初始化数据目录

```bash
./target/release/vfiles init
```

### 5. 创建首个管理员

```bash
./target/release/vfiles user create \
  --username admin \
  --email admin@example.com \
  --role admin \
  --password change-me
```

### 6. 启动服务

```bash
RUST_LOG=info ./target/release/vfiles serve
```

访问 http://localhost:3000 ，使用刚创建的管理员账户登录。

## 开发模式

后端：

```bash
cargo run --package vfiles-bin --bin vfiles -- serve
```

前端：

```bash
cd client && bun run dev
```

前端开发服务器默认跑在 http://localhost:5173 。

## 单文件部署模式

如果希望把前端直接嵌进二进制，可在前端构建完成后启用 `embed`：

```bash
cargo build -p vfiles-bin --release --features embed
```

之后运行 `./target/release/vfiles serve` 时，不再依赖外部 `client/dist`。

## 启动后最小验收

```bash
curl http://localhost:3000/api/health
```

推荐再手动验证这几项：

- 能否登录
- 能否创建目录
- 能否上传一个小文件
- 能否在文件列表中看到新文件

## 后续阅读

- [DEPLOYMENT.md](DEPLOYMENT.md)
- [ADMIN_SETUP.md](ADMIN_SETUP.md)
- [USER_GUIDE.md](USER_GUIDE.md)
- [API.md](API.md)
