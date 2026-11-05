# Живой прогон paghbox 1.3.0

Дата: 2026-11-05. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh`
(main 0.16.0; `uname -a` в этом билде печатает
`pagh pagh-riscv 0.10.0 0.16.1 riscv64` — константы release/version ядра,
версия пакета тут ни при чём). QEMU `qemu-system-riscv64` 10.2.2, машина
`virt`, 256 MiB, serial на Unix-сокете, `-bios default`, modern virtio
(`-global virtio-mmio.force-legacy=false`). Диск: 64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне — `paghbox`
1.3.0 и фикстуры. Драйвер — [`tools/live_test.py`](../tools/live_test.py):
17 загрузок по 1–4 команды, после каждой проверялись вывод и
`run: /mnt/paghbox exited with code N`.

Сырой транскрипт: [`live-1.3.0-transcript.txt`](live-1.3.0-transcript.txt).

## Итог

**50 PASS, 0 FAIL.** Все 12 новых applet'ов проверены функционально, включая
коды выхода 0/1/2:

* `rmdir` (`-p`, несколько операндов, `ENOTEMPTY`), `tac`, `sum` (BSD и SysV),
  `cksum` (POSIX CRC-32), `base64`/`base32` (encode/decode, `-w`), `diff`
  (equal/change/add/delete), `cmp` (`-s`, `-l`, EOF), `sed` (адрес, `p`, `d`,
  `s///g`, пустой матч), `awk` (`-F`, поля, `NR`, `/re/`, сравнения),
  `shuf` (`-n`, `-e`, `-i`), `numfmt` (`--to`, `--from`, `--to-unit`,
  `--suffix`, `--padding`, ошибка числа).
* `uname -a` подтверждает загрузку ядра 0.16.0 (см. примечание о константах
  выше).
* Серия сделана финальным бинарником 1.3.0; `sed s/a*/X/g` проверяет
  подавление пустого матча после непустого, чтобы результат совпадал с GNU.

## PASS-строки

```text
PASS uname: run /mnt/paghbox uname -a
PASS rmdir -p: run /mnt/paghbox rmdir -p p1/p2/p3
PASS rmdir multiple: run /mnt/paghbox rmdir /mnt/d_empty /mnt/d_nonempty
PASS tac: run /mnt/paghbox tac /mnt/lines.txt
PASS sum: run /mnt/paghbox sum /mnt/hello.txt
PASS sum -s: run /mnt/paghbox sum -s /mnt/hello.txt
PASS cksum: run /mnt/paghbox cksum /mnt/hello.txt
PASS base64: run /mnt/paghbox base64 /mnt/hello.txt
PASS base64 -w 4: run /mnt/paghbox base64 -w 4 /mnt/hello.txt
PASS base32: run /mnt/paghbox base32 /mnt/hello.txt
PASS base32 -w 8: run /mnt/paghbox base32 -w 8 /mnt/base.txt
PASS base64 -d: run /mnt/paghbox base64 -d /mnt/hello.b64
PASS base32 -d: run /mnt/paghbox base32 -d /mnt/hello.b32
PASS diff equal: run /mnt/paghbox diff /mnt/same1.txt /mnt/same2.txt
PASS diff change: run /mnt/paghbox diff /mnt/diff1.txt /mnt/diff2.txt
PASS cmp first: run /mnt/paghbox cmp /mnt/cmp1.txt /mnt/cmp2.txt
PASS cmp -l: run /mnt/paghbox cmp -l /mnt/cmp1.txt /mnt/cmp2.txt
PASS cmp -s: run /mnt/paghbox cmp -s /mnt/cmp1.txt /mnt/cmp2.txt
PASS cmp equal: run /mnt/paghbox cmp /mnt/same1.txt /mnt/same2.txt
PASS cmp eof: run /mnt/paghbox cmp /mnt/eof1.txt /mnt/eof2.txt
PASS sed address p: run /mnt/paghbox sed -n 2p /mnt/sed.txt
PASS sed substitute: run /mnt/paghbox sed s/a/X/g /mnt/sed.txt
PASS sed delete range: run /mnt/paghbox sed 2,3d /mnt/sed.txt
PASS sed two commands: run /mnt/paghbox sed -n 1p;2p /mnt/sed.txt
PASS sed a* global: run /mnt/paghbox sed s/a*/X/g /mnt/sed.txt
PASS awk compare: run /mnt/paghbox awk -F: $2>20{print$1,$2} /mnt/awk.txt
PASS awk string: run /mnt/paghbox awk -F: $1==banana{print$2} /mnt/awk.txt
PASS awk regex/NR: run /mnt/paghbox awk /a/{print$1,NR} /mnt/awk.txt
PASS awk NR: run /mnt/paghbox awk -F: NR==2{print$2} /mnt/awk.txt
PASS shuf -n: run /mnt/paghbox shuf -n 3 /mnt/shuf.txt
PASS shuf -e: run /mnt/paghbox shuf -e a b c
PASS shuf -i: run /mnt/paghbox shuf -i 1-5
PASS numfmt si: run /mnt/paghbox numfmt --to=si 1000 1500 1000000
PASS numfmt iec: run /mnt/paghbox numfmt --to=iec 1024 1536
PASS numfmt from: run /mnt/paghbox numfmt --from=si 1.5K
PASS numfmt pad/suffix: run /mnt/paghbox numfmt --to=si --suffix=B --padding=8 1000
PASS numfmt auto: run /mnt/paghbox numfmt --to=auto 1024 1500 1000000
PASS numfmt unit: run /mnt/paghbox numfmt --to=si --to-unit=1000 5000
PASS numfmt from iec: run /mnt/paghbox numfmt --from=iec 1Ki 2M
PASS numfmt invalid: run /mnt/paghbox numfmt --to=si abc
PASS diff insert: run /mnt/paghbox diff /mnt/diff_ins.txt /mnt/diff1.txt
PASS diff delete: run /mnt/paghbox diff /mnt/diff1.txt /mnt/diff_del.txt
PASS base64 invalid: run /mnt/paghbox base64 -d /mnt/invalid.b64
PASS rmdir usage: run /mnt/paghbox rmdir
PASS sed usage: run /mnt/paghbox sed
PASS awk usage: run /mnt/paghbox awk
PASS base64 option: run /mnt/paghbox base64 -z
PASS tac missing: run /mnt/paghbox tac /mnt/nope
PASS sum missing: run /mnt/paghbox sum /mnt/nope
PASS shuf bad range: run /mnt/paghbox shuf -i 5-1
```

