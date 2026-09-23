# rsync-30 flist 编码法权威定稿（round 7 · git clone 官方源 + mtime 解出）

通道胜利：`git clone --depth 1 github.com/RsyncProject/rsync`（本机 ✗ 避开 web_fetch 30s 墙 +
反爬壳 = r6 双败后的正解）→ flist.c 4003 行本地权威 ✗ 一次解码跑通。

## 核心编码（源引用）

| 机制 | 权威形 | 源 |
| --- | --- | --- |
| **XMIT 位表** | bit0=TOP_DIR · 1=SAME_MODE · 2=EXTENDED · 3=SAME_UID · 4=SAME_GID · **5=SAME_NAME · 6=LONG_NAME · 7=SAME_TIME** · 8=NO_CONTENT_DIR · 9=HLINKED · 10=USER_NAME_FOLLOWS · 11=GROUP_NAME_FOLLOWS · 12=HLINK_FIRST · 13=MOD_NSEC(31) · 14=SAME_ATIME · 17=CRTIME_EQ_MTIME | rsync.h:47-73 |
| **字段写序** | xflags(byte/varint) → l1(if SAME_NAME) → l2(varint30/byte) → name(l2字节) → (hlink) → **length=varlong30(min3)** → **mtime=varlong(min4)if !SAME_TIME** → nsec? → crtime? → **mode=int(LE4)if !SAME_MODE** → atime? → uid(varint)if !SAME_UID → gid → … | flist.c send L700-950 / recv L1002-1300 |
| **varlong(min) 紧凑形** ★ | 首字节=ctrl：`extra=int_byte_extra[ctrl/4]`（0x00-7F→0 · 80-BF→1 · C0-DF→2 · E0-EF→3 · F0-F3→4 · F4-F7→5 · F8-FF→6）→ 读 min-1 字节填 ub[0..min-2] → extra>0 则再读 extra 字节填 ub[min-1..] + ctrl余数(低8-extra位)放 ub[min-1+extra]；extra=0 则 **ub[min-1]=ctrl** → **ub 按 LE(int64 union+IVAL) 读值** | io.c:2056 + 表:185 |
| **varint(32)** | 同表驱动 + ub[0..ex]续 + ctrl余数 → LE 读 u32 | io.c:2024 |
| **int(mtime之前非int部分)** | `read_int`=IVAL=**平台本地序**（x86 = **小端**）✗ CAREFUL_ALIGNMENT 分支逐字节 LE 明证 | byteorder.h:39/92 |
| **shortint** | `(UVAL(b,1)<<8)+UVAL(b,0)` = **大端**（混用记档 ✗ 本帧未触及） | io.c read_shortint |
| **read_varlong30/varint30** | protocol≥30 时=varlong/varint 直通（我们锁 30 ✓） | io.h:21-34 |

## 57B 逐字段解码（首条 = `.` 目录）

- 头（idx0-4）`35 00 00 07 A0` = mux/flist 块头层（**块头语义 = r8 实装时与 strace 前段
  `81 FE 46 19 14 67` 连读一并定** ✗ 实装需要的条目流已不被其阻塞）
- idx5 `19` = **xflags = TOP_DIR|SAME_UID|SAME_GID**（0x01|0x08|0x10 = 语义完美吻合首条
  目录 + !preserve_uid 恒 SAME ✓）→ 无 LONG_NAME/SAME_NAME/SAME_TIME/SAME_MODE 四位
- idx6-7 `01 2E` = **l2=1(byte) + name='.'**
- idx8-10 `00 00 10` = **length=varlong30(min3)**：ctrl=00(extra0) + ub[0..1]=`00 10` →
  LE=0x00001000 = **4096** ✓（与 golden ls 的 4096 一致）
- idx11-14 `6a 51 86 b3` = **mtime=varlong(min4)**：ctrl=0x6a(extra0) + ub[0..2]=
  `51 86 b3` → LE = **0x6AB38651 = 1790150225 = epoch 2026-09-23 15:57:05+08 完全命中**
  ✅✅ r6"交错"谜团 = **ctrl 在首、值 LE 尾接** 的紧凑布局
- idx15+ = mode(LE4)·uid/gid(SAME跳)·其余条目（sub/a.txt 同法逐条）→ r8 实装编码器按
  同一路径逐字节输出

## 实装路径（r8）

args 解析（NUL 双尾）→ `\x81\xFE` 应答 → 收 `\x1E`+checksum 串 → 回 `#<选定>\n` + 4B →
收 mux 帧 → **按上表编码 flist 帧（xflags 按条目差分:首条 TOP|UID|GID、后续沿用 SAME_*）**
→ 终结帧对答（`01/02/03` 族按 strace 配对表）→ 树→条目流（collect_keys 复用 ✗ mode =
st_mode 原值先走 to_wire_mode 同步源 → r8 对齐）→ **终极探针 = 真 `rsync --list-only
rsync://…/files/` = RC0 列 vfiles 文件** → delta（r9）。
