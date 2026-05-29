# 管理员初始化指南

本指南描述当前 Rust CLI 下管理员相关的最短初始化流程。

## 前置步骤

先初始化数据目录与数据库：

```bash
./target/release/vfiles init
```

## 创建第一个管理员

当系统里还没有任何管理员时，首个用户必须使用 `--role admin`：

```bash
./target/release/vfiles user create \
  --username admin \
  --email admin@example.com \
  --role admin \
  --password change-me
```

推荐优先使用 `--password`，由服务端按当前认证实现完成密码处理。

## 常用 user 子命令

```bash
# 列出用户
./target/release/vfiles user list

# 查询单个用户
./target/release/vfiles user get <user-id>

# 更新邮箱 / 角色 / 启用状态
./target/release/vfiles user update <user-id> --email new@example.com --role manager --enable

# 删除用户
./target/release/vfiles user delete <user-id>
```

## 完整初始化示例

```bash
# 1. 初始化数据目录
./target/release/vfiles init

# 2. 创建管理员
./target/release/vfiles user create \
  --username admin \
  --email admin@example.com \
  --role admin \
  --password your_secure_password

# 3. 启动服务
RUST_LOG=info ./target/release/vfiles serve
```

## 常见问题

### `Admin already exists`

说明系统中已经有管理员账号，不能再次用“首个管理员”流程初始化。

### `first user must be created with --role admin`

说明当前实例还没有 bootstrap，首个用户必须显式指定管理员角色。

### 数据库或权限错误

- 先确认已经执行 `init`
- 再确认部署目录对运行用户可写

## 相关文档

- [QUICK_START.md](QUICK_START.md)
- [DEPLOYMENT.md](DEPLOYMENT.md)
- [USER_GUIDE.md](USER_GUIDE.md)