## Показательные выводы

```text
run /mnt/paghbox rmdir -p p1/p2/p3          # cwd=/mnt
run: /mnt/paghbox exited with code 0

run /mnt/paghbox rmdir /mnt/d_empty /mnt/d_nonempty
rmdir: /mnt/d_nonempty: Directory not empty
run: /mnt/paghbox exited with code 1

run /mnt/paghbox tac /mnt/lines.txt
three
two
one

run /mnt/paghbox sum /mnt/hello.txt
36979     1 /mnt/hello.txt

run /mnt/paghbox sum -s /mnt/hello.txt
542 1 /mnt/hello.txt

run /mnt/paghbox cksum /mnt/hello.txt
3015617425 6 /mnt/hello.txt

run /mnt/paghbox base64 -w 4 /mnt/hello.txt
aGVs
bG8K

run /mnt/paghbox base32 -w 8 /mnt/base.txt
NBSWY3DP
EB3W64TM
MQFA====

run /mnt/paghbox diff /mnt/diff1.txt /mnt/diff2.txt
2c2
< b
---
> B
4c4
< d
---
> e
run: /mnt/paghbox exited with code 1

run /mnt/paghbox cmp /mnt/cmp1.txt /mnt/cmp2.txt
/mnt/cmp1.txt /mnt/cmp2.txt differ: byte 5, line 2, offset 0x4
run: /mnt/paghbox exited with code 1

run /mnt/paghbox cmp -l /mnt/cmp1.txt /mnt/cmp2.txt
5 144 170

run /mnt/paghbox sed s/a*/X/g /mnt/sed.txt
XlXpXhX
XbXeXtX4X2X
XGXmXmX
XdXeXlXtX

run /mnt/paghbox awk -F: $2>20{print$1,$2} /mnt/awk.txt
banana 25
date 30

run /mnt/paghbox awk /a/{print$1,NR} /mnt/awk.txt
apple:10 1
banana:25 2
date:30 4

run /mnt/paghbox shuf -i 1-5
5
3
2
4
1

run /mnt/paghbox numfmt --to=si --suffix=B --padding=8 1000
   1.0kB
```

## `pkg install` на 0.16.0 (опционально)

Отдельная загрузка с `--net` и чистым 64 MiB ext2: `repo/` отдан
`python3 -m http.server 8000 --bind 127.0.0.1 -d repo`; гость видит хост как
`10.0.2.2:8000`.

```text
pagh:rv64> pkg repo http://10.0.2.2:8000/
pkg: repo http://10.0.2.2:8000/
pagh:rv64> pkg index
pkg: index 1 package(s)
pagh:rv64> pkg install paghbox
pkg: installed paghbox 1.3.0
```

Повторная загрузка того же образа (без сети) проверяет симлинки установщика:

```text
pagh:rv64> run /mnt/bin/tac /mnt/lines.txt
three
two
one
run: /mnt/bin/tac exited with code 0
pagh:rv64> run /mnt/bin/numfmt --to=si 1500
1.5k
run: /mnt/bin/numfmt exited with code 0
```

`ls /mnt/bin` показывает `paghbox` и 67 симлинков, включая `rmdir`, `tac`,
`sum`, `cksum`, `base64`, `base32`, `diff`, `cmp`, `sed`, `awk`, `shuf`,
`numfmt`. `MAX_COMMANDS = 160` покрывает пакет; SHA-256 и размер в `index`
проверены установщиком до записи.

## Чем проверка ограничена

* Шелл ядра не понимает кавычки, пайпы и редиректы; `awk`-программа с
  пробелами недоступна, поэтому использованы компактные `{print$1,$2}` и
  `-F:`. `sed`-скрипт тоже передан одним словом.
* У консольного fd 0 нет EOF. Все 12 инструментов принимают файл или `-`
  (stdin) через `util::Input`, но живым запуском проверены файловые операнды;
  stdin-ветки без пайпа не проверить. Это ограничение среды, а не заглушка в
  коде.
* `tr` по-прежнему проверен только разбором аргументов (см. историю 1.1.0).
* Serial-драйвер иногда теряет длинную очередь ввода; `tools/live_test.py`
  поэтому делает 1–4 команды на загрузку и один автоматический retry на
  свежем образе.
* `shuf` детерминирован (фиксированный seed) из-за отсутствия `getrandom` в
  целевом ABI; ожидаемые последовательности в PASS-строках это учитывают.
* `pkg install` проверялся отдельно (см. README / отчёт), основной прогон
  использует `run /mnt/paghbox`, чтобы не смешивать проверку applet'ов с
  сетью.
