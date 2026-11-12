# Живые прогоны paghbox

## 1.13.0 (procfs/statfs: ps, free, df)

Дата: ночь 2026-11-12/13. Ядро — **тестовое 0.24.0 из ветки `feat/statfs`**
(worktree `/home/oleg/pagh-riscv-feat`, head `1e137d8`), которая стекает три
PR pagh-riscv: #9 `/proc` (`meminfo`/`cpuinfo`/`uptime`/`version`/`mounts`/
`tasks`/`net`), #10 per-pid `/proc` (`<pid>/{status,stat,cmdline}`, `self`,
`kthreads`) и #11 `statfs(43)`/`fstatfs(44)` с нативным 96-байтным `Statfs`.
Исходники ядра только читались: копия `rsync --exclude=target --exclude=.git`
в `/tmp/pagh-night2/riscv-feat-src`, сборка `cargo build --locked --release` там
же. QEMU `qemu-system-riscv64` 10.2.2, `virt`, 256 MiB, диск 64 MiB ext2
(`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`), NIC в user network для
сетевых кейсов регрессии.

Драйвер — [`tools/live_test_1130.py`](../tools/live_test_1130.py): **51
загрузка** (50 на новом ядре + 1 на старом release-ядре 0.21.1), по 1–3
короткие команды, после каждой сверяются вывод, `run: /mnt/paghbox exited with
code N` и валидаторы таблиц. Состав: регрессия 1.8.0–1.12.0 (42 загрузки,
99 проверок) + новые `ps`/`free`/`df` (8 загрузок, 15 проверок) + загрузка
honest-refusal на старом ядре (3 проверки). Итог: **117 PASS, 0 FAIL**.

Сырые транскрипты: [`live-1.13.0-transcript.txt`](live-1.13.0-transcript.txt) и
[`live-1.13.0-pkg-install.txt`](live-1.13.0-pkg-install.txt)
(`pkg install paghbox 1.13.0` на тестовом ядре: 131 симлинк + `paghbox`,
`debugfs` сверяет inode; boot без NIC запускает `/mnt/bin/ps`, `/mnt/bin/free`,
`/mnt/bin/df`).

### Итог 1.13.0

* `ps` — шапка `  PID TTY      STAT COMMAND` и строка самого процесса
  (`/mnt/paghbox ps`); второй процесс виден через
  `timeout 5 /mnt/paghbox ps` (parent timeout + child ps, валидатор требует обе
  команды); `ps --uptime` → `uptime: X.XXs` из `/proc/uptime`; `ps -x` —
  usage 2.
* `free` — `Mem:` 262144 kB (256 MiB VM), `used + free == total`,
  `available == free`, `shared`/`buff-cache` — 0, `Swap` — 0; `-m` даёт
  MemTotal 256; `-h` → `256.0M`; `free -z` — usage 2. Валидатор разбирает
  строку, а не сравнивает её с константой: цифры `used`/`free` живые.
* `df` — `procfs` с нулевой ёмкостью на `/proc` и `ext2` с `1K-blocks=65536`
  на `/mnt` (64 MiB образ), `-h` → `64.0M`; `df /proc` и `df /mnt` — те же
  строки по операнду; `df /` → `df: /: Operation not supported` (1),
  `df /nope` → `No such file or directory` (1), `df -x` — usage 2.
* Регрессия 1.8.0/1.10.0/1.11.0/1.12.0 на новом ядре — без изменений.
* **Honest refusal на старом ядре 0.21.1:** `ps` → `ps: /proc: No such file or
  directory (the kernel needs the procfs ABI)` (1), `free` → `free:
  /proc/meminfo: No such file or directory` (1), `df /mnt` → `df: /mnt:
  Function not implemented` (1). Ни одна из трёх команд не печатает
  выдуманную таблицу.

### Чем проверка ограничена (1.13.0)

* Тестовое ядро — **ветка**, а не main: PR #9/#10/#11 открыты; транскрипт
  фиксирует путь `/tmp/pagh-night2/riscv-feat-src` и head ветки. `libpagh`
  0.6.1 (`pagh-api` main `b203ea6`) ещё не описывает `statfs`/procfs —
  `ps`/`free` читают `/proc` обычным `openat`/`read`, `df` делает сырой
  `ecall` в `statfs_sys.rs` с задокументированным 96-байтным контрактом.
* `ps`: TTY всегда `?` — tty/controlling terminal/process group в ABI нет;
  `TIME`/`START` нет — `/proc/<pid>/stat` не содержит времени старта; kernel
  threads (`/proc/kthreads/<tid>`) — не процессы и в таблицу не входят.
* `free`: `shared`/`buff/cache` — 0, потому что таких счётчиков (и page
  cache) в ядре нет; `MemAvailable == MemFree` — так рендерит ядро; `Swap` —
  0, swap-подсистемы нет. Это не заглушки, а значение «счётчика нет».
* `df`: имён устройств в ABI нет (в `/proc/mounts` поле device — mount point),
  поэтому `Filesystem` — тип ФС; `Use%` округляется вверх; у `procfs` ёмкость
  ноль, `Use%` — `-`.
* Старый kernel-кейс требует наличия release-бинаря 0.21.1 в
  `/home/oleg/pagh-riscv/target/...`; если его нет, драйвер пропускает
  загрузку (`--no-legacy`).
* TLS-кейсы 1.10.0 не повторяются (как и в 1.11.0/1.12.0): ABI не менялся,
  полный TLS-прогон требует host-интернета.

### PASS-строки 1.13.0

```text
PASS sha3 empty: run /mnt/paghbox sha3sum /mnt/empty.txt
PASS sha3 abc: run /mnt/paghbox sha3sum /mnt/abc.txt
PASS sha3 fox: run /mnt/paghbox sha3sum /mnt/fox.txt
PASS sha3 -b: run /mnt/paghbox sha3sum -b /mnt/abc.txt
PASS sha3 check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3
PASS sha3 bad check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3.bad
PASS sha3 status ok: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3
PASS sha3 status bad: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3.bad
PASS sha3 bad option: run /mnt/paghbox sha3sum -z
PASS calc precedence: run /mnt/paghbox calc 2+3*4
PASS calc parentheses: run /mnt/paghbox calc (2+3)*4
PASS calc unary minus: run /mnt/paghbox calc -5+2
PASS calc divide by zero: run /mnt/paghbox calc 1/0
PASS calc syntax: run /mnt/paghbox calc 2+
PASS calc modulo zero: run /mnt/paghbox calc 7%0
PASS calc two expressions: run /mnt/paghbox calc 1+1 2*3
PASS calc eval: run /mnt/paghbox calc -e 10%3
PASS base85 hello: run /mnt/paghbox base85 /mnt/hello.txt
PASS base85 zero group: run /mnt/paghbox base85 /mnt/zeros.bin
PASS base85 adobe: run /mnt/paghbox base85 -a /mnt/abc.txt
PASS base85 decode hello: run /mnt/paghbox base85 -d /mnt/b85hello.txt
PASS base85 decode adobe: run /mnt/paghbox base85 -d /mnt/b85adobe.txt
PASS base85 invalid: run /mnt/paghbox base85 -d /mnt/b85bad.txt
PASS readelf header: run /mnt/paghbox readelf /mnt/symbols.elf
PASS readelf not elf: run /mnt/paghbox readelf -h /mnt/notelf.txt
PASS readelf no operand: run /mnt/paghbox readelf
PASS readelf sections: run /mnt/paghbox readelf -S /mnt/symbols.elf
PASS readelf symbols: run /mnt/paghbox readelf -s /mnt/symbols.elf
PASS readelf program headers: run /mnt/paghbox readelf -l /mnt/symbols.elf
PASS readelf bad section file: run /mnt/paghbox readelf -S /mnt/notelf.txt
PASS readelf bad option: run /mnt/paghbox readelf -z /mnt/symbols.elf
PASS uuidgen default: run /mnt/paghbox uuidgen
PASS uuidgen count: run /mnt/paghbox uuidgen -n 3
PASS uuidgen bad count: run /mnt/paghbox uuidgen -n x
PASS paghfetch: run /mnt/paghbox paghfetch
PASS paghfetch usage: run /mnt/paghbox paghfetch x
PASS threads counter: run /mnt/paghbox threads
PASS threads usage: run /mnt/paghbox threads -x
PASS uuidgen entropy: run /mnt/paghbox uuidgen -n 4
PASS shuf entropy: run /mnt/paghbox shuf -e a b c d e
PASS kill -0 group: run /mnt/paghbox kill -0 0
PASS kill no such pid: run /mnt/paghbox kill 99999
PASS kill bad pid: run /mnt/paghbox kill abc
PASS dns literal: run /mnt/paghbox dns 10.0.2.2
PASS dns usage: run /mnt/paghbox dns -z
PASS dns missing host: run /mnt/paghbox dns
PASS curl http: run /mnt/paghbox curl http://10.0.2.2:55979/h
PASS curl http -i: run /mnt/paghbox curl -i http://10.0.2.2:55979/h
PASS wget http: run /mnt/paghbox wget -O h.out http://10.0.2.2:55979/h
PASS cat wget output: run /mnt/paghbox cat h.out
PASS curl chunked: run /mnt/paghbox curl http://10.0.2.2:55979/chunked
PASS curl chunked -i: run /mnt/paghbox curl -i http://10.0.2.2:55979/chunked
PASS dd copy: run /mnt/paghbox dd if=/mnt/abc.txt of=/mnt/ddcopy.txt
PASS dd copy content: run /mnt/paghbox cat /mnt/ddcopy.txt
PASS dd bs round trip: run /mnt/paghbox dd if=/mnt/fox.txt of=/mnt/fox.copy bs=8
PASS dd bs content: run /mnt/paghbox cmp /mnt/fox.txt /mnt/fox.copy
PASS dd notrunc existing: run /mnt/paghbox dd if=/mnt/abc.txt of=/mnt/fox.copy bs=4 conv=notrunc status=none
PASS dd notrunc leaves tail: run /mnt/paghbox cmp /mnt/fox.txt /mnt/fox.copy
PASS dd count block: run /mnt/paghbox dd if=/mnt/abc.txt bs=2 count=1 status=none
PASS dd skip byte: run /mnt/paghbox dd if=/mnt/abc.txt bs=1 skip=1 status=none
PASS dd bad size: run /mnt/paghbox dd bs=abc
PASS dd missing input: run /mnt/paghbox dd if=/mnt/nope
PASS dd unsupported conv: run /mnt/paghbox dd conv=fsync
PASS nc no args: run /mnt/paghbox nc
PASS nc unsupported -u: run /mnt/paghbox nc -u 10.0.2.2 80
PASS nc client -p: run /mnt/paghbox nc -p 1234 10.0.2.2 80
PASS nc client: run /mnt/paghbox nc 10.0.2.2 42809
PASS nc client: run /mnt/paghbox nc 10.0.2.2 42809
PASS nc listener: run /mnt/paghbox nc -l -p 7001
PASS printenv all: run /mnt/paghbox printenv
PASS printenv one: run /mnt/paghbox printenv PWD
PASS printenv missing: run /mnt/paghbox printenv NOPE
PASS printenv found and missing: run /mnt/paghbox printenv PWD NOPE
PASS printenv --: run /mnt/paghbox printenv -- PWD
PASS printenv bad option: run /mnt/paghbox printenv -x
PASS dos2unix converts: run /mnt/paghbox dos2unix /mnt/crlf.txt
PASS dos2unix content: run /mnt/paghbox cat /mnt/crlf.txt
PASS dos2unix lone cr: run /mnt/paghbox dos2unix /mnt/lonecr.txt
PASS dos2unix lone cr content: run /mnt/paghbox cmp /mnt/lonecr.txt /mnt/lonecr.expected
PASS unix2dos converts: run /mnt/paghbox unix2dos /mnt/lf.txt
PASS unix2dos idempotent: run /mnt/paghbox unix2dos /mnt/lf.txt
PASS unix2dos content: run /mnt/paghbox cmp /mnt/lf.txt /mnt/lf.dos.expected
PASS iconv ascii utf8: run /mnt/paghbox iconv -f ascii -t utf8 /mnt/hello.txt
PASS iconv latin1 utf8 to file: run /mnt/paghbox iconv -f latin1 -t utf8 -o /mnt/u8out.txt /mnt/latin1.txt
PASS iconv latin1 utf8 to file: run /mnt/paghbox iconv -f latin1 -t utf8 -o /mnt/u8out.txt /mnt/latin1.txt
PASS iconv latin1 bytes: run /mnt/paghbox cmp /mnt/u8out.txt /mnt/utf8.txt
PASS iconv utf8 to latin1: run /mnt/paghbox iconv -f utf8 -t latin1 -o /mnt/lat2.txt /mnt/utf8.txt
PASS iconv latin1 bytes back: run /mnt/paghbox cmp /mnt/latin1.txt /mnt/lat2.txt
PASS iconv utf8 to utf16le: run /mnt/paghbox iconv -f utf8 -t utf16le -o /mnt/u8le.txt /mnt/utf8.txt
PASS iconv utf16le bytes: run /mnt/paghbox cmp /mnt/u8le.txt /mnt/utf16le.bin
PASS iconv utf16be to utf8: run /mnt/paghbox iconv -f utf16be -t utf8 -o /mnt/u8be.txt /mnt/utf16be.bin
PASS iconv utf16be bytes: run /mnt/paghbox cmp /mnt/u8be.txt /mnt/utf8.txt
PASS iconv utf16le to utf8: run /mnt/paghbox iconv -f utf16le -t utf8 -o /mnt/u8le2.txt /mnt/utf16le.bin
PASS iconv utf16le bytes back: run /mnt/paghbox cmp /mnt/u8le2.txt /mnt/utf8.txt
PASS iconv invalid utf8: run /mnt/paghbox iconv -f utf8 -t ascii /mnt/latin1.txt
PASS iconv unrepresentable: run /mnt/paghbox iconv -f utf8 -t ascii /mnt/utf8.txt
PASS iconv no options: run /mnt/paghbox iconv
PASS iconv bad charset: run /mnt/paghbox iconv -f klingon -t utf8 /mnt/utf8.txt
PASS iconv -l: run /mnt/paghbox iconv -l
PASS ps table: run /mnt/paghbox ps
PASS ps uptime: run /mnt/paghbox ps --uptime
PASS ps via timeout: run /mnt/paghbox timeout 5 /mnt/paghbox ps
PASS ps bad option: run /mnt/paghbox ps -x
PASS free default: run /mnt/paghbox free
PASS free -m: run /mnt/paghbox free -m
PASS free -h: run /mnt/paghbox free -h
PASS free bad unit: run /mnt/paghbox free -z
PASS df all: run /mnt/paghbox df
PASS df -h: run /mnt/paghbox df -h
PASS df /proc: run /mnt/paghbox df /proc
PASS df /mnt: run /mnt/paghbox df /mnt
PASS df synthetic root: run /mnt/paghbox df /
PASS df missing path: run /mnt/paghbox df /nope
PASS df bad option: run /mnt/paghbox df -x
PASS ps without procfs: run /mnt/paghbox ps
PASS free without meminfo: run /mnt/paghbox free
PASS df without statfs: run /mnt/paghbox df /mnt
```

