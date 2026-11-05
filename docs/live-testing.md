# Живой прогон paghbox 1.1.0

Дата: 2026-11-05. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh`
(0.10.1, native ABI). QEMU: `qemu-system-riscv64` 10.2.2, машина `virt`, 256 MiB,
serial на Unix-сокете, `-bios default`. Диск: 64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне образа лежат
`paghbox` 1.1.0 и фикстуры. Одна загрузка, 81 команда `run /mnt/paghbox ...`
подряд; после каждой — `run: /mnt/paghbox exited with code N`.

Сырой транскрипт: [`live-1.1.0-transcript.txt`](live-1.1.0-transcript.txt).

## Итог

81 PASS, 0 FAIL. 24 из 25 новых applet'ов проверены функционально (включая
`ls/stat/readlink/realpath`, все текстовые фильтры, `grep` с regex-подмножеством).
`tr` по спецификации читает только stdin, а у консольного fd 0 в ядре 0.10.1
нет EOF и нет пайпов/редиректа, поэтому живым запуском проверен только разбор
аргументов (`tr`, `tr -d z-a`, `tr a b c`, `tr -z a b` → exit 2); само
преобразование проверяется компиляцией/ревью и остаётся ограничением среды.

## PASS-строки

```text
PASS uname: run /mnt/paghbox uname
PASS uname: run /mnt/paghbox uname -a
PASS uname: run /mnt/paghbox uname -sn
PASS uname: run /mnt/paghbox uname -z
PASS hostname: run /mnt/paghbox hostname
PASS arch: run /mnt/paghbox arch
PASS nproc: run /mnt/paghbox nproc
PASS sleep: run /mnt/paghbox sleep 0.2
PASS sleep: run /mnt/paghbox sleep 1.5
PASS sleep: run /mnt/paghbox sleep nope
PASS sleep: run /mnt/paghbox sleep .
PASS uptime: run /mnt/paghbox uptime
PASS date: run /mnt/paghbox date
PASS ls: run /mnt/paghbox ls -l /mnt
PASS ls: run /mnt/paghbox ls /mnt/dir1
PASS ls: run /mnt/paghbox ls -d /mnt/dir1
PASS ls: run /mnt/paghbox ls /mnt/hello.txt /mnt/dir1
PASS ls: run /mnt/paghbox ls /mnt/missing
PASS ls: run /mnt/paghbox ls -a /mnt/dir1
PASS stat: run /mnt/paghbox stat /mnt/hello.txt
PASS stat: run /mnt/paghbox stat /mnt/dir1
PASS stat: run /mnt/paghbox stat /mnt/missing
PASS readlink: run /mnt/paghbox readlink /mnt/link
PASS readlink: run /mnt/paghbox readlink /mnt/hello.txt
PASS realpath: run /mnt/paghbox realpath /mnt/dir1/../hello.txt
PASS realpath: run /mnt/paghbox realpath /mnt/./dir1/./sub/../..
PASS uniq: run /mnt/paghbox uniq /mnt/uniq.txt
PASS uniq: run /mnt/paghbox uniq -c /mnt/uniq.txt
PASS uniq: run /mnt/paghbox uniq -d /mnt/uniq.txt
PASS uniq: run /mnt/paghbox uniq -u /mnt/uniq.txt
PASS sort: run /mnt/paghbox sort /mnt/sort.txt
PASS sort: run /mnt/paghbox sort -r /mnt/sort.txt
PASS sort: run /mnt/paghbox sort -n /mnt/nums.txt
PASS sort: run /mnt/paghbox sort -n -r /mnt/nums.txt
PASS sort: run /mnt/paghbox sort -u /mnt/dup.txt
PASS nl: run /mnt/paghbox nl /mnt/nl.txt
PASS nl: run /mnt/paghbox nl -b a /mnt/nl.txt
PASS fold: run /mnt/paghbox fold -w 4 /mnt/fold.txt
PASS fold: run /mnt/paghbox fold -w 0 /mnt/fold.txt
PASS expand: run /mnt/paghbox expand -t 4 /mnt/tabs.txt
PASS unexpand: run /mnt/paghbox unexpand /mnt/spaces.txt
PASS unexpand: run /mnt/paghbox unexpand -a /mnt/spaces.txt
PASS rev: run /mnt/paghbox rev /mnt/rev.txt
PASS cut: run /mnt/paghbox cut -d : -f 2 /mnt/cut.txt
PASS cut: run /mnt/paghbox cut -d : -f 1,3 /mnt/cut.txt
PASS cut: run /mnt/paghbox cut -d : -f 2- /mnt/cut.txt
PASS cut: run /mnt/paghbox cut /mnt/cut.txt
PASS comm: run /mnt/paghbox comm /mnt/comm1.txt /mnt/comm2.txt
PASS comm: run /mnt/paghbox comm -1 /mnt/comm1.txt /mnt/comm2.txt
PASS comm: run /mnt/paghbox comm -12 /mnt/comm1.txt /mnt/comm2.txt
PASS comm: run /mnt/paghbox comm -3 /mnt/comm1.txt /mnt/comm2.txt
PASS paste: run /mnt/paghbox paste -d, /mnt/paste1.txt /mnt/paste2.txt
PASS paste: run /mnt/paghbox paste /mnt/paste1.txt /mnt/paste3.txt
PASS strings: run /mnt/paghbox strings -n 4 /mnt/strings.bin
PASS strings: run /mnt/paghbox strings -n 6 /mnt/strings.bin
PASS hexdump: run /mnt/paghbox hexdump -C /mnt/hex.bin
PASS hexdump: run /mnt/paghbox hexdump /mnt/hex.bin
PASS grep: run /mnt/paghbox grep alpha /mnt/grep.txt
PASS grep: run /mnt/paghbox grep -i beta /mnt/grep.txt
PASS grep: run /mnt/paghbox grep -n -i beta /mnt/grep.txt
PASS grep: run /mnt/paghbox grep -c -i beta /mnt/grep.txt
PASS grep: run /mnt/paghbox grep -v foo /mnt/grep.txt
PASS grep: run /mnt/paghbox grep -F [test] /mnt/grep.txt
PASS grep: run /mnt/paghbox grep ^[a-z]*$ /mnt/grep.txt
PASS grep: run /mnt/paghbox grep a.*a /mnt/grep.txt
PASS grep: run /mnt/paghbox grep a$ /mnt/grep.txt
PASS grep: run /mnt/paghbox grep ^B /mnt/grep.txt
PASS grep: run /mnt/paghbox grep ^beta[0-9][0-9]$ /mnt/grep.txt
PASS grep: run /mnt/paghbox grep -l beta /mnt/grep.txt /mnt/hello.txt
PASS grep: run /mnt/paghbox grep -n beta /mnt/grep.txt /mnt/hello.txt
PASS grep: run /mnt/paghbox grep -c beta /mnt/grep.txt /mnt/hello.txt
PASS grep: run /mnt/paghbox grep zzz /mnt/grep.txt
PASS grep: run /mnt/paghbox grep [ /mnt/grep.txt
PASS grep: run /mnt/paghbox grep
PASS tr: run /mnt/paghbox tr
PASS tr: run /mnt/paghbox tr -d z-a
PASS tr: run /mnt/paghbox tr a b c
PASS tr: run /mnt/paghbox tr -z a b
PASS nproc: run /mnt/paghbox nproc 2
PASS hostname: run /mnt/paghbox hostname extra
PASS arch: run /mnt/paghbox arch extra
```

## Попытка `pkg install` (0.10.1)

Отдельная загрузка с чистым диском и HTTP-репозиторием `repo/` (порт 8000):

```text
pagh:rv64> pkg repo http://10.0.2.2:8000/
pkg: repo http://10.0.2.2:8000/
pagh:rv64> pkg index
pkg: index 1 package(s)
pagh:rv64> pkg list
paghbox 1.1.0 298732 B
pagh:rv64> pkg install paghbox
pkg: install: package manifest is invalid
pagh:rv64> pkg installed
pkg: 0 package(s) installed
```

Пакет скачан и прошёл проверку размера/SHA-256 (иначе ошибка была бы другой),
но установщик 0.10.1 ограничен `MAX_COMMANDS = 32` и отвергает манифест с 37
командами. Это ограничение ядра, а не формата; прямой `run /mnt/paghbox`
работает, что и проверено в основном прогоне.

Контрольный пакет с тем же ELF и манифестом, но `commands=` из первых 32 имён,
установился на том же ядре: `pkg: installed paghbox 1.1.0`, в `/mnt/bin`
появились 32 ссылки, а `run /mnt/bin/sort -n /mnt/nums.txt` напечатал
`-3 -0.5 0 02 2 3.5 7 10` и завершился с кодом 0. Значит, пакет, подпись и ELF
корректны; отказ вызван именно лимитом 32 команды.

## Показательные выводы

```text
run /mnt/paghbox uname -a
pagh pagh-riscv 0.10.0 0.10.1 riscv64

run /mnt/paghbox ls -l /mnt
drwxr-xr-x 4096 dir1/
-rw-r--r-- 35 hello.txt
-rwxr-xr-x 298288 paghbox

run /mnt/paghbox stat /mnt/hello.txt
/mnt/hello.txt: file, size=35, mode=0o644, ino=23

run /mnt/paghbox sort -n /mnt/nums.txt
-3
-0.5
0
02
2
3.5
7
10

run /mnt/paghbox grep -n -i beta /mnt/grep.txt
2:Beta
4:beta42

run /mnt/paghbox comm -1 /mnt/comm1.txt /mnt/comm2.txt
b
	c
	e
```

Последняя строка содержит настоящие табы: `comm` сохраняет позиции колонок,
как GNU (`comm -1` оставляет таб перед общей строкой).
