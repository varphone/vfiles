# rsync 协议 30 · push（客户端 sender → 本端 receiver）实装记录（r7 → r12）

## 形态差异（相对 download）

| 项 | download（r9/r10） | push（本轮） |
| --- | --- | --- |
| 客户端 args | 含 `--sender` | **不含 `--sender`**（flag 簇 `-re.iLsfxCIvu`） |
| `--server` 后 | 本端 = sender（发 flist、发数据） | 本端 = receiver（**收 flist、发请求、收数据**） |
| filter list | 本端（sender）**读**客户端发来的规则 | **不读**（客户端 `am_sender=1` 且 receiver_wants_list=false → 不发） |
| flist | 本端编码发送 | 本端**解码接收**（`recv_file_list`） |
| 请求帧 | 本端读 | 本端**发**：`write_ndx(i)` + `shortint(0xA000=ITEM_TRANSFER\|ITEM_IS_NEW)` + **sum_head 全零** |
| 数据帧 | 本端发（literal 或 delta token） | 本端收（sum_head 全零 → 客户端全 literal） |
| 收尾 | 读 4×DONE、发 3×DONE | **发 4×DONE、读 3×DONE**（超时防挂） |

## 收端 flist 解码（`recv_file_entry` 逆序）

`xflags`(varint) → `[l1 byte if SAME_NAME]` → `[l2 varint if LONG_NAME else byte]` → `name = lastname[..l1] + suffix`
→ `length=varlong3` → `[mtime=varlong4 if !SAME_TIME]` → `[nsec varint if MOD_NSEC]` →
`[mode=int32 LE if !SAME_MODE]` → `[uid varint if !SAME_UID]` → `[gid varint if !SAME_GID]` →
`[symlink: varint30 len + bytes]` → 循环至 `varint(0)`（varint 模式再读 io_error varint）。

- `preserve_uid/gid=false`（默认无 `-o/-g`）→ uid/gid 段跳过。
- **ndx = 排序后下标**：收端 flist 必须用与 sender 相同的 `f_name_cmp` 序（复用 `sort_flist`），
  否则请求 ndx 与客户端 `files[ndx]` 错位（会请求错文件）。

## 请求 / 数据帧

请求（本端 → 客户端）：
```
write_ndx(i) | iflags(LE16 = 0xA000) | sum_head = count=0, blength=0, s2length=0, remainder=0
```
→ 客户端 `receive_sums` 读得 count=0 → `match_sums` 全 literal（不计算 delta）。

应答（客户端 → 本端）：
```
write_ndx(i) | iflags(LE16) | sum_head 回显(16B) | [int32(len) + 原始字节]* | int32(0) | MD5(内容) 16B
```

## 实装与验收（`crates/vfiles-rsync/src/lib.rs`）

`recv_file_list` + `data_varint`/`data_varlong`/`data_byte`/`int_byte_extra`（io.c 移植）+
`handle_conn` 第五参 `write_file` 闭包（bin 侧接 `UploadService` 上传链）。

真 `rsync 3.2.7` 验收：

| 场景 | 结果 |
| --- | --- |
| `rsync -r /tmp/src/ rsync://…/files/push1/` | RC0、服务端日志 `rsync push 完成 files=3` |
| 推后 `--list-only` | `. / a.txt / tiny.txt / sub`（tree 正确） |
| 推后 `rsync -r …/push1/ /tmp/pull-check/` + `diff -r /tmp/src` | **IDENTICAL** |
| 3MiB 随机 + 深层嵌套 + UTF-8 名推送 → 拉回 `cmp` | 3/3 逐字节同 |
| 重复推送（幂等） | RC0 |
| 推后对该文件下载 `-I`（r11 delta 回归） | Literal 1,768 / Matched 3,143,960 + 重建正确 |

## 债

- 收端**无 basis/delta**（恒请求整文件 ✗ sum_head 全零）：带宽优化 = 后续轮。
- 收端不比较 mtime/size（恒传输 = 等价 `-I`）。
- 符号链接/设备/硬链接不落地（读走即弃）。
- 空目录不创建（父目录由上传链隐式创建）。
- **无认证**（匿名可写 ✗ `VFILES_RSYNC_ENABLED=true` 显式开启 = 运维自担；secrets 认证 = 后续轮）。
- `--delete` 推送未支持（客户端会发 filter list = 会解码失败，记档）。