## 1.12.0 (текстовые applet'ы: dos2unix, unix2dos, iconv, printenv)

Дата: ночь 2026-11-12/13. Ядро:
`/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/release/pagh`
(release-сборка 0.21.1: ABI 0.21.x + `getrandom`/TLS). QEMU `qemu-system-riscv64`
10.2.2, машина `virt`, 256 MiB, serial на Unix-сокете, `-bios default`, modern
virtio, NIC в QEMU user network для сетевых кейсов (DHCP `10.0.2.15`, хост
`10.0.2.2`). Диск: 64 MiB ext2
(`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`); в корне — `paghbox`
1.12.0, фикстуры 1.8.0, `h` для host-сервера и конверсионные фикстуры
(`crlf.txt`, `latin1.txt`, `utf8.txt`, `utf16le.bin`, `utf16be.bin`, эталоны
для `cmp`).

Драйвер — [`tools/live_test_1120.py`](../tools/live_test_1120.py): **42
загрузки по 1–3 команды** (вход одной загрузки ≤ 150 байт — предел буфера
консоли, драйвер это проверяет), после каждой сверяются вывод,
`run: /mnt/paghbox exited with code N` и, где данные не фиксированы, регулярные
выражения. Состав: регрессия 1.8.0 (13 загрузок), сетевые кейсы 1.10.0
(7 загрузок), `dd`/`nc` 1.11.0 (8 загрузок), новые 1.12.0 (14 загрузок:
`printenv` ×2, `dos2unix` ×2, `unix2dos`, `iconv` ×8, `iconv -l`). Итог:
**99 PASS, 0 FAIL**. 1.12.0 не менял ABI; пакет собран против `libpagh` 0.6.1
(`pagh-api` main `b203ea6`).

Сырые транскрипты: [`live-1.12.0-transcript.txt`](live-1.12.0-transcript.txt)
и [`live-1.12.0-pkg-install.txt`](live-1.12.0-pkg-install.txt)
(`pkg install paghbox 1.12.0`: 128 симлинков + `paghbox`, `debugfs` сверяет
inode; boot без NIC запускает `/mnt/bin/iconv`, `/mnt/bin/printenv`,
`/mnt/bin/dd`, `/mnt/bin/nc`).

### Итог 1.12.0

* `dos2unix`/`unix2dos` — `crlf.txt` (17 B) после `dos2unix` → 14 B, `cat`
  3 строки; одинокий `\r` сохранён (сверка `cmp` с host-эталоном);
  `unix2dos` идемпотентен (два прогона; `cmp` с CRLF-эталоном → 0).
* `iconv` — `-f latin1 -t utf8 -o` → байт-в-байт равен host-фикстуре UTF-8;
  обратно UTF-8→latin1 `cmp` → 0; UTF-8↔UTF-16LE/BE round-trip через `cmp`;
  `-f ascii -t utf8` → `hello`; fail-closed: `latin1.txt` как UTF-8 → exit 1
  (`invalid input for UTF-8 at byte 3`), `utf8.txt` → ASCII → exit 1
  (`cannot convert U+00E9 to ASCII`); неизвестный charset → 2; `-l` — ровно
  5 строк.
* `printenv` — `printenv` → `PWD=/mnt`; `printenv PWD` → `/mnt`;
  `printenv NOPE` → exit 1 без вывода; `PWD NOPE` → `/mnt` + exit 1;
  `-- PWD` → `/mnt`; `-x` → 2.
* Дубликатов нет: `head`/`tail`/`wc`/`grep` уже входят в paghbox с 1.8.0
  (и проверяются регрессией), 1.12.0 добавляет четыре **других** applet'а;
  это сверено по `NAMES` и `commands=` пакета.
* Регрессия: 1.8.0 (sha3/calc/base85/readelf/uuidgen/paghfetch), сетевые
  1.10.0 (`threads`/`uuidgen`/`shuf`/`kill`/`dns`-литерал/`curl`/`wget` http),
  `dd`/`nc` 1.11.0 — без изменений.

### Чем проверка ограничена (1.12.0)

* Конверсии проверяются **байт-в-байт** через `cmp` с host-фикстурами, а не
  печатью в консоль: драйвер декодирует serial-лог по чанкам, поэтому
  многобайтный UTF-8-символ на границе двух чтений может стать U+FFFD в логе,
  хотя гость записал верные байты. Видимый вывод `iconv` в кейсах — только
  ASCII (`hello.txt` → `hello`). Это свойство драйвера/лога, не applet'а.
* `iconv`: только пять кодировок (UTF-8, ASCII, ISO-8859-1, UTF-16LE,
  UTF-16BE); нет UTF-16 с BOM-sniffing, UTF-32, MIME/code-page алиасов.
  Локаль в ABI отсутствует, поэтому `-f`/`-t` обязательны. Один входной файл.
* `dos2unix`/`unix2dos`: in-place конверсия буферизует файл целиком (потоковая
  in-place невыразима: чтение и запись одного файла одновременно усекли бы
  данные); busybox-овский общий `-u` не реализован — это две команды.
  stdin-режим ждёт вечно: у консольного fd 0 нет EOF.
* `printenv`: окружение — ровно `envp` процесса (на этом ядре `PWD`), без
  `/etc/environment` и login-шелла; поэтому listing из одной строки — это факт,
  а не усечение.
* TLS-кейсы 1.10.0 не повторяются: 1.12.0 не меняет ABI/TLS, полный TLS-прогон
  требует host-интернета (`live_test_1100.py --tls`).

### PASS-строки 1.12.0

```text
PASS sha3 empty: run /mnt/paghbox sha3sum /mnt/empty.txt
PASS sha3 abc: run /mnt/paghbox sha3sum /mnt/abc.txt
PASS sha3 fox: run /mnt/paghbox sha3sum /mnt/fox.txt
PASS sha3 -b: run /mnt/paghbox sha3sum -b /mnt/abc.txt
PASS sha3 check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3
PASS sha3 bad check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3.bad
PASS sha3 status ok: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3
PASS sha3 status bad: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3.bad
PASS sha3 bad option: run /mnt/paghbox sha3sum -z
PASS calc precedence: run /mnt/paghbox calc 2+3*4
PASS calc parentheses: run /mnt/paghbox calc (2+3)*4
PASS calc unary minus: run /mnt/paghbox calc -5+2
PASS calc divide by zero: run /mnt/paghbox calc 1/0
PASS calc syntax: run /mnt/paghbox calc 2+
PASS calc modulo zero: run /mnt/paghbox calc 7%0
PASS calc two expressions: run /mnt/paghbox calc 1+1 2*3
PASS calc eval: run /mnt/paghbox calc -e 10%3
PASS base85 hello: run /mnt/paghbox base85 /mnt/hello.txt
PASS base85 zero group: run /mnt/paghbox base85 /mnt/zeros.bin
PASS base85 adobe: run /mnt/paghbox base85 -a /mnt/abc.txt
PASS base85 decode hello: run /mnt/paghbox base85 -d /mnt/b85hello.txt
PASS base85 decode adobe: run /mnt/paghbox base85 -d /mnt/b85adobe.txt
PASS base85 invalid: run /mnt/paghbox base85 -d /mnt/b85bad.txt
PASS readelf header: run /mnt/paghbox readelf /mnt/symbols.elf
PASS readelf not elf: run /mnt/paghbox readelf -h /mnt/notelf.txt
PASS readelf no operand: run /mnt/paghbox readelf
PASS readelf sections: run /mnt/paghbox readelf -S /mnt/symbols.elf
PASS readelf symbols: run /mnt/paghbox readelf -s /mnt/symbols.elf
PASS readelf program headers: run /mnt/paghbox readelf -l /mnt/symbols.elf
PASS readelf bad section file: run /mnt/paghbox readelf -S /mnt/notelf.txt
PASS readelf bad option: run /mnt/paghbox readelf -z /mnt/symbols.elf
PASS uuidgen default: run /mnt/paghbox uuidgen
PASS uuidgen count: run /mnt/paghbox uuidgen -n 3
PASS uuidgen bad count: run /mnt/paghbox uuidgen -n x
PASS paghfetch: run /mnt/paghbox paghfetch
PASS paghfetch usage: run /mnt/paghbox paghfetch x
PASS threads counter: run /mnt/paghbox threads
PASS threads usage: run /mnt/paghbox threads -x
PASS uuidgen entropy: run /mnt/paghbox uuidgen -n 4
PASS shuf entropy: run /mnt/paghbox shuf -e a b c d e
PASS kill -0 group: run /mnt/paghbox kill -0 0
PASS kill no such pid: run /mnt/paghbox kill 99999
PASS kill bad pid: run /mnt/paghbox kill abc
PASS dns literal: run /mnt/paghbox dns 10.0.2.2
PASS dns usage: run /mnt/paghbox dns -z
PASS dns missing host: run /mnt/paghbox dns
PASS curl http: run /mnt/paghbox curl http://10.0.2.2:60349/h
PASS curl http -i: run /mnt/paghbox curl -i http://10.0.2.2:60349/h
PASS wget http: run /mnt/paghbox wget -O h.out http://10.0.2.2:60349/h
PASS cat wget output: run /mnt/paghbox cat h.out
PASS curl chunked: run /mnt/paghbox curl http://10.0.2.2:60349/chunked
PASS curl chunked -i: run /mnt/paghbox curl -i http://10.0.2.2:60349/chunked
PASS dd copy: run /mnt/paghbox dd if=/mnt/abc.txt of=/mnt/ddcopy.txt
PASS dd copy content: run /mnt/paghbox cat /mnt/ddcopy.txt
PASS dd bs round trip: run /mnt/paghbox dd if=/mnt/fox.txt of=/mnt/fox.copy bs=8
PASS dd bs content: run /mnt/paghbox cmp /mnt/fox.txt /mnt/fox.copy
PASS dd notrunc existing: run /mnt/paghbox dd if=/mnt/abc.txt of=/mnt/fox.copy bs=4 conv=notrunc status=none
PASS dd notrunc leaves tail: run /mnt/paghbox cmp /mnt/fox.txt /mnt/fox.copy
PASS dd count block: run /mnt/paghbox dd if=/mnt/abc.txt bs=2 count=1 status=none
PASS dd skip byte: run /mnt/paghbox dd if=/mnt/abc.txt bs=1 skip=1 status=none
PASS dd bad size: run /mnt/paghbox dd bs=abc
PASS dd missing input: run /mnt/paghbox dd if=/mnt/nope
PASS dd unsupported conv: run /mnt/paghbox dd conv=fsync
PASS nc no args: run /mnt/paghbox nc
PASS nc unsupported -u: run /mnt/paghbox nc -u 10.0.2.2 80
PASS nc client -p: run /mnt/paghbox nc -p 1234 10.0.2.2 80
PASS nc client: run /mnt/paghbox nc 10.0.2.2 54523
PASS nc client: run /mnt/paghbox nc 10.0.2.2 54523
PASS nc listener: run /mnt/paghbox nc -l -p 7001
PASS printenv all: run /mnt/paghbox printenv
PASS printenv one: run /mnt/paghbox printenv PWD
PASS printenv missing: run /mnt/paghbox printenv NOPE
PASS printenv found and missing: run /mnt/paghbox printenv PWD NOPE
PASS printenv --: run /mnt/paghbox printenv -- PWD
PASS printenv bad option: run /mnt/paghbox printenv -x
PASS dos2unix converts: run /mnt/paghbox dos2unix /mnt/crlf.txt
PASS dos2unix content: run /mnt/paghbox cat /mnt/crlf.txt
PASS dos2unix lone cr: run /mnt/paghbox dos2unix /mnt/lonecr.txt
PASS dos2unix lone cr content: run /mnt/paghbox cmp /mnt/lonecr.txt /mnt/lonecr.expected
PASS unix2dos converts: run /mnt/paghbox unix2dos /mnt/lf.txt
PASS unix2dos idempotent: run /mnt/paghbox unix2dos /mnt/lf.txt
PASS unix2dos content: run /mnt/paghbox cmp /mnt/lf.txt /mnt/lf.dos.expected
PASS iconv ascii utf8: run /mnt/paghbox iconv -f ascii -t utf8 /mnt/hello.txt
PASS iconv latin1 utf8 to file: run /mnt/paghbox iconv -f latin1 -t utf8 -o /mnt/u8out.txt /mnt/latin1.txt
PASS iconv latin1 utf8 to file: run /mnt/paghbox iconv -f latin1 -t utf8 -o /mnt/u8out.txt /mnt/latin1.txt
PASS iconv latin1 bytes: run /mnt/paghbox cmp /mnt/u8out.txt /mnt/utf8.txt
PASS iconv utf8 to latin1: run /mnt/paghbox iconv -f utf8 -t latin1 -o /mnt/lat2.txt /mnt/utf8.txt
PASS iconv latin1 bytes back: run /mnt/paghbox cmp /mnt/latin1.txt /mnt/lat2.txt
PASS iconv utf8 to utf16le: run /mnt/paghbox iconv -f utf8 -t utf16le -o /mnt/u8le.txt /mnt/utf8.txt
PASS iconv utf16le bytes: run /mnt/paghbox cmp /mnt/u8le.txt /mnt/utf16le.bin
PASS iconv utf16be to utf8: run /mnt/paghbox iconv -f utf16be -t utf8 -o /mnt/u8be.txt /mnt/utf16be.bin
PASS iconv utf16be bytes: run /mnt/paghbox cmp /mnt/u8be.txt /mnt/utf8.txt
PASS iconv utf16le to utf8: run /mnt/paghbox iconv -f utf16le -t utf8 -o /mnt/u8le2.txt /mnt/utf16le.bin
PASS iconv utf16le bytes back: run /mnt/paghbox cmp /mnt/u8le2.txt /mnt/utf8.txt
PASS iconv invalid utf8: run /mnt/paghbox iconv -f utf8 -t ascii /mnt/latin1.txt
PASS iconv unrepresentable: run /mnt/paghbox iconv -f utf8 -t ascii /mnt/utf8.txt
PASS iconv no options: run /mnt/paghbox iconv
PASS iconv bad charset: run /mnt/paghbox iconv -f klingon -t utf8 /mnt/utf8.txt
PASS iconv -l: run /mnt/paghbox iconv -l
```

