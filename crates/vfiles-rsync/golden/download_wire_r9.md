# rsync 协议 30 · 整文件下载（sender 侧）真机转录（r9 → r10 实装钥匙）

来源：官方 `rsync 3.2.7 --daemon`（`RSYNC_CHECKSUM_LIST=md5`）+ 客户端
`rsync --protocol=30 rsync://127.0.0.1:8873/gold/tiny.txt /tmp/dl-md5/`（新建目标 = 无 basis =
count 0 全 literal）✗ `strace -f -xx -e trace=read,write` 按 fd3 解析。

## 一、目标文件

`tiny.txt` 内容 = `tiny-content-12345\n`（19 字节），mode `0o100664`，size 19。

## 二、完整字节序列（协议 30 · 协商 md5）

**客户端 → 服务端（按序）**
1. banner `@RSYNCD: 30.0 sha512 sha256 sha1 md5 md4\n`
2. `gold\n` → 收 `@RSYNCD: OK\n`
3. args：`--server\0--sender\0-e.LsfxCIvu\0.\0gold/tiny.txt\0\0`
   （单文件请求 = **无 `-d`/`--list-only`**，flag 簇退化为 `-e.LsfxCIvu`）
4. checksum vstring：`1e` + `xxh128 xxh3 xxh64 md5 md4 sha1`（30B）
5. filter list mux：`04 00 00 07 00 00 00 00`

**服务端 → 客户端（按序）**
1. banner（`@RSYNCD: 30.0 …`）
2. `@RSYNCD: OK\n`
3. `81 FE`（compat_flags 0x1FE varint）
4. checksum 清单 vstring：`23` + `md5`（env 强制 = 单算法 → 客户端必选 md5）
5. checksum_seed 4B（本转录 `19 f5 7d 67`）
6. **flist 帧**（`17 00 00 07` = 23B payload）：
   ```
   18 08 74 69 6e 79 2e 74 78 74 00 13 00 6a 56 a8 b3 b4 81 00 00 00 00
   ```
   - `18` = xflags（SAME_UID|SAME_GID；**单文件请求无 `.` 条目**、无 TOP_DIR/SAME_*）
   - `08` + `tiny.txt` = l2 + name（**name = basename**）
   - `00 13 00` = length varlong3 = 19
   - `6a 56 a8 b3` = mtime varlong4
   - `b4 81 00 00` = mode LE（0o100664）
   - `00 00` = flist 结束

**客户端 → 服务端（请求帧，`13 00 00 07` = 19B payload）**
```
01 00 a0 | 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```
- `01` = `write_ndx(0)`（prev_positive=-1 → diff 1）
- `00 a0` = `write_shortint(iflags = 0xA000)` = **ITEM_TRANSFER(1<<15) | ITEM_IS_NEW(1<<13)**
  （LE：a0 00）
- 16B = receiver 的 `sum_head`：`count=0, blength=0, s2length=0, remainder=0`
  （新建目标无 basis → count 0 → **无块校验和跟随**）

**服务端 → 客户端（数据帧，`3e 00 00 07` = 62B payload）**
```
01 00 a0                                   # ndx 回显 + iflags 回显（write_ndx_and_attrs）
00 ×16                                     # write_sum_head 回显 receiver 的 sum_head
13 00 00 00                                # literal 长度 19（write_int）
74 69 6e 79 2d 63 6f 6e 74 65 6e 74 2d 31 32 33 34 35 0a   # 原始字节
00 00 00 00                                # 终结 token（write_int(0)）
1b c5 9b d9 ac cf 79 fe 78 19 0f d1 bc 1b 21 a8   # 文件校验和
```

**收尾（同 list-only）**：收 3×NDX_DONE → 发 2×NDX_DONE + 终结 NDX_DONE + 5×varlong30(3) 统计
→ 末 NDX_DONE 问候。

## 三、关键结论（r10 可直接照抄）

1. **文件校验和 = `MD5(文件内容)`，无 seed**：`md5(content)` =
   `1bc59bd9accf79fe78190fd1bc1b21a8` 与转录**逐字节同** ✗ 官方 `sum_update`（checksum.c:643）
   对 MD5 只 `md5_update(data)`；seed 仅进 `get_checksum2`（块校验和）✗ 故整文件路径不需要 seed。
2. **必须只 advertise `md5`**：若照抄官方全清单（`xxh128 …`），双方协商出 **xxh128**（16B，
   需 XXH3-128 实现）✗ 收敛到 `"md5"` 单算法 = 客户端必选 md5 = 纯 MD5 可验。
3. **token 流（无压缩 = `simple_send_token`）**：每 32KB（`CHUNK_SIZE`）一块
   `write_int(len) + 原始字节`（token -2，不写 token int），最后一次 `write_int(remainder) +
   字节 + write_int(0)`（token -1）✗ 等价实现 = 分块 `int32(len)+data` 后统一 `int32(0)`。
4. **全 literal 对 count>0 也合法**：sender 可忽略 receiver 的块校验和直接全 literal 发
   （正确但无 delta 收益）→ r10 可先做整文件，真 delta（rolling + strong 匹配）后置。
5. **ndx = 排序后下标**：`generator.c:2804` 生成端遍历 `cur_flist->sorted[i]`；非 inc 模式下
   `sorted == files` 且双方都排序 → **我方 flist 必须先按 rsync `f_name_cmp` 序排好**，否则
   ndx↔条目错位（会发错文件）。f_name_cmp 细节（flist.c `f_name_cmp`）：目录视为 `t_PATH`、
   非目录 `t_ITEM`，`.` 特判 `s_TRAILING`（排最前）✗ r10 需按此实现比较器并用真机下载校验。
6. **单文件请求**：flist 无 `.`，name = basename；`-r` 目录请求才有 `.` 首条 + 全相对路径名。
7. **依赖**：`md-5 0.10.6` 已在本机 cargo registry（Cargo.lock 亦有）✗ 离线可加。

## 四、r10 待接（由本转录解锁）

- `FlatEntry` 增 `fs_path`（namespace 内读取路径）+ 源接口增 `read_file(path)`
- `handle_conn` 的 send_files 循环：`ndx>=0` → 读 iflags（+BASIS/XNAME）→ 读 sum_head+块
  → `write_ndx_and_attrs` 回显 → `write_sum_head` 回显 → 全 literal → MD5 → 继续循环
- entries 按 f_name_cmp 排序后再编码（ndx 对齐）
- 真机验收：`rsync -r rsync://host/files/<dir>/ /tmp/out/` = RC0 + `cmp` 逐字节同
