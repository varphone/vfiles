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

# 改名（修复历史数据里未通过校验的用户名，见「忘记管理员账号 / 密码」一节）
./target/release/vfiles user update <user-id> --username newname

# 删除用户
./target/release/vfiles user delete <user-id>
```

> 所有 `user` 子命令都读写**配置里的同一个数据库**：默认是 `$VFILES_STORAGE_ROOT/vfiles.db`
> （未设置时即运行目录下的 `data/vfiles.db`）。如果服务用了自定义路径，请带上
> `VFILES_DATABASE_PATH=/path/to/vfiles.db` 再执行，否则会操作到另一个库。

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

## 忘记管理员账号 / 密码

密码只以 Argon2 哈希存储，**无法反推**，只能用下面的方式重置（不需要邮件 / SMTP，离线即可）：

```bash
# 1) 找到管理员账号（忘记用户名时）
vfiles user list                       # 列出 id / username / email / role / disabled
# 也可以直接查库（只读）：
sqlite3 data/vfiles.db "SELECT username,email,role,disabled FROM users WHERE role='admin'"

# 2) 重置密码（默认会让该用户已登录的会话全部失效）
vfiles user reset-password --username admin --generate     # 生成随机强密码并打印 new_password=...
# 或指定密码（至少 8 位）：
vfiles user reset-password --username admin --password 'new-password-123'

# 3) 用新密码登录
curl -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username_or_email":"admin","password":"new-password-123"}'
```

其他说明：

- `--user-id <uuid>` 可替代 `--username`；`--keep-sessions` 保留已登录会话（默认全部下线）；
- `--password-hash <hash>` 用于自动化脚本，直接写入已哈希的密码；
- 若管理员数量为 0（例如误删），用 `vfiles user create --username admin --email … \
  --password … --role admin` 会走「首个管理员」引导流程重新建立；
- **用户名含 `-` 等非法字符的历史账号**：旧版本 bootstrap 未校验用户名，这类账号在读取时
  会报 `Internal error: Invalid username`。新版已修复（读取兼容、创建校验），并支持修复：
  `vfiles user update <user-id> --username admin2` 改成合法用户名后再 `reset-password` 即可登录。

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