## 1.11.0 (ABI 0.21.x: nc и dd)

Дата: ночь 2026-11-12/13. Ядро:
`/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/release/pagh`
(release-сборка 0.21.1: ABI 0.21.x + `getrandom`/TLS; `uname` version 0.21.1).
QEMU `qemu-system-riscv64` 10.2.2, машина `virt`, 256 MiB, serial на Unix-сокете,
`-bios default`, modern virtio (`-global virtio-mmio.force-legacy=false`), NIC в
QEMU user network для сетевых кейсов (DHCP `10.0.2.15`, хост `10.0.2.2`). Диск:
64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне — `paghbox`
1.11.0, фикстуры 1.8.0 и короткий `h` (`hello\n`) для host-сервера.

Драйвер — [`tools/live_test_1110.py`](../tools/live_test_1110.py): **28 загрузок
по 1–3 команды** (вход одной загрузки ≤ 150 байт — тот же предел буфера консоли,
что у 1.8.0/1.10.0, драйвер это проверяет), после каждой сверяются вывод,
`run: /mnt/paghbox exited with code N` и, где данные не фиксированы, регулярные
выражения. Первые 13 загрузок — регрессия 1.8.0, затем сетевые кейсы 1.10.0
(`threads`, `uuidgen`/`shuf` на `getrandom`, `kill`, `dns`-литерал, `curl`/`wget`
по `http://` против host-сервера), затем новые `dd` и `nc`. Итог:
**68 PASS, 0 FAIL**. 1.11.0 не менял ABI; пакет собран против `libpagh` 0.6.1
(`pagh-api` main `b203ea6`).

Сырой транскрипт: [`live-1.11.0-transcript.txt`](live-1.11.0-transcript.txt).
Отдельно: [`live-1.11.0-pkg-install.txt`](live-1.11.0-pkg-install.txt) —
`pkg install paghbox 1.11.0` на чистом 64 MiB ext2: 124 симлинка + `paghbox`
(125 имён), `debugfs` сверяет inode; boot без NIC запускает `/mnt/bin/dd`
(`dd-payload`) и `/mnt/bin/nc` (`usage: nc`, exit 2). Снимается
[`tools/live_pkg_install.py`](../tools/live_pkg_install.py) (host HTTP-сервер,
две загрузки).

### Итог 1.11.0

* `dd` — `if=abc.txt of=ddcopy.txt` → `0+1 records in` / `0+1 records out`
  (bs по умолчанию 512, файл 3 байта — один частичный блок), `cat` → `abc`;
  `bs=8` на `fox.txt` (43 байта) → `5+1 records in/out` и `cmp` → 0;
  `conv=notrunc` поверх `fox.copy` пишет 3 байта без усечения, второй `cmp` →
  `differ: byte 1` (хвост файла сохранён); `bs=2 count=1 status=none` → `ab`,
  `bs=1 skip=1 status=none` → `bc`; неверный `bs=abc` → exit 2, отсутствующий
  `if=` → exit 1, `conv=fsync` → exit 2 (в ABI нет `fsync`, поэтому conversion
  отклоняется с причиной, а не игнорируется).
* `nc` — без аргументов → usage (2); `-u` → 2 с явным «TCP client/listener
  only» (UDP `connect`/EOF в ABI нет); TCP-клиент `nc 10.0.2.2:PORT` против
  host-сервера получает `hello from host` и выходит 0 после FIN; listener
  `nc -l -p 7001` принимает соединение через QEMU `hostfwd`
  (`hostfwd=tcp:127.0.0.1:HPORT-:7001`), получает тот же payload и выходит 0.
* Регрессия: кейсы 1.8.0 (sha3/calc/base85/readelf/uuidgen/paghfetch) и сетевые
  1.10.0-кейсы проходят на 0.21.1 без изменений.
* `ps`/`free`/`pgrep`/`df` не добавлены и не выдаются за реализованные: в ABI
  0.21.x нет `/proc`, `/proc/meminfo` и `statfs` (см. README, «Ограничения
  среды и ABI»).

### Чем проверка ограничена (1.11.0)

* `nc` — только TCP: `-u`, `-e`, `-z`, `-i`, `-w`, `-s` отклоняются как
  неподдерживаемые. Один коннект и один listener; `shutdown` в ABI закрывает оба
  направления, поэтому EOF на stdin не делает half-close — сокет остаётся
  открытым на чтение до FIN от peer'а.
* `nc`-клиент в живом кейсе идёт на IPv4-литерал; резолв имени через
  `libpagh::dns` проверен в 1.10.0 на `curl`/`wget` (тот же резолвер), у `nc`
  отдельного DNS-кейса нет.
* host-сервер в кейсе `nc` отправляет payload через 0.75 с после accept: peer,
  который принимает и закрывается в тот же миг, превращает `connect()` в
  `ETIMEDOUT` — это воспроизводится и на `curl`, то есть ограничение
  kernel TCP-клиента под QEMU user-net, а не applet'а.
* `dd` без `if=` читает stdin, у которого нет EOF; живой кейс всегда с `if=`.
  `seek` живым кейсом не покрыт (только код-путь): на ext2 запись в offset
  проверена ранее `truncate`/`cp`, отдельного кейса у `dd seek` нет.
* TLS-кейсы 1.10.0 здесь не повторяются: 1.11.0 не меняет ABI/TLS, а полный
  TLS-прогон требует host-интернета (он в `live_test_1100.py --tls`).

### PASS-строки 1.11.0

```text
PASS sha3 empty: run /mnt/paghbox sha3sum /mnt/empty.txt
PASS sha3 abc: run /mnt/paghbox sha3sum /mnt/abc.txt
PASS sha3 fox: run /mnt/paghbox sha3sum /mnt/fox.txt
PASS sha3 -b: run /mnt/paghbox sha3sum -b /mnt/abc.txt
PASS sha3 check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3
PASS sha3 bad check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3.bad
PASS sha3 status ok: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3
PASS sha3 status bad: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3.bad
PASS sha3 bad option: run /mnt/paghbox sha3sum -z
PASS calc precedence: run /mnt/paghbox calc 2+3*4
PASS calc parentheses: run /mnt/paghbox calc (2+3)*4
PASS calc unary minus: run /mnt/paghbox calc -5+2
PASS calc divide by zero: run /mnt/paghbox calc 1/0
PASS calc syntax: run /mnt/paghbox calc 2+
PASS calc modulo zero: run /mnt/paghbox calc 7%0
PASS calc two expressions: run /mnt/paghbox calc 1+1 2*3
PASS calc eval: run /mnt/paghbox calc -e 10%3
PASS base85 hello: run /mnt/paghbox base85 /mnt/hello.txt
PASS base85 zero group: run /mnt/paghbox base85 /mnt/zeros.bin
PASS base85 adobe: run /mnt/paghbox base85 -a /mnt/abc.txt
PASS base85 decode hello: run /mnt/paghbox base85 -d /mnt/b85hello.txt
PASS base85 decode adobe: run /mnt/paghbox base85 -d /mnt/b85adobe.txt
PASS base85 invalid: run /mnt/paghbox base85 -d /mnt/b85bad.txt
PASS readelf header: run /mnt/paghbox readelf /mnt/symbols.elf
PASS readelf not elf: run /mnt/paghbox readelf -h /mnt/notelf.txt
PASS readelf no operand: run /mnt/paghbox readelf
PASS readelf sections: run /mnt/paghbox readelf -S /mnt/symbols.elf
PASS readelf symbols: run /mnt/paghbox readelf -s /mnt/symbols.elf
PASS readelf program headers: run /mnt/paghbox readelf -l /mnt/symbols.elf
PASS readelf bad section file: run /mnt/paghbox readelf -S /mnt/notelf.txt
PASS readelf bad option: run /mnt/paghbox readelf -z /mnt/symbols.elf
PASS uuidgen default: run /mnt/paghbox uuidgen
PASS uuidgen count: run /mnt/paghbox uuidgen -n 3
PASS uuidgen bad count: run /mnt/paghbox uuidgen -n x
PASS paghfetch: run /mnt/paghbox paghfetch
PASS paghfetch usage: run /mnt/paghbox paghfetch x
PASS threads counter: run /mnt/paghbox threads
PASS threads usage: run /mnt/paghbox threads -x
PASS uuidgen entropy: run /mnt/paghbox uuidgen -n 4
PASS shuf entropy: run /mnt/paghbox shuf -e a b c d e
PASS kill -0 group: run /mnt/paghbox kill -0 0
PASS kill no such pid: run /mnt/paghbox kill 99999
PASS kill bad pid: run /mnt/paghbox kill abc
PASS dns literal: run /mnt/paghbox dns 10.0.2.2
PASS dns usage: run /mnt/paghbox dns -z
PASS dns missing host: run /mnt/paghbox dns
PASS curl http: run /mnt/paghbox curl http://10.0.2.2:46557/h
PASS curl http -i: run /mnt/paghbox curl -i http://10.0.2.2:46557/h
PASS wget http: run /mnt/paghbox wget -O h.out http://10.0.2.2:46557/h
PASS cat wget output: run /mnt/paghbox cat h.out
PASS curl chunked: run /mnt/paghbox curl http://10.0.2.2:46557/chunked
PASS curl chunked -i: run /mnt/paghbox curl -i http://10.0.2.2:46557/chunked
PASS dd copy: run /mnt/paghbox dd if=/mnt/abc.txt of=/mnt/ddcopy.txt
PASS dd copy content: run /mnt/paghbox cat /mnt/ddcopy.txt
PASS dd bs round trip: run /mnt/paghbox dd if=/mnt/fox.txt of=/mnt/fox.copy bs=8
PASS dd bs content: run /mnt/paghbox cmp /mnt/fox.txt /mnt/fox.copy
PASS dd notrunc existing: run /mnt/paghbox dd if=/mnt/abc.txt of=/mnt/fox.copy bs=4 conv=notrunc status=none
PASS dd notrunc leaves tail: run /mnt/paghbox cmp /mnt/fox.txt /mnt/fox.copy
PASS dd count block: run /mnt/paghbox dd if=/mnt/abc.txt bs=2 count=1 status=none
PASS dd skip byte: run /mnt/paghbox dd if=/mnt/abc.txt bs=1 skip=1 status=none
PASS dd bad size: run /mnt/paghbox dd bs=abc
PASS dd missing input: run /mnt/paghbox dd if=/mnt/nope
PASS dd unsupported conv: run /mnt/paghbox dd conv=fsync
PASS nc no args: run /mnt/paghbox nc
PASS nc unsupported -u: run /mnt/paghbox nc -u 10.0.2.2 80
PASS nc client -p: run /mnt/paghbox nc -p 1234 10.0.2.2 80
PASS nc client: run /mnt/paghbox nc 10.0.2.2 34777
PASS nc listener: run /mnt/paghbox nc -l -p 7001
```

