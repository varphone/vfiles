# rsync 协议 30 · delta 块校验和 + 滚动静默规则（r5 真机实证 = r11 实装钥匙）

来源：官方 `rsync 3.2.7 --daemon`（`RSYNC_CHECKSUM_LIST=md5`）+ 客户端
`rsync --protocol=30 -I rsync://127.0.0.1:8878/g3/pat.bin /tmp/basis/`（已有 basis = 走 delta）
✗ `strace -f -xx -e trace=read,write` 按 fd3 提取接收端请求帧。

## 一、基准数据

`pat.bin` = 200000B，`bytes((i*7+3)%256)`；basis = 同文件（首轮已下载）。

## 二、请求帧（客户端 → 服务端，mux `c7 06 00 07` = 1735B payload）

```
01                    # write_ndx(0)
08 80                 # write_shortint(iflags = 0x8008 = ITEM_REPORT_TIME|ITEM_TRANSFER)
1e 01 00 00           # sum_head.count     = 286
bc 02 00 00           # sum_head.blength   = 700
02 00 00 00           # sum_head.s2length  = 2      ← 首轮强校验和只取 2 字节
f4 01 00 00           # sum_head.remainder = 500    （285*700+500 = 200000 ✓）
da fe 0c 76           # block0.sum1（int32 LE = 0x760cfeda）
25 91                 # block0.sum2（s2length=2 字节）
...                   # 286 块 × (4B sum1 + 2B sum2)
```

## 三、块校验和公式（逐位实证）

1. **弱校验和 `sum1`** = rsync `get_checksum1`（`CHAR_OFFSET=0`，**`schar` 有符号字节**）：
   ```
   s1 = Σ (i8)byte[i]                (mod 2^32)
   s2 = Σ s1                          (逐步累加, mod 2^32)
   sum1 = (s1 & 0xffff) | (s2 << 16)
   ```
   实证：`pat[0..700]` 计算 = `0x760cfeda`，与转录 `da fe 0c 76` **逐位同** ✓
2. **强校验和 `sum2`** = `MD5(seed_le ‖ block)` 的前 `s2length` 字节
   （`get_checksum2` 的 `proper_seed_order=1` 分支 = `CF_CHKSUM_SEED_FIX`，客户端 `-e` 串含 `C`）。
   实证：seed=`76 9d 58 67`（LE）→ `MD5(seed‖pat[0..700])[:2]` = `25 91` 与转录 **同** ✓
   ✗ 反证：`MD5(block)` = `4b19`、`MD5(block‖seed)` = `82fa` 均不命中。
   ⚠ **与整文件校验和不同**：整文件 = `MD5(内容)` **无 seed**（streaming `sum_update` 路径）。

## 四、滚动更新（match.c `hash_search` null_hash 分支）

```
去首：s1 -= (i8)data[offset];  s2 -= k * (i8)data[offset]
加尾：若 offset+k < len: s1 += (i8)data[offset+k]; s2 += s1
      否则: k -= 1   （窗口收缩 = 末尾短块对齐）
```

## 五、r11 实装结果（`crates/vfiles-rsync/src/lib.rs`）

`BlockSum{sum1,sum2,len}` + `build_delta_tokens`（弱命中→强确认→匹配 token `-(idx+1)`；
未命中段 literal `int32(len)+data`；终结 `int32(0)`）+ `md5_seeded`。

真机验收（`sensor_pts_diag_xtr_*.csv` 2,254,652B）：

| 场景 | Literal | Matched |
| --- | --- | --- |
| 首轮（无 basis） | 2,254,652 | 0 |
| 改 1000B 后 `-I` | **2,992** | **2,251,660**（99.87%） |
| 再跑 `-I`（内容同） | **0** | **2,254,652**（100%） |
| 递归树 4 文件改 500B `-I` | **1,448** | **70,688,460** |

落地内容 SHA-256 全部与 `blobs` 表一致 ✓（delta 重建逐字节正确）。
