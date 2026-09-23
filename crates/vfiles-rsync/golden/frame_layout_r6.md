# 57B flist 帧布局反推（round 6 · 三锚值域法）

输入 = strace 57B read（`golden/client_strace_raw.txt` 内 `5\0\0\7\240\31\1.…` 串）+ 已知值域
（golden ls 输出：mode 0o100664/0o40775、size 13/4096、mtime 2026-09-23 15:57:05 +08 = epoch
1790150225 = 0x6ab38651）。

## 已解字段（铁证）

| 字段 | 结论 | 证据 |
| --- | --- | --- |
| **name** | **1B 长度 + bytes + `\0`** | 三锚：`.`(pre 0x01 @6) · `sub`(pre 0x03 @26) · `a.txt`(pre 0x05 @39) = 三独立长度字节 ✓✓✓ |
| **mode** | **u32 LE** | 0o40775(=drwxrwxr-x 目录) @19 · 0o100664(=文件) @51 双命中 ✓（与 golden ls 的 `drwxrwxr-x`/`-rw-rw-r--` 值一致 ✓） |
| **size** | **u16** | 4096 = `00 10` LE @9/@30 双命中、13 = `0D 00` @45 ✗ 目录与文件各一处 ✓ |
| 头部 | `35 00 00 07 A0 19` 前缀 = mux/flist 头层（长度/flag 组合待 r7 与 mtime 一并源对） | `.` name @6 属头部后 |

## mtime 未解（r7 权威通道）

- epoch 1790150225 的 **u32 BE/LE、u64、±1 天全扫、float32/64 全部零命中** ✗
- 帧中 `6a 51 86 b3`(@11-14) 与 epoch 字节 `6a b3 86 51` = **中间两对交错**（分组/交换编码，
  疑 16 位半段重排或 varint 变形）
- 双网络源失：GitHub raw flist.c web_fetch 30s 超时 ✗、manpage.me 反爬壳(312 词无正文) ✗
- **r7 权威通道**：GitHub **blob 带行号页**（渲染含源码行，取 flist.c L1841/L3204 段 =
  send/read_file_entry 的 mtime 写读路径）→ 定编码 → 五层实装（args/0x1E/#回/mux+flist/
  终结对答）→ 终极探针 `rsync --list-only rsync://…/files/` = RC0 列 vfiles 文件