## 1.10.0 (ABI 0.21.0: getrandom, DNS и TLS)

Дата: 2026-11-12. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/release/pagh`
(main 0.21.0; `uname` печатает `pagh pagh-riscv 0.10.0 0.21.0 riscv64`).
QEMU `qemu-system-riscv64` 10.2.2, машина `virt`, 256 MiB, serial на
Unix-сокете, `-bios default`, modern virtio
(`-global virtio-mmio.force-legacy=false`), NIC в QEMU user network для сетевых
кейсов (DHCP `10.0.2.15`, хост `10.0.2.2`, DNS `10.0.2.3`). Диск: 64 MiB ext2,
создан на хосте `mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне —
`paghbox` 1.10.0, фикстуры 1.8.0 и короткий `h` (`hello\n`) для host-сервера.

Драйвер — [`tools/live_test_1100.py`](../tools/live_test_1100.py): **27 загрузок
по 1–4 команды** (вход одной загрузки ≤ 150 байт — буфер консоли ядра не
принимает длинную очередь; драйвер это проверяет), после каждой сверяются
вывод, `run: /mnt/paghbox exited with code N` и, для случайных данных,
регулярное выражение/валидатор. Итог: **65 PASS, 0 FAIL**. Первые 13 загрузок —
регрессия кейсов 1.8.0 на ядре 0.21.0 (36 проверок); дальше — новые проверки:
`threads`, `uuidgen`/`shuf` на `getrandom`, `kill` (ESRCH через `libpagh`
0.6.1), `dns`, `curl`/`wget` по `http://` против host-сервера (включая
chunked-ответ), `https://` на `example.com` и fail-closed-ветки.

> **Release-ядро, не debug.** Debug-сборка ядра 0.21.0 переполняет 16 KiB
> kernel-стек на userland-сессии TLS (`tls_connect` → `user_open`): guard canary
> рапортует `[task] STACK OVERFLOW`, затем user-задача падает по instruction
> page fault. Release-ядро рукопожатие проводит; прогон поэтому на release (как и
> `docs/live-tls.txt` самого ядра). Это ограничение debug-сборки, не `paghbox`.

Сырой транскрипт: [`live-1.10.0-transcript.txt`](live-1.10.0-transcript.txt).
Отдельно: [`live-1.10.0-pkg-install.txt`](live-1.10.0-pkg-install.txt) —
`pkg install paghbox 1.10.0` (122 симлинка + `debugfs`), снимается
[`tools/live_pkg_install.py`](../tools/live_pkg_install.py) (host HTTP-сервер,
две загрузки; вторая — без NIC).

### Итог 1.10.0

* `threads` (демо 1.9.0, впервые в транскрипте) — 4 потока × 250 инкрементов под
  `libpagh::sync::Mutex`; `counter=1000`, exit 0; лишний аргумент — usage (2).
* `uuidgen` — 16 байт из пула энтропии ядра (`getrandom`): четыре UUID одного
  запуска различны (проверяет валидатор), формат версии 4 и варианта `[89ab]`
  сохранён; `-n 3`/`-n 4` — столько строк; `-n x` — 2; при отсутствии seed'а
  applet выходит 1, а не подменяет источник.
* `dns` — `example.com -> 8.…` через resolver QEMU `10.0.2.3` и через
  `-s 10.0.2.3`; IPv4-литерал отвечает без сети (`10.0.2.2 -> 10.0.2.2`);
  NXDOMAIN — `dns: example.invalid: Protocol error` + exit 1; пустой хост/
  неизвестная опция — exit 2.
* `curl`/`wget` `http://` — host-сервер `10.0.2.2:PORT`: тело `hello`; `-i` —
  `HTTP/1.0 200 OK` + `Content-Length: 6`; `wget -O h.out` —
  `wget: saved h.out (6 bytes)`, `cat h.out` → `hello`.
* `curl`/`wget` `https://example.com/` — kernel TLS 1.3 (в логе ядра
  `verified leaf=… chain=4 certs anchors=6`, `CertificateVerify OK`,
  `close_notify`): `HTTP/1.1 200 OK`, `Content-Type: text/html`, тело
  `Example Domain`; проверены `-i`, `-o out.html`, `wget -O w.html`
  (`wget: saved w.html (577 bytes)`) и `wget -O -`.
* Fail-closed: `curl https://example.invalid/` — `curl: resolve: …` (1);
  `curl ftp://example.com/` — `unsupported URL` (2); локальный
  `openssl s_server` с self-signed сертификатом на `10.0.2.2:PORT` —
  `curl: tls: Certificate rejected by the trust decision` (1), в логе ядра
  `verify FAILED cause=chain (NoAnchor)`. Отключить проверку нельзя: флага нет
  ни в applet'е, ни в ABI.
* `curl`/`wget` framing — host-сервер отдаёт `Transfer-Encoding: chunked` в
  ответ на HTTP/1.0-запрос (peer вправе проигнорировать версию): тело из двух
  чанков (3+3 байта) склеено в `hello`, `-i` показывает `Transfer-Encoding:
  chunked`; `Content-Length`-путь проверен на `/h` (`Content-Length: 6`).
* `shuf -e a b c d e` — те же пять строк в другом порядке: seed из
  `getrandom`, а не фиксированный.
