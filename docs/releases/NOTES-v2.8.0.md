## VFiles 2.8.0

Web 客户端界面重构与发布前整改版本。

### 亮点

- Nextcloud 风格的文件工作区：应用顶栏、视图侧栏、文件列表、详情侧栏与统一搜索。
- 新增最近、收藏、活动视图；管理台与设置中心统一布局。
- 同账号多密码（目录范围权限）与访问密码管理。
- 历史/快照浏览、实时列表与概览的索引优化。

### 发布前修复

- 修复 WebDAV `PROPFIND Depth: 1` 返回 500，cadaver/curl 可正常列目录。
- 升级 `zip`、`libsqlite3-sys`、`rustls`、`tokio`、`zstd` 等依赖，清除已知漏洞。
- 开放注册默认关闭；rsync 默认关闭；systemd unit 增加基础沙箱并支持非 root 用户。
- 登录页页脚与次要链接整理；新增像素级视觉回归基线。

### 升级说明

- 需要 SQLite 迁移 `0029`–`0034`；大库首次启动会回填索引与派生列，请预留时间并先备份。
- 目录历史的 `totalCommits` 默认按需返回，需要时传 `with_total=true`。
- 访问密码只作用于 HTTP/API；FTP、WebDAV、rsync 仍只接受主密码。
- 公开部署请设置 `VFILES_AUTH_ALLOW_REGISTER=false`、强随机 `VFILES_AUTH_COOKIE_SECRET`，并按需显式开启 FTP/rsync。
