# r5 wire 转录解剖（官方 daemon 18873 全字节 · strace 直挂 -f 抓取）

## 五层时序
1. 服→客 banner: @RSYNCD: 31.0 sha512 sha256 sha1 md5 md4\n (41B)
2. 客→服 banner 同形; 客→服 GOLD\n; 服→客 @RSYNCD: OK\n
3. args（NUL 分隔双 NUL 尾）: --server\0--sender\0-de.LsfxCIvu\0--list-only\0.\0GOLD/\0\0
4. 服→客 0x81 0xFE (2B) → 客→服 0x1E(=30) + 'xxh128 xxh3 xxh64 md5 md4 sha1'(30B)
   → 服→客 '#' + 'xxh128 xxh3 xxh64 md5 md4 sha1 none' + \n (35B = 算法选定) + 4B 'F\x19\x14g'
5. mux/flist 57B 铁证帧 (服→客):
   35 00 00 07 A0 19 01 2E 00 00 10 6A 51 86 B3 EB 6C 39 31 FD 41 00 00 A1 9A 03 73 75 62 00
   00 10 EB 3D D2 47 A0 98 05 61 2E 74 78 74 00 0D 00 EB 3D D2 47 B4 81 00 00 00 00
   锚: '03'+'sub'+\0 与 '05'+'a.txt'+\0 = name = 1B len + bytes 双证 ✓ 定位 mtime/mode 段 = r6 源码对齐
6. 客↔服 终结帧族: 01 00 00 07 00 / 04 00 00 07 FF FF FF FF / 08 00 00 21 58 00 00 00 00 00 00 00 (双 fd mux 对话)
7. 客→服 02 00 00 07 00 00 / 03 00 00 07 00 00 00 / 10 00 00 07 00 14 00 00 44 00 00 0D 00 00 01 00 00 00 00 00

原始转录: client_strace_raw.txt (34KB, 完整双端 read/write 分片)