* `kill` — `kill -0 0` (проба группы) даёт 0 без вывода; `kill 99999` →
  `kill: 99999: No such process` (exit 1) — это `Errno::ESRCH`, который
  `libpagh` 0.6.1 (`pagh-api` PR #1) отображает своим текстом, а не
  `Unknown`; `kill abc` → `invalid pid` (2). Пакет собран против `libpagh`
  0.6.1 (`pagh-api` PR #1, ветка `night/20261112-abi-errnos` @ `472874e`);
  сборка на 0.6.0 (`ed37c1d`) тоже проверена — обе версии `Errno`
  компилируются, applet больше не подменяет `Errno::Unknown`.

### Чем проверка ограничена (1.10.0)

* DNS — один `A`-запрос, без DNSSEC/CNAME/AAAA; адрес зависит от сети хоста.
  TLS-кейсы требуют host-интернета, HTTP-кейсы — локального сервера; локальные
  кейсы идут без сети.
* TLS-гейт проверен applet-уровнем только на отказе цепочки (self-signed) и на
  успешном `example.com`; SAN/expiry/clock-gate по отдельности покрыты
  `tools/test_tls.py` ядра и `docs/live-tls.txt`.
* `curl`/`wget` не умеют редиректы, keep-alive и сжатие; тело — до 8 MiB;
  `wget -O -` пишет тело в stdout, summary — в stderr.
* Debug-ядро на userland-TLS падает (см. врезку выше); прогон — на release.
* У 1.9.0 не было отдельного драйвера/транскрипта; `threads` впервые проверен
  здесь, регрессия 1.8.0 перезапущена на 0.21.0.

### PASS-строки 1.10.0

```text
PASS sha3 empty: run /mnt/paghbox sha3sum /mnt/empty.txt
PASS sha3 abc: run /mnt/paghbox sha3sum /mnt/abc.txt
PASS sha3 fox: run /mnt/paghbox sha3sum /mnt/fox.txt
PASS sha3 -b: run /mnt/paghbox sha3sum -b /mnt/abc.txt
PASS sha3 check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3
PASS sha3 bad check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3.bad
PASS sha3 status ok: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3
PASS sha3 status bad: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3.bad
PASS sha3 bad option: run /mnt/paghbox sha3sum -z
PASS calc precedence: run /mnt/paghbox calc 2+3*4
PASS calc parentheses: run /mnt/paghbox calc (2+3)*4
PASS calc unary minus: run /mnt/paghbox calc -5+2
PASS calc divide by zero: run /mnt/paghbox calc 1/0
PASS calc syntax: run /mnt/paghbox calc 2+
PASS calc modulo zero: run /mnt/paghbox calc 7%0
PASS calc two expressions: run /mnt/paghbox calc 1+1 2*3
PASS calc eval: run /mnt/paghbox calc -e 10%3
PASS base85 hello: run /mnt/paghbox base85 /mnt/hello.txt
PASS base85 zero group: run /mnt/paghbox base85 /mnt/zeros.bin
PASS base85 adobe: run /mnt/paghbox base85 -a /mnt/abc.txt
PASS base85 decode hello: run /mnt/paghbox base85 -d /mnt/b85hello.txt
PASS base85 decode adobe: run /mnt/paghbox base85 -d /mnt/b85adobe.txt
PASS base85 invalid: run /mnt/paghbox base85 -d /mnt/b85bad.txt
PASS readelf header: run /mnt/paghbox readelf /mnt/symbols.elf
PASS readelf not elf: run /mnt/paghbox readelf -h /mnt/notelf.txt
PASS readelf no operand: run /mnt/paghbox readelf
PASS readelf sections: run /mnt/paghbox readelf -S /mnt/symbols.elf
PASS readelf symbols: run /mnt/paghbox readelf -s /mnt/symbols.elf
PASS readelf program headers: run /mnt/paghbox readelf -l /mnt/symbols.elf
PASS readelf bad section file: run /mnt/paghbox readelf -S /mnt/notelf.txt
PASS readelf bad option: run /mnt/paghbox readelf -z /mnt/symbols.elf
PASS uuidgen default: run /mnt/paghbox uuidgen
PASS uuidgen count: run /mnt/paghbox uuidgen -n 3
PASS uuidgen bad count: run /mnt/paghbox uuidgen -n x
PASS paghfetch: run /mnt/paghbox paghfetch
PASS paghfetch usage: run /mnt/paghbox paghfetch x
PASS threads counter: run /mnt/paghbox threads
PASS threads usage: run /mnt/paghbox threads -x
PASS uuidgen entropy: run /mnt/paghbox uuidgen -n 4
PASS shuf entropy: run /mnt/paghbox shuf -e a b c d e
PASS kill -0 group: run /mnt/paghbox kill -0 0
PASS kill no such pid: run /mnt/paghbox kill 99999
PASS kill bad pid: run /mnt/paghbox kill abc
PASS dns literal: run /mnt/paghbox dns 10.0.2.2
PASS dns usage: run /mnt/paghbox dns -z
PASS dns missing host: run /mnt/paghbox dns
PASS curl http: run /mnt/paghbox curl http://10.0.2.2:48633/h
PASS curl http -i: run /mnt/paghbox curl -i http://10.0.2.2:48633/h
PASS wget http: run /mnt/paghbox wget -O h.out http://10.0.2.2:48633/h
PASS cat wget output: run /mnt/paghbox cat h.out
PASS curl chunked: run /mnt/paghbox curl http://10.0.2.2:48633/chunked
PASS curl chunked -i: run /mnt/paghbox curl -i http://10.0.2.2:48633/chunked
PASS dns example.com: run /mnt/paghbox dns example.com
PASS dns -s example.com: run /mnt/paghbox dns -s 10.0.2.3 example.com
PASS dns nxdomain: run /mnt/paghbox dns example.invalid
PASS curl https: run /mnt/paghbox curl https://example.com/
PASS curl https -i: run /mnt/paghbox curl -i https://example.com/
PASS curl https -o: run /mnt/paghbox curl -o out.html https://example.com/
PASS cat https output: run /mnt/paghbox cat out.html
PASS wget https: run /mnt/paghbox wget -O w.html https://example.com/
PASS wget https body: run /mnt/paghbox cat w.html
PASS wget https stdout: run /mnt/paghbox wget -O - https://example.com/
PASS curl nxdomain: run /mnt/paghbox curl https://example.invalid/
PASS curl bad scheme: run /mnt/paghbox curl ftp://example.com/
PASS curl bad tls: run /mnt/paghbox curl https://10.0.2.2:48639/
```

## 1.8.0 (батч 9)

Дата: 2026-11-06. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh`
(main 0.16.1; `run /mnt/paghbox uname -a` в QEMU печатает
`pagh pagh-riscv 0.10.0 0.16.1 riscv64`). QEMU `qemu-system-riscv64` 10.2.2,
машина `virt`, 256 MiB, serial на Unix-сокете, `-bios default`, modern virtio
(`-global virtio-mmio.force-legacy=false`). Диск: 64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне — `paghbox`
1.8.0 и фикстуры: `empty.txt`/`abc.txt`/`fox.txt`, `hello.txt`, `zeros.bin`,
`notelf.txt` (64 байта, без ELF-magic), синтетический `symbols.elf` с
`.symtab`/`.dynsym`/`.shstrtab`, base85-векторы `b85hello.txt`,
`b85adobe.txt`, `b85bad.txt`, check-файлы `sums.sha3` и `sums.sha3.bad`.

Драйвер — [`tools/live_test_180.py`](../tools/live_test_180.py): **13 загрузок
по 2–3 команды**, после каждой проверялись вывод и
`run: /mnt/paghbox exited with code N`. Итог: **36 PASS, 0 FAIL**; все 6 новых
applet'ов проверены функционально. SHA3-256 сверен с векторами FIPS 202
(включая `abc`), base85 — с `base64.a85encode` хоста в обе стороны, вывод
`readelf` — с синтетическим ELF, UUID — регулярным выражением
`8-4-4-4-12` с версией 4 и вариантом `[89ab]`.

Сырой транскрипт: [`live-1.8.0-transcript.txt`](live-1.8.0-transcript.txt).

### Итог 1.8.0

* `sha3sum` — SHA3-256 (Keccak-f[1600], rate 136, suffix `0x06`): пустой файл
  → `a7ff…434a`, `abc` → `3a98…1532`, `fox` → `6907…fc04`; `-b` даёт
  `hex *name`; `-c` — `OK`, неверная сумма — `FAILED` + exit 1, `--status`
  подавляет вывод; неизвестная опция — exit 2. Ядро хэша проверено отдельно
  на границах rate (135/136/137/272/273 байта) против Python `hashlib`.
* `calc` — `2+3*4` → `14`, `(2+3)*4` → `20`, `-5+2` → `-3`; два выражения
  дают две строки, `-e 10%3` → `1`; `1/0` и `7%0` — `division by zero`,
  exit 1; `2+` — `syntax error`, exit 2.
* `base85` — `hello\n` → `BOu!rD[H`, четыре нулевых байта → `z`,
  `-a abc` → `<~@:E^~>` (режим ASCII85/Adobe); decode обоих видов даёт
  `hello`, `<~v~>` → `invalid input`, exit 1.
* `readelf` — заголовок синтетического ELF64 RISC-V: `EXEC (2)`,
  `RISC-V (243)`, entry `0x0`, shoff 64, shnum 8; `-S` печатает 8 секций
  (`.text`/`.data`/`.dynsym`/`.dynstr`/`.symtab`/`.strtab`/`.shstrtab`);
  `-s` — те же три строки, что `nm` (`d counter`, `T main`, `U puts`);
  `-l` — `Program headers: none`; не-ELF — exit 1, неизвестная опция и
  отсутствие операнда — exit 2.
* `uuidgen` — один UUID, `-n 3` — три строки; все значения имеют версию 4
  и вариант RFC 4122; `-n x` — exit 2. Seed — `clock_gettime(1)` + pid,
  поэтому значения разные от загрузки к загрузке, но не CSPRNG.
* `paghfetch` — `sysname: pagh`, `nodename: pagh-riscv`, `release: 0.10.0`,
  `machine: riscv64`, `uptime: 0s`, `nproc: 1`, `pwd: /mnt`; рамка рисуется
  из реального `sysname`; лишний операнд — exit 2.

### PASS-строки 1.8.0

```text
PASS sha3 empty: run /mnt/paghbox sha3sum /mnt/empty.txt
PASS sha3 abc: run /mnt/paghbox sha3sum /mnt/abc.txt
PASS sha3 fox: run /mnt/paghbox sha3sum /mnt/fox.txt
PASS sha3 -b: run /mnt/paghbox sha3sum -b /mnt/abc.txt
PASS sha3 check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3
PASS sha3 bad check: run /mnt/paghbox sha3sum -c /mnt/sums.sha3.bad
PASS sha3 status ok: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3
PASS sha3 status bad: run /mnt/paghbox sha3sum --status -c /mnt/sums.sha3.bad
PASS sha3 bad option: run /mnt/paghbox sha3sum -z
PASS calc precedence: run /mnt/paghbox calc 2+3*4
PASS calc parentheses: run /mnt/paghbox calc (2+3)*4
PASS calc unary minus: run /mnt/paghbox calc -5+2
PASS calc divide by zero: run /mnt/paghbox calc 1/0
PASS calc syntax: run /mnt/paghbox calc 2+
PASS calc modulo zero: run /mnt/paghbox calc 7%0
PASS calc two expressions: run /mnt/paghbox calc 1+1 2*3
PASS calc eval: run /mnt/paghbox calc -e 10%3
PASS base85 hello: run /mnt/paghbox base85 /mnt/hello.txt
PASS base85 zero group: run /mnt/paghbox base85 /mnt/zeros.bin
PASS base85 adobe: run /mnt/paghbox base85 -a /mnt/abc.txt
PASS base85 decode hello: run /mnt/paghbox base85 -d /mnt/b85hello.txt
PASS base85 decode adobe: run /mnt/paghbox base85 -d /mnt/b85adobe.txt
PASS base85 invalid: run /mnt/paghbox base85 -d /mnt/b85bad.txt
PASS readelf header: run /mnt/paghbox readelf /mnt/symbols.elf
PASS readelf not elf: run /mnt/paghbox readelf -h /mnt/notelf.txt
PASS readelf no operand: run /mnt/paghbox readelf
PASS readelf sections: run /mnt/paghbox readelf -S /mnt/symbols.elf
PASS readelf symbols: run /mnt/paghbox readelf -s /mnt/symbols.elf
PASS readelf program headers: run /mnt/paghbox readelf -l /mnt/symbols.elf
PASS readelf bad section file: run /mnt/paghbox readelf -S /mnt/notelf.txt
PASS readelf bad option: run /mnt/paghbox readelf -z /mnt/symbols.elf
PASS uuidgen default: run /mnt/paghbox uuidgen
PASS uuidgen count: run /mnt/paghbox uuidgen -n 3
PASS uuidgen bad count: run /mnt/paghbox uuidgen -n x
PASS paghfetch: run /mnt/paghbox paghfetch
PASS paghfetch usage: run /mnt/paghbox paghfetch x
```

### Показательные выводы 1.8.0

```text
run /mnt/paghbox sha3sum /mnt/abc.txt
3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532  /mnt/abc.txt

run /mnt/paghbox calc (2+3)*4
20

run /mnt/paghbox base85 /mnt/hello.txt
BOu!rD[H

run /mnt/paghbox readelf /mnt/symbols.elf
ELF header:
  type:      EXEC (2)
  machine:   RISC-V (243)
  entry:     0x0000000000000000
  phoff:     0
  shoff:     64
  flags:     0x0
  ehsize:    64
  phentsize: 0
  phnum:     0
  shentsize: 64
  shnum:     8

run /mnt/paghbox uuidgen
38046507-5739-4eca-83a8-42356c0092cc

run /mnt/paghbox paghfetch
      +------+   paghfetch
      | pagh |   sysname:   pagh
      +------+   nodename:  pagh-riscv
                 release:   0.10.0
                 machine:   riscv64
                 uptime:    0s
                 nproc:     1
                 pwd:       /mnt
```

### Чем проверка 1.8.0 ограничена

* **Нет пайпов и редиректов.** Round-trip `base85` проверен не конвейером, а
  сверкой с `base64.a85encode` хоста в обе стороны: guest-энкодер должен
  выдать ровно host-строку, guest-декодер — вернуть исходные байты.
* **Короткие очереди ввода.** Serial-драйвер ядра принимает ограниченное
  число байт, пока гость печатает; три команды `readelf` с длинным выводом
  подряд приводили к таймауту `run.py` на `sendall`. Поэтому `readelf`
  разбит на три загрузки по 2–3 команды с коротким выводом (это ограничение
  драйвера/консоли, не applet'а).
* **`readelf`** — только ELF64 little-endian; `-S`/`-s`/`-l` требуют таблицу
  секций; DWARF не читается, `addr2line` в наборе нет. Вывод — компактное
  подмножество GNU, а не его формат.
* **`uuidgen`** — не источник энтропии: seed из `clock_gettime(1)` и pid,
  уникальность не гарантирована. Проверены только форма, версия/вариант и
  количество, а не непредсказуемость.
* **`sha3sum`** — только SHA3-256; SHA3-224/384/512 и SHAKE не реализованы.
* **`calc`** — только `i64`; шелл ядра не понимает кавычки, поэтому
  выражения передаются одним словом без пробелов.
* **`paghfetch`** показывает `release` из uname (0.10.0), а не версию ядра
  0.16.1; distro/CPU/RAM не выдумываются, `nproc` всегда 1.

## 1.7.0 (батч 8)

Дата: 2026-11-06. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh`
(main 0.16.1; `run /mnt/paghbox uname -a` в QEMU печатает
`pagh pagh-riscv 0.10.0 0.16.1 riscv64`). QEMU `qemu-system-riscv64` 10.2.2,
машина `virt`, 256 MiB, serial на Unix-сокете, `-bios default`, modern virtio
(`-global virtio-mmio.force-legacy=false`). Диск: 64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне — `paghbox`
1.7.0, `/mnt/bin`-симлинки на `echo`/`sleep`/`true`/`false`/`cat` и фикстуры:
`empty.txt`/`abc.txt`/`fox.txt`/`numbers.txt`, `hello.txt`, hex-файлы
(`hello.hex`, `hexspaced.txt`, `badhex*.txt`), base58-векторы
(`b58hello.txt`, `b58abc.txt`, `b58bad.txt`, `zeros.bin`), check-файлы
`sums.sha224*`/`sums.sha384*`/`crc32.*` и gzip-потоки: `stored.gz` и
`stored2.gz` (host `gzip.compress(compresslevel=0)`, DEFLATE stored blocks),
`compressed.gz` (level 9) и `bad.gz`.

Драйвер — [`tools/live_test_170.py`](../tools/live_test_170.py): **14 загрузок
по 3 команды**, после каждой проверялись вывод и
`run: /mnt/paghbox exited with code N`. Итог: **42 PASS, 0 FAIL**; все 7 новых
applet'ов проверены функционально, коды 0/1/2 и точные хэш-векторы сверены с
эталонами Python `hashlib`/`zlib`.

Сырой транскрипт: [`live-1.7.0-transcript.txt`](live-1.7.0-transcript.txt).

### Итог 1.7.0

* `sha224sum` — векторы FIPS 180-4 для `empty`/`abc`/`fox` совпали
  (`d14a…`, `2309…`, `730e…`); `-b` даёт `hex *name`, `-c` — `OK`, неверная
  сумма — `FAILED` + exit 1, `--status` подавляет вывод, неизвестная опция —
  exit 2. SHA-224 переиспользует SHA-256 с другим IV и усечением до 28 байт.
* `sha384sum` — те же три вектора (`38b0…`, `cb00…`, `ca73…`), тот же
  интерфейс `-b`/`-c`/`--status`; SHA-384 — SHA-512 с другим IV и усечением
  до 48 байт.
* `crc32` — `123456789` → `cbf43926 9 /mnt/numbers.txt`, `abc` →
  `352441c2 3`, пустой файл → `00000000 0`; `-c` по формату
  `crc size name` даёт `OK`, неверное значение — `FAILED` + exit 1,
  кривая строка — предупреждение и exit 1.
* `base58` — `hello\n` → `tzCkV5DK`, `00 01` → `12` (ведущий нулевой байт
  как `1`); decode возвращает `hello`/`abc`; `0OIl` → `invalid input` и
  exit 1; лишний операнд — exit 2.
* `hex2bin`/`bin2hex` — `61 62\n63\t0a` → `decoded.bin` → `6162630a`
  (round-trip через файл); lowercase `68656c6c6f0a`, `-u` →
  `68656C6C6F0A`, `-w 4` → `6865`/`6c6c`/`6f0a`; неверный hex и плохая
  ширина — exit 1/2.
* `zcat` — host stored-block `stored.gz` → `zcat stored payload`, два
  concatenated member'а → обе строки; `compressed.gz` (BTYPE 1/2) →
  `compressed DEFLATE blocks are not supported` exit 1, `bad.gz` → `bad gzip
  magic` exit 1, отсутствующий файл → exit 1, `-c` → usage exit 2.

### PASS-строки 1.7.0

```text
PASS sha224 empty: run /mnt/paghbox sha224sum /mnt/empty.txt
PASS sha224 abc: run /mnt/paghbox sha224sum /mnt/abc.txt
PASS sha224 fox: run /mnt/paghbox sha224sum /mnt/fox.txt
PASS sha224 -b: run /mnt/paghbox sha224sum -b /mnt/abc.txt
PASS sha224 check: run /mnt/paghbox sha224sum -c /mnt/sums.sha224
PASS sha224 bad check: run /mnt/paghbox sha224sum -c /mnt/sums.sha224.bad
PASS sha384 empty: run /mnt/paghbox sha384sum /mnt/empty.txt
PASS sha384 abc: run /mnt/paghbox sha384sum /mnt/abc.txt
PASS sha384 fox: run /mnt/paghbox sha384sum /mnt/fox.txt
PASS sha384 -b: run /mnt/paghbox sha384sum -b /mnt/abc.txt
PASS sha384 check: run /mnt/paghbox sha384sum -c /mnt/sums.sha384
PASS sha384 bad check: run /mnt/paghbox sha384sum -c /mnt/sums.sha384.bad
PASS sha224 status ok: run /mnt/paghbox sha224sum --status -c /mnt/sums.sha224
PASS sha384 status bad: run /mnt/paghbox sha384sum --status -c /mnt/sums.sha384.bad
PASS sha224 bad option: run /mnt/paghbox sha224sum -z
PASS crc32 check value: run /mnt/paghbox crc32 /mnt/numbers.txt
PASS crc32 abc: run /mnt/paghbox crc32 /mnt/abc.txt
PASS crc32 empty: run /mnt/paghbox crc32 /mnt/empty.txt
PASS crc32 check ok: run /mnt/paghbox crc32 -c /mnt/crc32.good
PASS crc32 check bad: run /mnt/paghbox crc32 -c /mnt/crc32.bad
PASS crc32 malformed: run /mnt/paghbox crc32 -c /mnt/crc32.malformed
PASS base58 hello: run /mnt/paghbox base58 /mnt/hello.txt
PASS base58 leading zero: run /mnt/paghbox base58 /mnt/zeros.bin
PASS base58 usage: run /mnt/paghbox base58 /mnt/hello.txt /mnt/abc.txt
PASS base58 decode hello: run /mnt/paghbox base58 -d /mnt/b58hello.txt
PASS base58 decode abc: run /mnt/paghbox base58 -d /mnt/b58abc.txt
PASS base58 invalid: run /mnt/paghbox base58 -d /mnt/b58bad.txt
PASS hex2bin write: run /mnt/paghbox hex2bin -o /mnt/decoded.bin /mnt/hexspaced.txt
PASS hex2bin round trip: run /mnt/paghbox bin2hex /mnt/decoded.bin
PASS hex2bin invalid: run /mnt/paghbox hex2bin /mnt/badhexchar.txt
PASS bin2hex lower: run /mnt/paghbox bin2hex /mnt/hello.txt
PASS bin2hex upper: run /mnt/paghbox bin2hex -u /mnt/hello.txt
PASS bin2hex wrap: run /mnt/paghbox bin2hex -w 4 /mnt/hello.txt
PASS bin2hex bad width: run /mnt/paghbox bin2hex -w x /mnt/hello.txt
PASS bin2hex missing: run /mnt/paghbox bin2hex /mnt/nope.bin
PASS bin2hex stdout hex2bin: run /mnt/paghbox hex2bin -o - /mnt/hello.hex
PASS zcat stored: run /mnt/paghbox zcat /mnt/stored.gz
PASS zcat concatenated: run /mnt/paghbox zcat /mnt/stored2.gz
PASS zcat compressed: run /mnt/paghbox zcat /mnt/compressed.gz
PASS zcat bad magic: run /mnt/paghbox zcat /mnt/bad.gz
PASS zcat bad option: run /mnt/paghbox zcat -c /mnt/stored.gz
PASS zcat missing: run /mnt/paghbox zcat /mnt/nope.gz
```

### Показательные выводы 1.7.0

```text
run /mnt/paghbox sha224sum /mnt/abc.txt
23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7  /mnt/abc.txt

run /mnt/paghbox sha384sum /mnt/fox.txt
ca737f1014a48f4c0b6dd43cb177b0afd9e5169367544c494011e3317dbf9a509cb1e5dc1e85a941bbee3d7f2afbc9b1  /mnt/fox.txt

run /mnt/paghbox crc32 /mnt/numbers.txt
cbf43926 9 /mnt/numbers.txt

run /mnt/paghbox base58 /mnt/hello.txt
tzCkV5DK

run /mnt/paghbox bin2hex -w 4 /mnt/hello.txt
6865
6c6c
6f0a

run /mnt/paghbox zcat /mnt/stored2.gz
zcat stored payload
second member
```

### Чем проверка 1.7.0 ограничена

* **Только stored-block gzip.** `zcat`/`gunzip` понимают DEFLATE BTYPE=00 —
  ровно то, что пишет `gzip` этого порта и `gzip.compress(level=0)`; поток с
  BTYPE 1/2 отвергается (проверено `compressed.gz`). Это ограничение
  декодера, а не `zcat`.
* **Нет пайпов.** Round-trip `hex2bin` → `bin2hex` и `base58` проверен через
  файлы (`hex2bin -o`, затем отдельный запуск `bin2hex`), потому что шелл
  ядра не умеет конвейеры и редиректы.
* **stdin без EOF.** Все проверки используют файловые операнды; ветки
  «файл или stdin» реализованы через `util::read_input`, но консольный fd 0
  не даёт EOF, поэтому stdin-режим в QEMU не завершается.
* **`crc32` печатает hex**, а не десятичное число POSIX `cksum`; `-c`
  принимает только собственный формат `crc size name`.
* **`base58`** — Bitcoin-алфавит без checksum (base58check нет); decode
  игнорирует ASCII whitespace, чтобы читать свой же вывод с `\n`.
* Serial-драйвер иногда теряет длинную очередь ввода; поэтому 3 команды на
  загрузку и один retry на свежем образе.

## 1.6.0 (батч 7)

Дата: 2026-11-06. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh`
(main 0.16.1; `run /mnt/paghbox uname -a` в QEMU печатает
`pagh pagh-riscv 0.10.0 0.16.1 riscv64`). QEMU `qemu-system-riscv64` 10.2.2,
машина `virt`, 256 MiB, serial на Unix-сокете, `-bios default`, modern virtio
(`-global virtio-mmio.force-legacy=false`). Диск: 64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне — `paghbox`
1.6.0, `/mnt/bin`-симлинки на `echo`/`sleep`/`true`/`false`/`cat` и фикстуры,
включая синтетический ELF `symbols.elf` с `.symtab` и `.dynsym`.

Драйвер — [`tools/live_test_160.py`](../tools/live_test_160.py): **23 загрузки
по 2–3 команды**, после каждой проверялись вывод и
`run: /mnt/paghbox exited with code N`. Итог: **67 PASS, 0 FAIL**; все 13
новых applet'ов проверены функционально, коды 0/1/2/123/124/127 и точные
строки `nm`/`more`/`shred`/`watch` сверены с моделью.

Сырой транскрипт: [`live-1.6.0-transcript.txt`](live-1.6.0-transcript.txt).
Отдельно: [`live-1.6.0-pkg-install.txt`](live-1.6.0-pkg-install.txt)
(`pkg install paghbox 1.6.0`, 107 симлинков, `timeout` и `type` из `/mnt/bin`).

### Итог 1.6.0

* `timeout` — короткая команда (`1 sleep 0.2` → 0) и абсолютный путь
  (`/mnt/paghbox echo`) работают; дедлайн `0.1 sleep 2` → **124** для
  `TERM`, `KILL` и `9`; отсутствующая команда → 127, плохая длительность или
  сигнал → 2.
* `watch` — `-t -c 2 -n 0` даёт ровно две ANSI-очистки `ESC[2J ESC[H` и два
  `hi`; без `-t` заголовок `Every 0s: /mnt/bin/echo hi`.
* `xargs` — `-n 2` (две строки), `-I{}` (четыре подстановки), default
  `echo`, пустой `-r` → 0; отсутствующая команда → 124, `false` → 123.
* `parallel` — `-j 2` и default echo дают по одной строке на токен строго
  последовательно; `-j 0` → 2.
* `install` — `-D -m 600` создаёт `/mnt/inst/new/copy.txt`, содержимое
  совпадает; повтор без `-f` → 1 (`File exists`), с `-f` → 0; неверный
  mode/операнды → 2.
* `shred` — `-n 1 -s 10` оставляет детерминированный printable-паттерн в
  `cat`; `-u` удаляет файл (`cat` → 1); `-z` даёт 10 нулей + исходный `0a`
  в `xxd`.
* `command`/`type` — `-v sleep` → `/mnt/bin/sleep`, `-V echo` →
  `echo is /mnt/bin/echo`, прямой и вложенный (`command /mnt/paghbox echo`)
  запуск; missing → 127 (запуск) или 1 (`-v`); usage → 2.
* `time` — `time echo` печатает `timed` + `real X.XXXs`, `user 0.000s`,
  `sys 0.000s`, код ребёнка; `false` → 1, missing → 127.
* `nice` — `-n 5` запускает `echo nice` (no-op по приоритету), `false` → 1,
  `-n bad` → 2.
* `exit` — `exit 7` → 7, `exit` → 0, `exit 300` → 2.
* `more` — `-n 3` и `-d -n 5` дают точные чанки с `--More--`; `-n 0` → 2.
* `nm` — `.symtab` (`d counter`, `T main`, `U puts`), `-D` (`T dynfunc`),
  `-u` (только `U puts`), `-n` (сортировка по value); не-ELF → 1, `-z` → 2.
* `pkg install` (опционально) — `pkg: installed paghbox 1.6.0`; 107
  симлинков; `/mnt/bin/timeout` и `/mnt/bin/type` работают.

### PASS-строки 1.6.0

```text
PASS timeout short: run /mnt/paghbox timeout 1 sleep 0.2
PASS timeout absolute: run /mnt/paghbox timeout 2 /mnt/paghbox echo timed-ok
PASS timeout TERM: run /mnt/paghbox timeout 0.1 sleep 2
PASS timeout KILL: run /mnt/paghbox timeout -s KILL 0.1 sleep 2
PASS timeout numeric: run /mnt/paghbox timeout -s 9 0.1 sleep 2
PASS timeout missing: run /mnt/paghbox timeout 1 /mnt/nope
PASS timeout bad duration: run /mnt/paghbox timeout bogus /mnt/bin/echo hi
PASS timeout bad signal: run /mnt/paghbox timeout -s BOGUS 1 /mnt/bin/echo hi
PASS xargs -n 2: run /mnt/paghbox xargs -a /mnt/tokens.txt -n 2 echo
PASS xargs -I: run /mnt/paghbox xargs -a /mnt/tokens.txt -I{} echo pre-{}-post
PASS xargs empty -r: run /mnt/paghbox xargs -a /mnt/empty.txt -r echo
PASS xargs default: run /mnt/paghbox xargs -a /mnt/tokens.txt
PASS xargs missing command: run /mnt/paghbox xargs -a /mnt/tokens.txt /mnt/nope
PASS xargs failure: run /mnt/paghbox xargs -a /mnt/tokens.txt -n 1 /mnt/bin/false
PASS parallel -j 2: run /mnt/paghbox parallel -j 2 -a /mnt/tokens.txt /mnt/bin/echo
PASS parallel default echo: run /mnt/paghbox parallel -a /mnt/tokens.txt
PASS parallel bad jobs: run /mnt/paghbox parallel -j 0 -a /mnt/tokens.txt echo
PASS install -D -m: run /mnt/paghbox install -D -m 600 /mnt/install_src.txt /mnt/inst/new/copy.txt
PASS install content: run /mnt/paghbox cat /mnt/inst/new/copy.txt
PASS install exists: run /mnt/paghbox install -m 600 /mnt/install_src.txt /mnt/inst/new/copy.txt
PASS install first: run /mnt/paghbox install -m 644 /mnt/install_src.txt /mnt/ow.txt
PASS install -f: run /mnt/paghbox install -f -m 644 /mnt/install_src.txt /mnt/ow.txt
PASS install force content: run /mnt/paghbox cat /mnt/ow.txt
PASS install bad mode: run /mnt/paghbox install -m 999 /mnt/install_src.txt /mnt/x
PASS install one operand: run /mnt/paghbox install /mnt/install_src.txt
PASS install bad option: run /mnt/paghbox install -z /mnt/install_src.txt /mnt/x
PASS shred -n 1 -s 10: run /mnt/paghbox shred -n 1 -s 10 /mnt/shred.txt
PASS shred cat after: run /mnt/paghbox cat /mnt/shred.txt
PASS shred -u: run /mnt/paghbox shred -u -n 1 /mnt/shred_del.txt
PASS shred remove: run /mnt/paghbox shred -u -n 1 /mnt/shred_del.txt
PASS shred gone: run /mnt/paghbox cat /mnt/shred_del.txt
PASS shred zero: run /mnt/paghbox shred -n 1 -s 10 -z /mnt/shred.txt
PASS shred zero run: run /mnt/paghbox shred -n 1 -s 10 -z /mnt/shred.txt
PASS shred zero xxd: run /mnt/paghbox xxd /mnt/shred.txt
PASS shred bad passes: run /mnt/paghbox shred -n x /mnt/shred.txt
PASS command -v: run /mnt/paghbox command -v sleep
PASS command -V: run /mnt/paghbox command -V echo
PASS command run: run /mnt/paghbox command echo direct
PASS command nested: run /mnt/paghbox command /mnt/paghbox echo nested
PASS command missing: run /mnt/paghbox command /mnt/nope
PASS command -v missing: run /mnt/paghbox command -v /mnt/nope
PASS command no operand: run /mnt/paghbox command
PASS command bad option: run /mnt/paghbox command -z
PASS command -v no operand: run /mnt/paghbox command -v
PASS type default: run /mnt/paghbox type sleep
PASS type -v: run /mnt/paghbox type -v echo
PASS type missing: run /mnt/paghbox type -V missing
PASS time echo: run /mnt/paghbox time /mnt/bin/echo timed
PASS time false: run /mnt/paghbox time /mnt/bin/false
PASS time missing: run /mnt/paghbox time /mnt/nope
PASS nice -n 5: run /mnt/paghbox nice -n 5 /mnt/bin/echo nice
PASS nice false: run /mnt/paghbox nice /mnt/bin/false
PASS nice bad: run /mnt/paghbox nice -n bad echo hi
PASS exit 7: run /mnt/paghbox exit 7
PASS exit default: run /mnt/paghbox exit
PASS exit invalid: run /mnt/paghbox exit 300
PASS more -n 3: run /mnt/paghbox more -n 3 /mnt/more.txt
PASS more -d: run /mnt/paghbox more -d -n 5 /mnt/more.txt
PASS more bad lines: run /mnt/paghbox more -n 0 /mnt/more.txt
PASS nm symtab: run /mnt/paghbox nm /mnt/symbols.elf
PASS nm dynsym: run /mnt/paghbox nm -D /mnt/symbols.elf
PASS nm undefined: run /mnt/paghbox nm -u /mnt/symbols.elf
PASS nm numeric: run /mnt/paghbox nm -n /mnt/symbols.elf
PASS nm not elf: run /mnt/paghbox nm /mnt/notelf.txt
PASS nm bad option: run /mnt/paghbox nm -z /mnt/symbols.elf
PASS watch -t -c 2: run /mnt/paghbox watch -t -c 2 -n 0 /mnt/bin/echo hi
PASS watch header: run /mnt/paghbox watch -c 1 -n 0 /mnt/bin/echo hi
```

### Показательные выводы 1.6.0

```text
run /mnt/paghbox timeout 0.1 sleep 2
run: /mnt/paghbox exited with code 124

run /mnt/paghbox xargs -a /mnt/tokens.txt -n 2 echo
alpha beta
gamma delta

run /mnt/paghbox install -D -m 600 /mnt/install_src.txt /mnt/inst/new/copy.txt
run: /mnt/paghbox exited with code 0
run /mnt/paghbox cat /mnt/inst/new/copy.txt
install me

run /mnt/paghbox shred -n 1 -s 10 /mnt/shred.txt
run: /mnt/paghbox exited with code 0
run /mnt/paghbox cat /mnt/shred.txt
gMM30WS\7A

run /mnt/paghbox more -d -n 5 /mnt/more.txt
1
2
3
4
5
--More-- (Press space to continue, 'q' to quit.)
6
7
8
9
10

run /mnt/paghbox nm -n /mnt/symbols.elf
0000000000000000 U puts
0000000000000040 d counter
0000000000001234 T main

run /mnt/paghbox watch -t -c 2 -n 0 /mnt/bin/echo hi
^[[2J^[[Hhi
^[[2J^[[Hhi
```

### Чем проверка 1.6.0 ограничена

* **Нет cross-process kill.** `kill(2)` в 0.16.1 трактует положительный pid
  как thread id собственного run; `kill(child_pid)` может попасть в
  родительский поток (в пробе это убивало сам `timeout`). Поэтому `timeout`
  на дедлайне возвращает 124 и не ждёт живого ребёнка: его снимает
  `cleanup_orphans` раннера после выхода foreground-процесса. `-s SIG`
  валидируется, но не доставляется.
* **Нет TTY.** `isatty` в ABI нет: `watch` всегда пишет ANSI-очистку в fd 1,
  `more` не ждёт клавиш. `-c N` у `watch` — расширение для теста.
* **stdin без EOF.** `xargs`/`parallel` рабочим входом используют `-a FILE`;
  stdin-режим повис бы навсегда (свойство консоли).
* **`parallel` последователен**, `-j N` принят и проигнорирован; в ABI нет
  приоритетов, поэтому `nice` — no-op, а `time` печатает `user/sys 0.000s`.
* **`install -m`** — per-run override (fchmodat не пишет inode); `-D` обходит
  ENOTSUP `mkdirat('/mnt')` через stat-before-mkdir.
* **`shred`** — best-effort: нет `fsync`, паттерн printable (удобно для
  serial-транскрипта), не secure erase.
* **`nm`** — только ELF64 little-endian, `.symtab` предпочитается `.dynsym`,
  без demangling.
* Serial-драйвер иногда теряет длинную очередь ввода; поэтому 2–3 команды на
  загрузку и один retry на свежем образе.

## 1.5.0 (батч 6)

Дата: 2026-11-06. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh`
(main 0.16.1; `run /mnt/paghbox uname -a` в QEMU печатает
`pagh pagh-riscv 0.10.0 0.16.1 riscv64`). QEMU `qemu-system-riscv64` 10.2.2,
машина `virt`, 256 MiB, serial на Unix-сокете, `-bios default`, modern virtio
(`-global virtio-mmio.force-legacy=false`). Диск: 64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне — `paghbox`
1.5.0 и фикстуры.

Драйвер — [`tools/live_test_150.py`](../tools/live_test_150.py): **34 загрузки
по 2–3 команды**, после каждой проверялись вывод и
`run: /mnt/paghbox exited with code N`. Итог: **78 PASS, 0 FAIL**; все 12
новых applet'ов проверены функционально. Команды с длинным выводом стоят
последними в загрузке: serial-драйвер теряет ввод, если гость занят печатью
длинной строки, пока хост ещё отправляет следующие команды.

Сырой транскрипт: [`live-1.5.0-transcript.txt`](live-1.5.0-transcript.txt).
Отдельно: [`live-1.5.0-pkg-install.txt`](live-1.5.0-pkg-install.txt)
(`pkg install paghbox`, 94 симлинка).

### Итог 1.5.0

* `md5sum`, `sha1sum`, `sha256sum`, `sha512sum`, `b2sum` — известные векторы
  для `""`, `"abc"` и `"The quick brown fox jumps over the lazy dog"`; `-b`,
  `-c` (`OK`/`FAILED`), `--status`; одиночный файл/stdin и несколько файлов.
* `basenc` — base64, base64url, base32, base32hex, base16; encode/decode,
  `-w 4`, unpadded decode и отказ на неверных хвостовых битах; `-i` и ошибки
  (exit 1/2).
* `gzip`/`gunzip` — запись gzip-потока с DEFLATE stored blocks, `-k`, `-f`,
  `-c`, `-d`; round-trip в госте; host-поток `gzip.compress(level=0)` прочитан
  гостем, а guest-поток распакован Python `gzip.decompress` через `debugfs`.
* `tar`/`untar` — ustar create/list/extract `tree/` (файлы, каталоги,
  симлинк), `-C`, `-f`, `-v`; host-архив с prefix-именем >100 символов
  прочитан, guest-архив разобран Python `tarfile`.
* `patch` — in-place с `.orig`, два hunks, `-p1`, `-o`, новый файл из
  `/dev/null`, `\ No newline at end of file` (проверено `xxd`), неверный
  контекст и неверные counts → exit 1.
* `cal` — февраль 2024, январь 1970, сентябрь 1752, `-y 2024`, текущий месяц
  (epoch → январь 1970) и `cal 2024`; сетка 7 колонок сверена с Python
  `calendar`; неверные месяц/год/опция → exit 2.
* `pkg install` (опционально) — `paghbox 1.5.0`, 94 симлинка; проверены
  `/mnt/bin/md5sum` и `/mnt/bin/cal`.

### PASS-строки 1.5.0

```text
PASS md5sum: run /mnt/paghbox md5sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt
PASS sha1sum: run /mnt/paghbox sha1sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt
PASS sha256sum: run /mnt/paghbox sha256sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt
PASS sha512sum: run /mnt/paghbox sha512sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt
PASS b2sum: run /mnt/paghbox b2sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt
PASS md5sum -b: run /mnt/paghbox md5sum -b /mnt/abc.txt
PASS md5sum check: run /mnt/paghbox md5sum -c /mnt/sums.md5
PASS sha256 check: run /mnt/paghbox sha256sum -c /mnt/sums.sha256
PASS md5sum bad check: run /mnt/paghbox md5sum -c /mnt/sums.bad
PASS md5sum --status: run /mnt/paghbox md5sum -c --status /mnt/sums.md5
PASS md5sum unknown option: run /mnt/paghbox md5sum -z
PASS basenc base64 encode: run /mnt/paghbox basenc --base64 /mnt/plain.txt
PASS basenc base64 decode: run /mnt/paghbox basenc --base64 -d /mnt/b64.txt
PASS basenc base64url decode: run /mnt/paghbox basenc --base64url -d /mnt/weird64url.txt
PASS basenc base32 encode: run /mnt/paghbox basenc --base32 /mnt/plain.txt
PASS basenc base32 decode: run /mnt/paghbox basenc --base32 -d /mnt/b32.txt
PASS basenc base32hex decode: run /mnt/paghbox basenc --base32hex -d /mnt/b32hex.txt
PASS basenc base16 encode: run /mnt/paghbox basenc --base16 /mnt/plain.txt
PASS basenc base16 decode: run /mnt/paghbox basenc --base16 -d /mnt/b16.txt
PASS basenc -w 4: run /mnt/paghbox basenc --base64 -w 4 /mnt/plain.txt
PASS basenc missing type: run /mnt/paghbox basenc /mnt/plain.txt
PASS basenc invalid data: run /mnt/paghbox basenc --base64 -d /mnt/bad64.txt
PASS basenc bad option: run /mnt/paghbox basenc -z
PASS gzip keep: run /mnt/paghbox gzip -k /mnt/gzip_src.txt
PASS gunzip force: run /mnt/paghbox gunzip -f /mnt/gzip_src.txt.gz
PASS gzip content: run /mnt/paghbox cat /mnt/gzip_src.txt
PASS gzip -dc host stream: run /mnt/paghbox gzip -d -c /mnt/ext.gz
PASS gunzip -c host stream: run /mnt/paghbox gunzip -c /mnt/ext.gz
PASS gzip first: run /mnt/paghbox gzip -k /mnt/plain.txt
PASS gzip exists: run /mnt/paghbox gzip --keep /mnt/plain.txt
PASS gzip force: run /mnt/paghbox gzip -fk /mnt/plain.txt
PASS gzip unknown suffix: run /mnt/paghbox gzip -d /mnt/plain.txt
PASS gzip bad option: run /mnt/paghbox gzip -z
PASS gunzip bad option: run /mnt/paghbox gunzip -z
PASS tar create: run /mnt/paghbox tar -cf /mnt/tree.tar -C /mnt tree
PASS tar list: run /mnt/paghbox tar -tf /mnt/tree.tar
PASS tar create again: run /mnt/paghbox tar -cf /mnt/tree.tar -C /mnt tree
PASS tar extract: run /mnt/paghbox tar -xf /mnt/tree.tar -C /mnt/out
PASS tar cat: run /mnt/paghbox cat /mnt/out/tree/a.txt
PASS tar list host: run /mnt/paghbox tar -tf /mnt/ext.tar
PASS tar extract host: run /mnt/paghbox tar -xf /mnt/ext.tar -C /mnt/out2
PASS tar readlink host: run /mnt/paghbox readlink /mnt/out2/extdir/link
PASS untar extract: run /mnt/paghbox untar -f /mnt/ext.tar -C /mnt/out2
PASS untar cat: run /mnt/paghbox cat /mnt/out2/extdir/one.txt
PASS tar list long: run /mnt/paghbox tar -tf /mnt/long.tar
PASS tar no mode: run /mnt/paghbox tar -f /mnt/ext.tar
PASS tar bad option: run /mnt/paghbox tar -z
PASS untar conflict: run /mnt/paghbox untar -c -f /mnt/ext.tar
PASS patch apply: run /mnt/paghbox patch -i /mnt/fix.patch
PASS patch result: run /mnt/paghbox cat /mnt/patch_me.txt
PASS patch backup: run /mnt/paghbox cat /mnt/patch_me.txt.orig
PASS patch multi: run /mnt/paghbox patch -i /mnt/multi.patch
PASS patch multi result: run /mnt/paghbox cat /mnt/multi.txt
PASS patch p1: run /mnt/paghbox patch -p1 -i /mnt/p1.patch
PASS patch p1 result: run /mnt/paghbox cat /mnt/multi2.txt
PASS patch output: run /mnt/paghbox patch -o /mnt/patched.out -i /mnt/fix2.patch
PASS patch original intact: run /mnt/paghbox cat /mnt/fix2_target.txt
PASS patch output content: run /mnt/paghbox cat /mnt/patched.out
PASS patch no newline: run /mnt/paghbox patch -i /mnt/nonl.patch
PASS patch no newline xxd: run /mnt/paghbox xxd /mnt/nonl.txt
PASS patch create file: run /mnt/paghbox patch -p1 -i /mnt/newfile.patch
PASS patch created content: run /mnt/paghbox cat /mnt/created.txt
PASS patch context fail: run /mnt/paghbox patch -i /mnt/bad.patch
PASS patch count fail: run /mnt/paghbox patch -i /mnt/badcount.patch
PASS patch missing arg: run /mnt/paghbox patch -p
PASS cal feb 2024: run /mnt/paghbox cal 2 2024
PASS cal jan 1970: run /mnt/paghbox cal 1 1970
PASS cal sep 1752: run /mnt/paghbox cal 9 1752
PASS cal -y 2024: run /mnt/paghbox cal -y 2024
PASS cal current: run /mnt/paghbox cal
PASS cal 2024 grid: run /mnt/paghbox cal 2024
PASS cal month 13: run /mnt/paghbox cal 13 2024
PASS cal year 0: run /mnt/paghbox cal 2 0
PASS cal bad option: run /mnt/paghbox cal -z
PASS host gzip create: run /mnt/paghbox gzip -k /mnt/plain.txt
PASS host tar create: run /mnt/paghbox tar -cf /mnt/tree.tar -C /mnt tree longdddddddddddddddddddddddddddddddddddddddddddddddddd
PASS host gzip: gzip.decompress(guest /plain.txt.gz)
PASS host tar: tarfile read guest /tree.tar
```

## 1.4.0 (батч 5)

Дата: 2026-11-05. Ядро: `/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh`
(main 0.16.1; `run /mnt/paghbox uname -a` в QEMU печатает
`pagh pagh-riscv 0.10.0 0.16.1 riscv64`). QEMU `qemu-system-riscv64` 10.2.2,
машина `virt`, 256 MiB, serial на Unix-сокете, `-bios default`, modern virtio
(`-global virtio-mmio.force-legacy=false`). Диск: 64 MiB ext2, создан на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне — `paghbox`
1.4.0 и фикстуры.

Драйвер — [`tools/live_test_140.py`](../tools/live_test_140.py): **21 загрузка
по 2–4 команды**, после каждой проверялись вывод и
`run: /mnt/paghbox exited with code N`. Сетевые группы запускались с `--net`
против `python3 -m http.server` на хосте (`10.0.2.2:<port>`). Итог:
**72 PASS, 0 FAIL**; все 15 новых applet'ов проверены функционально.

Сырой транскрипт: [`live-1.4.0-transcript.txt`](live-1.4.0-transcript.txt).
Отдельно: [`live-1.4.0-pkg-install.txt`](live-1.4.0-pkg-install.txt)
(`pkg install paghbox`, 82 симлинка).

### Итог 1.4.0

* `whoami`/`id` — фиксированная identity `root` (uid/gid syscall в ABI нет),
  варианты `-u`, `-g`, `-G` и `-u -g`; `env` — `PWD=/mnt` из `envp`.
* `tee` — `-i FILE` (расширение из-за отсутствия пайпов/редиректа), несколько
  выходных файлов, `-a`; байты идут и в stdout.
* `split` — `-l 2` (части 2/2/1 строки) и `-b 4 -d` (три части, проверено
  `xxd`); `csplit` — `/SECTION/`, номер `3`, `-f`, `-n 3`, `-s`; `join` —
  whitespace и `-1 2 -2 1 -t :`; `fmt` — `-w 20` и `-s`.
* `od` — `-A x|d|o`, `-t x1|o1|c` с 16-байтными строками и финальным offset;
  `xxd` — дамп, `-l`, `-s`, `-r`; `uuencode`/`uudecode` — classic-формат,
  декодирование в файл и `-o -`.
* `curl` — тело, `-i` (заголовки + тело), `-o`; `wget` — basename, `-O`,
  summary; 404 и `Connection refused` → exit 1.
* `kill` — `-0 0 0` (несколько pid), `ESRCH` → `No such process`, неизвестный
  сигнал → exit 2.
* `pkg install` (опционально) — `paghbox 1.4.0`, затем `run /mnt/bin/whoami` →
  `root` и `run /mnt/bin/xxd /mnt/hello.txt` → корректный дамп; `ls /mnt/bin`
  показывает `paghbox` + 82 симлинка.

### PASS-строки 1.4.0

```text
PASS whoami: run /mnt/paghbox whoami
PASS id: run /mnt/paghbox id
PASS id -u: run /mnt/paghbox id -u
PASS uname -a: run /mnt/paghbox uname -a
PASS id -g: run /mnt/paghbox id -g
PASS id -G: run /mnt/paghbox id -G
PASS id -u -g: run /mnt/paghbox id -u -g
PASS env: run /mnt/paghbox env
PASS env operand: run /mnt/paghbox env extra
PASS env option: run /mnt/paghbox env -i
PASS tee files: run /mnt/paghbox tee -i /mnt/tee_in.txt /mnt/tee_out.txt /mnt/tee_out2.txt
PASS tee out1: run /mnt/paghbox cat /mnt/tee_out.txt
PASS tee out2: run /mnt/paghbox cat /mnt/tee_out2.txt
PASS tee -a: run /mnt/paghbox tee -a -i /mnt/tee_in.txt /mnt/tee_append.txt
PASS tee -a cat: run /mnt/paghbox cat /mnt/tee_append.txt
PASS split -l: run /mnt/paghbox split -l 2 /mnt/split.txt /mnt/sp
PASS split aa: run /mnt/paghbox cat /mnt/spaa
PASS split ab: run /mnt/paghbox cat /mnt/spab
PASS split ac: run /mnt/paghbox cat /mnt/spac
PASS split -b -d: run /mnt/paghbox split -b 4 -d /mnt/bytes.bin /mnt/b
PASS split b00: run /mnt/paghbox xxd /mnt/b00
PASS split b01: run /mnt/paghbox xxd /mnt/b01
PASS split b02: run /mnt/paghbox xxd /mnt/b02
PASS csplit /RE/ -s: run /mnt/paghbox csplit -s -f /mnt/cs /mnt/csplit.txt /SECTION/
PASS csplit cs00: run /mnt/paghbox cat /mnt/cs00
PASS csplit cs01: run /mnt/paghbox cat /mnt/cs01
PASS csplit 3 -n3: run /mnt/paghbox csplit -f /mnt/cn -n 3 /mnt/csplit.txt 3
PASS csplit cn000: run /mnt/paghbox cat /mnt/cn000
PASS csplit cn001: run /mnt/paghbox cat /mnt/cn001
PASS join default: run /mnt/paghbox join /mnt/j1 /mnt/j2
PASS join -t -1 -2: run /mnt/paghbox join -1 2 -2 1 -t : /mnt/j3 /mnt/j4
PASS join missing: run /mnt/paghbox join /mnt/j1 /mnt/nope
PASS fmt -w20: run /mnt/paghbox fmt -w 20 /mnt/fmt.txt
PASS fmt -s -w20: run /mnt/paghbox fmt -s -w 20 /mnt/fmt.txt
PASS fmt bad width: run /mnt/paghbox fmt -w 0 /mnt/fmt.txt
PASS od x x1: run /mnt/paghbox od -A x -t x1 /mnt/hello.txt
PASS od d c: run /mnt/paghbox od -A d -t c /mnt/hello.txt
PASS od o o1: run /mnt/paghbox od -A o -t o1 /mnt/hello.txt
PASS xxd dump: run /mnt/paghbox xxd /mnt/hello.txt
PASS xxd -l -s: run /mnt/paghbox xxd -l 4 -s 1 /mnt/hello.txt
PASS xxd -r: run /mnt/paghbox xxd -r /mnt/hello.hex
PASS uuencode: run /mnt/paghbox uuencode /mnt/hello.txt hello.txt
PASS uudecode -o: run /mnt/paghbox uudecode -o /mnt/udec.txt /mnt/hello.uu
PASS udec cat: run /mnt/paghbox cat /mnt/udec.txt
PASS uudecode stdout: run /mnt/paghbox uudecode -o - /mnt/hello.uu
PASS kill -0 self x2: run /mnt/paghbox kill -0 0 0
PASS kill -0 missing: run /mnt/paghbox kill -0 999999
PASS kill bad signal: run /mnt/paghbox kill -BOGUS 0
PASS kill -TERM missing: run /mnt/paghbox kill -TERM 999999
PASS whoami usage: run /mnt/paghbox whoami x
PASS id usage: run /mnt/paghbox id x
PASS tee usage: run /mnt/paghbox tee -z
PASS split usage: run /mnt/paghbox split -z
PASS csplit usage: run /mnt/paghbox csplit
PASS join usage: run /mnt/paghbox join
PASS od usage: run /mnt/paghbox od -A q
PASS uuencode usage: run /mnt/paghbox uuencode
PASS uudecode usage: run /mnt/paghbox uudecode -z
PASS xxd usage: run /mnt/paghbox xxd -z
PASS curl bad scheme: run /mnt/paghbox curl https://10.0.2.2/x
PASS wget hostname: run /mnt/paghbox wget http://example.com/x
PASS curl body: run /mnt/paghbox curl http://10.0.2.2:50247/demo.txt
PASS curl -i: run /mnt/paghbox curl -i http://10.0.2.2:50247/demo.txt
PASS curl -o: run /mnt/paghbox curl -o /mnt/c.out http://10.0.2.2:50247/demo.txt
PASS curl file: run /mnt/paghbox cat /mnt/c.out
PASS wget default: run /mnt/paghbox wget http://10.0.2.2:50247/demo.txt
PASS wget file: run /mnt/paghbox cat /mnt/demo.txt
PASS wget -O: run /mnt/paghbox wget -O /mnt/w.out http://10.0.2.2:50247/demo.txt
PASS wget -O file: run /mnt/paghbox cat /mnt/w.out
PASS curl 404: run /mnt/paghbox curl http://10.0.2.2:50247/missing.txt
PASS wget 404: run /mnt/paghbox wget http://10.0.2.2:50247/missing.txt
PASS curl refused: run /mnt/paghbox curl http://10.0.2.2:1/x
```

### Показательные выводы 1.4.0

```text
run /mnt/paghbox whoami
root
run: /mnt/paghbox exited with code 0

run /mnt/paghbox id
uid=0 gid=0(root) groups=0(root)

run /mnt/paghbox uname -a
pagh pagh-riscv 0.10.0 0.16.1 riscv64

run /mnt/paghbox tee -i /mnt/tee_in.txt /mnt/tee_out.txt /mnt/tee_out2.txt
tee input line
run: /mnt/paghbox exited with code 0

run /mnt/paghbox split -l 2 /mnt/split.txt /mnt/sp
run: /mnt/paghbox exited with code 0
# cat /mnt/spaa → one/two; spab → three/four; spac → five

run /mnt/paghbox csplit -s -f /mnt/cs /mnt/csplit.txt /SECTION/
run: /mnt/paghbox exited with code 0
# cat /mnt/cs00 → alpha/beta; cs01 → SECTION/gamma/delta

run /mnt/paghbox join -1 2 -2 1 -t : /mnt/j3 /mnt/j4
1:x:p
4:d:r

run /mnt/paghbox fmt -w 20 /mnt/fmt.txt
The quick brown fox
jumps over the lazy
dog and keeps on
running. A short
second line here.

A separate paragraph
follows with a few
words.

run /mnt/paghbox od -A x -t x1 /mnt/hello.txt
000000 68 65 6c 6c 6f 0a
000006

run /mnt/paghbox xxd /mnt/hello.txt
00000000: 6865 6c6c 6f0a                           hello.

run /mnt/paghbox uuencode /mnt/hello.txt hello.txt
begin 644 hello.txt
&:&5L;&\*
end

run /mnt/paghbox curl http://10.0.2.2:50247/demo.txt
network ok

run /mnt/paghbox curl -i http://10.0.2.2:50247/demo.txt
HTTP/1.0 200 OK
Server: SimpleHTTP/0.6 Python/3.14.7
Date: Thu, 05 Nov 2026 20:51:24 GMT
Content-type: text/plain
Content-Length: 11

network ok

run /mnt/paghbox wget -O /mnt/w.out http://10.0.2.2:50247/demo.txt
wget: saved /mnt/w.out (11 bytes)

run /mnt/paghbox kill -0 0 0
run: /mnt/paghbox exited with code 0

run /mnt/paghbox kill -TERM 999999
kill: 999999: No such process
run: /mnt/paghbox exited with code 1
```

### `pkg install` 1.4.0 (опционально)

`repo/` отдан `python3 -m http.server <port> --bind 127.0.0.1 -d repo`; гость
видит хост как `10.0.2.2:<port>` (QEMU `--net`, kernel 0.16.1):

```text
pagh:rv64> pkg repo http://10.0.2.2:42661/
pkg: repo http://10.0.2.2:42661/
pagh:rv64> pkg index
pkg: index 1 package(s)
pagh:rv64> pkg install paghbox
pkg: installed paghbox 1.4.0
pagh:rv64> run /mnt/bin/whoami
root
pagh:rv64> run /mnt/bin/xxd /mnt/hello.txt
00000000: 6865 6c6c 6f0a                           hello.
```

`ls /mnt/bin` показывает `paghbox` и 82 симлинка; `MAX_COMMANDS = 160`
покрывает bundle.

### Чем проверка 1.4.0 ограничена

* Шелл ядра по-прежнему не понимает кавычки, пайпы и редиректы. Поэтому
  `tee` читает файл через `-i FILE`, а `uuencode`/`uudecode` проверены через
  файлы; stdin-ветки (кроме `-i -`) живым запуском недоступны.
* У консольного fd 0 нет EOF, поэтому любой фильтр без файлового операнда
  ждёт ввод вечно — это свойство среды, а не заглушка.
* `curl`/`wget` — только `http://` + IPv4-литерал; DNS/TLS/редиректов нет,
  HTTP/1.0 с `Connection: close`.
* `kill` адресует только потоки своего процесса; чужой pid даёт ESRCH, а
  `libpagh` не имеет ESRCH в таблице errno (печатается `No such process`).
* Serial-драйвер иногда теряет длинную очередь ввода; драйвер делает 2–4
  команды на загрузку и один retry на свежем образе.
* `pkg install` проверялся отдельно (см. выше); основной прогон использует
  `run /mnt/paghbox`, чтобы не смешивать applet'ы и сеть.

## 1.3.0 (история)

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

### Итог

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

### PASS-строки

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

### Показательные выводы

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

### `pkg install` на 0.16.0 (опционально)

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

### Чем проверка ограничена

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
