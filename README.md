# pagh-programs — портированные программы для pagh-riscv

Репозиторий портов: программы написаны **под нативный API `libpagh`**
(соседний checkout `../pagh-api`), а не под Linux-совместимый слой. Один
busybox-style ELF `paghbox` содержит 82 applet'а; `tools/build_repo.py`
упаковывает его в `.ppkg` и пишет `repo/index`. Установщик `pkg` в ядре
pagh-riscv — потребитель этого репозитория: скачивает пакет по HTTP, сверяет
SHA-256/размер, ставит ELF в `/mnt/bin` и создаёт симлинки на команды.

## Структура

```
catalog.md                 целевые 120 программ и честный статус (82 [x])
docs/package-format.md     формат .ppkg и index (контракт с ядром)
programs/                  crate paghbox: dispatch + applets/
  src/main.rs              argv[0]/`paghbox <applet>` + exit codes
  src/util.rs              EAGAIN-retry чтение, Input, вывод
  src/applets/             82 модуля: echo true false yes pwd basename dirname seq
                           cat wc head tail ls stat uname hostname arch nproc sleep
                           uptime date readlink realpath uniq sort nl fold expand
                           unexpand rev cut tr comm paste strings hexdump grep
                           rmdir tac sum cksum base64 base32 diff cmp sed awk
                           shuf numfmt whoami id env tee split csplit join fmt
                           od xxd uuencode uudecode curl wget kill
                           (+ bre/codec/httpc/uu helpers)
tools/build_repo.py        cargo build -> .ppkg -> index, и --check
tools/live_test_140.py     QEMU live suite 1.4.0 (21 загрузка, 72 проверки)
repo/                      сгенерировано, в git не входит (см. ниже)
```

## Сборка

Тулчейн закреплён в [`rust-toolchain.toml`](rust-toolchain.toml)
(`nightly-2026-07-12`, как у `pagh-api` и `pagh-riscv`), цель — в
`.cargo/config.toml`. Линкер-скрипт передаёт `programs/build.rs` абсолютным
путём (`../pagh-api/pagh-user.ld` относительно `programs/`), чтобы сборка не
зависела от текущего каталога.

```sh
# из корня pagh-programs
cargo build --release --target riscv64gc-unknown-none-elf
python3 tools/build_repo.py
python3 tools/build_repo.py --check
```

`build_repo.py` без аргументов сам вызывает cargo, затем пишет
`repo/pkg/paghbox-1.4.0.ppkg` и `repo/index`. `--elf PATH` упаковывает готовый
ELF, `--no-build` берёт стандартный путь без cargo, `--check` только
перечитывает index/пакеты и проверяет magic, длины, манифест, SHA-256, размер
payload'а и отсутствие «осиротевших» `.ppkg`.

### Path-зависимость и fallback

`programs/Cargo.toml` содержит одну строку:

```toml
libpagh = { path = "../../pagh-api/libpagh" }
```

Из каталога `programs/` это `/home/oleg/pagh-api/libpagh` при текущей
раскладке (оба репозитория — соседи в `/home/oleg`). В задании путь был указан
как `../pagh-api/libpagh`; для crate'а внутри `programs/` он на уровень
мельче, поэтому здесь `../../`. Cargo **не умеет** fallback для path-зависимости:
если `pagh-api` лежит в другом месте, нужно либо положить его соседом
(`/home/oleg/pagh-api`), либо поправить эту одну строку. Ошибка при
отсутствии выглядит как «failed to load source for dependency `libpagh`».
`libpagh` в этот репозиторий не копируется и submodule'ом не является.

## Запуск через HTTP (гость → хост)

```sh
python3 -m http.server 8000 -d repo
```

Хост слушает `0.0.0.0:8000`, гость в QEMU видит его как `10.0.2.2:8000`:

```
pkg repo http://10.0.2.2:8000/
pkg index
pkg list
pkg install paghbox
paghbox echo hello
echo hello                    # симлинк /mnt/bin/echo -> paghbox
cat /mnt/...                  # после того, как ядро реализует openat/read
```

`pkg` — установщик в ядре, а не apt: без Debian-метаданных, зависимостей и
подписей. Он читает `index`, качает `pkg/<name>-<version>.ppkg`, проверяет
payload по SHA-256/размеру **до** записи и создаёт симлинки по `commands=` из
манифеста. Формат заморожен в [`docs/package-format.md`](docs/package-format.md).

### `size` в index

В `index` поля `size` и `sha256` описывают **весь `.ppkg`** — те байты,
которые установщик скачивает и хеширует. Манифест внутри пакета хранит
`sha256` только payload'а (ELF) и проверяется после этой загрузочной проверки.
Полная длина файла складывается из заголовка:
`16 + manifest_len + payload_len`. `--check` проверяет именно такую трактовку;
это же зафиксировано в [`docs/package-format.md`](docs/package-format.md).

## Applet'ы (82)

| Команда | Что делает | Коды выхода |
|---|---|---|
| `echo` | аргументы через пробел; `-n`, `-e` (`\n \t \r \\ \a \b \f \v \e \0NNN \xHH \c`), `-E` | 0; 1 при ошибке записи |
| `true` | ничего, `exit 0` | 0 |
| `false` | ничего, `exit 1` | 1 |
| `yes` | бесконечно повторяет строку (по умолчанию `y`), пока fd 1 принимает | 0 (не завершается); 1 при ошибке записи |
| `pwd` | печатает `PWD` (fallback `/`), принимает `-L`/`-P` | 0; 2 на лишний аргумент |
| `basename` | лексическое имя файла, необязательный суффикс | 0; 2 на аргументы |
| `dirname` | лексический каталог (`/a`→`/`, `a`→`.`) | 0; 2 на аргументы |
| `seq` | целые `[FIRST [INCR]] LAST`, `-s SEP`; нулевой шаг — ошибка | 0; 1 на число; 2 на синтаксис |
| `cat` | файлы или stdin, `-` = stdin, `-u` игнорируется | 0; 1 на open/read/write (остальные файлы продолжаются) |
| `wc` | `-l -w -c`, файлы/stdin, total при нескольких | 0; 1 при ошибке; 2 на опцию |
| `head` | первые N строк (10), `-n N`/`-N`, баннеры при нескольких файлах | 0; 1 на open/read/write; 2 на синтаксис |
| `tail` | последние N строк (10), те же опции; читает файл целиком в память | 0; 1; 2 |
| `ls` | каталоги/файлы, `-a` (принят: в VFS нет `.`/`..`), `-1`, `-l` (`drwxr-xr-x 4096 name/`), `-d`; при нескольких операндах заголовок `ls: path:`; каталоги с `/` | 0; 1 на операнд; 2 на опцию |
| `stat` | `path: kind, size=N, mode=0oNNN, ino=N`; несколько операндов | 0; 1; 2 |
| `uname` | `-s -n -r -v -m`, `-a`; по умолчанию sysname | 0; 1 (syscall); 2 |
| `hostname` | `nodename` из `uname` | 0; 1; 2 |
| `arch` | `machine` из `uname` | 0; 1; 2 |
| `nproc` | всегда `1` (один hart, одна user-задача) | 0; 2 |
| `sleep` | секунды, включая `0.5`/`1.5`; `nanosleep`, округление вверх до тика | 0; 1; 2 |
| `uptime` | `up <sec>s` из `clock_gettime(1)` | 0; 1; 2 |
| `date` | `epoch=<sec>.<nsec>` из `clock_gettime(0)`; часового пояса нет | 0; 1; 2 |
| `readlink` | цель симлинка как есть; не-симлинк — ошибка | 0; 1; 2 |
| `realpath` | лексическая нормализация через `path::resolve`; симлинки не разворачиваются, путь может не существовать | 0; 2 |
| `uniq` | `-c` (count), `-d`, `-u`; только подряд идущие дубликаты | 0; 1; 2 |
| `sort` | `-r`, `-n` (десятичные, точное целочисленное сравнение), `-u`; несколько файлов как один поток | 0; 1; 2 |
| `nl` | нумерация непустых строк, `-b a` — всех; ширина 6 + tab | 0; 1; 2 |
| `fold` | `-w N` (80), побайтный перенос | 0; 1; 2 |
| `expand` | `-t N` (8), табы в пробелы по столбцам | 0; 1; 2 |
| `unexpand` | `-a`; шаг фиксирован 8, по умолчанию только ведущие пробелы | 0; 1; 2 |
| `rev` | обратный порядок байтов каждой строки | 0; 1; 2 |
| `cut` | `-d CHAR`, `-f LIST` (`1`, `3-5`, `2,4-6`, `4-`, `-3`); файлы/stdin | 0; 1; 2 |
| `tr` | `SET1 SET2`, `-d`, `-s`, диапазоны `a-z`; всегда stdin (пайпов/редиректа нет) | 0; 1; 2 |
| `comm` | две отсортированные файла, три колонки, `-1/-2/-3`; позиционные табы как GNU | 0; 1; 2 |
| `paste` | `-d LIST`, несколько файлов, строка к строке | 0; 1; 2 |
| `strings` | серии ≥N (4) печатных ASCII (`0x20..0x7e`), `-n N`; потоково | 0; 1; 2 |
| `hexdump` | `-C`-подобный вид: offset, 16 байт hex, ASCII; несколько файлов — один поток | 0; 1; 2 |
| `grep` | литерал или BRE-подмножество (`^ $ . * [] [^] \`), `-i -v -n -c -l -F`; несколько файлов; 0 матч / 1 нет / 2 ошибка | 0; 1; 2 |
| `rm` | файлы/каталоги, `-r/-R`, `-f`, `-d`; симлинк удаляется как ссылка | 0; 1 при ошибке |
| `mkdir` | каталоги, `-p`; mode принимается и игнорируется (нет set_mode) | 0; 1; 2 |
| `touch` | создать файл, существующий не обрезает; utime в ABI нет | 0; 1; 2 |
| `cp` | файлы, `-r` каталоги, `-f`, симлинки копируются ссылкой | 0; 1; 2 |
| `mv` | `renameat2` в пределах одной ФС; cross-fs → ENOTSUP | 0; 1; 2 |
| `ln` | hard link по умолчанию, `-s` символическая (цель как есть) | 0; 1; 2 |
| `truncate` | `-s SIZE`, `-c`; ftruncate | 0; 1; 2 |
| `chmod` | octal MODE; per-run override, **не persist** (fchmodat) | 0; 1; 2 |
| `find` | пути, `-name GLOB`, `-type f/d/l`, `-maxdepth N`, `-print`; без -delete/-exec | 0; 1; 2 |
| `du` | `-s`, `-a`, `-h`; рекурсивные размеры | 0; 1; 2 |
| `tree` | рекурсивное дерево, `-L N`; счётчики каталогов/файлов | 0; 1; 2 |
| `file` | пусто/ELF/каталог/симлинк/текст-vs-бинарный по содержимому | 0; 1; 2 |
| `printf` | `%s %d %i %x %X %o %c %%`, escapes; аргументы циклятся | 0; 1; 2 |
| `expr` | целочисленная арифметика, сравнения, `!`; 0/1/2 | 0; 1; 2 |
| `test` / `[` | `-e -f -d -s -L -r -w -x -z -n`, `= != -eq…` | 0/1/2 |
| `factor` | trial division u64; `N: p q …` | 0; 1; 2 |
| `which` | `/mnt/bin/<name>`, затем `<PWD>/<name>`; PATH не задан | 0; 1; 2 |
| `mktemp` | `/mnt/tmp.XXXXXX` или `-p DIR`; pid+счётчик, без O_EXCL | 0; 1; 2 |
| `rmdir` | пустые каталоги, `-p` — родители; `ENOTEMPTY` в диагностике; отказ для `.`/`..`/`/` | 0; 1; 2 |
| `tac` | обратный порядок строк файла/stdin; последняя строка без `\n` дополняется | 0; 1; 2 |
| `sum` | BSD 16-бит rotate (по умолчанию) и SysV (`-s`); `checksum blocks [name]` | 0; 1; 2 |
| `cksum` | POSIX CRC-32 + размер: `crc size [name]` | 0; 1; 2 |
| `base64` | encode/decode (`-d`), `-w N` (76; 0 — без переноса) | 0; 1 (ввод); 2 |
| `base32` | RFC 4648 base32, те же `-d`/`-w N` | 0; 1 (ввод); 2 |
| `diff` | normal-формат `NaM`/`NdM`/`NcM`, `<`/`>`/`---`; LCS (Hirschberg) | 0 одинаковы; 1 различия; 2 ошибка |
| `cmp` | первый байт: `byte N, line L, offset 0xH`; `-s`, `-l` (octal); EOF-диагностика | 0; 1; 2 |
| `sed` | `s/RE/REP/[g]`, `p`, `d`, адреса `N`/`N,M`, `-n`; BRE-lite, без `-i` | 0; 1; 2 (скрипт) |
| `awk` | `-F`, `$0/$N`, `NR`, `NF`, `/re/`, сравнения, `{print ...}`; без массивов/BEGIN | 0; 1; 2 (программа) |
| `shuf` | строки/stdin, `-n N`, `-e ARGS...`, `-i LO-HI`; фиксированный xorshift seed | 0; 1; 2 |
| `numfmt` | `--to`/`--from=si\|iec\|auto`, `--to-unit`, `--suffix`, `--padding`; u128, без FP | 0; 1 (число); 2 |
| `whoami` | `root`; uid/gid syscall в ABI пока нет | 0; 1 (write); 2 |
| `id` | `uid=0 gid=0(root) groups=0(root)`; `-u`, `-g`, `-G` (и их комбинации) | 0; 1; 2 |
| `env` | пары `KEY=VALUE` из `envp` (`PWD=/...`); без exec-режима и опций | 0; 1; 2 |
| `tee` | stdin → stdout + файлы, `-a`; расширение `-i FILE` из-за отсутствия пайпов | 0; 1; 2 |
| `split` | `-l N` (1000), `-b N`, `-d`, `[FILE [PREFIX]]`; 676/100 частей | 0; 1; 2 |
| `csplit` | `N`, `/RE/` (BRE-lite), `-f`, `-n`, `-s`; печатает размеры частей | 0; 1; 2 |
| `join` | две сортированные файла, `-1 N`, `-2 N`, `-t CHAR`; без `-a` | 0; 1; 2 |
| `fmt` | reflow абзацев, `-w N` (75), `-s` — только разбивать длинные строки | 0; 1; 2 |
| `od` | `-A x\|o\|d`, `-t x1\|o1\|c`, 16 байт/строка + финальный offset | 0; 1; 2 |
| `xxd` | offset/hex/ASCII; `-r` (reverse), `-l N`, `-s OFF` | 0; 1 (hex/io); 2 |
| `uuencode` | classic: `begin 644 NAME`, группы 45, `end`; без `-m` | 0; 1; 2 |
| `uudecode` | classic decode в файл из `begin` или `-o FILE` (`-` = stdout) | 0; 1 (ввод); 2 |
| `curl` | HTTP/1.0 GET по IPv4, `-i`, `-o FILE`; `Connection: close` | 0; 1 (сеть/HTTP); 2 |
| `wget` | тот же GET, файл по basename или `-O FILE`; summary в stderr | 0; 1; 2 |
| `kill` | TERM/KILL/INT/USR1/USR2/STOP/CONT/0 и номера, несколько pid | 0; 1 (в т.ч. ESRCH); 2 |


`cat/wc/head/tail` и новые файловые фильтры читают через `libpagh`: `read` на
fd файла или 0, а на `EAGAIN` (консольный ввод без байта) делают
`sched_yield` и повторяют. `0` байт = EOF. У консольного fd 0 в текущем ядре
EOF не наступает, поэтому `cat`, `sort`, `grep`, `tr` и остальные без файлового
операнда ждут ввод вечно — это свойство ядра. Следствие: `tr` по спецификации
читает stdin, и без пайпов/редиректа его преобразование нельзя проверить в
QEMU; живым запуском проверен его разбор аргументов (`tr`, `tr -d z-a`,
`tr a b c`, `tr -z a b` → exit 2). Шелл ядра делит строку по пробелам и не
понимает кавычки, поэтому `grep`-паттерн с пробелом недоступен.

Не поддерживается намеренно: пайпы и редиректы (их нет в шелле ядра),
отрицательные `head -n -N`, `tail -f`, `wc -m`, суффиксы/опции GNU сверх
перечисленных. `ls` без цветов/рекурсии, `stat -c`, `sort -k/-t/-o`,
`uniq` без выходного файла, `cut -c/-b/--complement`, `tr -c/-t` и именованные
классы, `grep` без alternation/групп/ERE, `hexdump` только `-C`-вид.
Подмножества `sed`/`awk`/`diff`/`cmp`/`numfmt` перечислены в их модулях и в
разделе «Ограничения среды и ABI». FPU в user-mode включается ядром с 0.11.0,
но `sort -n`, `awk`, `numfmt` и остальные новые инструменты сознательно не
используют плавающую точку: сравниваются десятичные знаки точно (без
экспоненты и `inf`), чтобы порядок не зависел от округления f64. Форматирование
`wc` — политика порта, а не ABI.

## Статус и тестирование

Сборка: `cargo build --release --target riscv64gc-unknown-none-elf` даёт
статический RV64 ELF; `python3 tools/build_repo.py` пишет
`repo/pkg/paghbox-1.4.0.ppkg`, а `python3 tools/build_repo.py --check` зелёный.
Пакет `paghbox 1.4.0` содержит 82 команды; его размер и SHA-256 приведены в
разделе [`repo/`](#repo-и-воспроизводимость) ниже. `MAX_COMMANDS = 160` в
установщике 0.16.1 с запасом покрывает 82.

Живой прогон сделан на **ядре 0.16.1** (`/home/oleg/pagh-riscv/target/.../debug/pagh`,
native ABI + pkg installer), QEMU `virt`, 256 MiB, диск 64 MiB ext2
(`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`). Драйвер —
[`tools/live_test_140.py`](tools/live_test_140.py): он собирает образ, копирует
`paghbox` и фикстуры и делает **21 загрузку по 2–4 команды** (короткие группы
обязательны: serial-драйвер теряет длинные строки), проверяя вывод и
`run: ... exited with code N` после каждой. Результат: **72 PASS, 0 FAIL**, все
15 новых applet'ов проверены функционально. Сеть проверена на реальном
HTTP-сервере (`python3 -m http.server`) на хосте: гость обращается к нему как
`10.0.2.2:<port>` с `--net`.

Сырые транскрипты: [`docs/live-1.4.0-transcript.txt`](docs/live-1.4.0-transcript.txt)
(21 загрузка), [`docs/live-1.4.0-pkg-install.txt`](docs/live-1.4.0-pkg-install.txt)
(`pkg install` и симлинки), PASS-строки и таблица:
[`docs/live-testing.md`](docs/live-testing.md).

| Проверка | Результат |
|---|---|
| `run /mnt/paghbox uname -a` | `pagh pagh-riscv 0.10.0 0.16.1 riscv64` (release/version-константы ядра; Cargo-версия main — 0.16.1) |
| `whoami`, `id`, `id -u/-g/-G` | `root`; `uid=0 gid=0(root) groups=0(root)`; `0`/`0`/`0`; `id -u -g` → `0 0` |
| `env` | `PWD=/mnt`; `env extra`/`env -i` → exit 2 |
| `tee` | `tee -i in out1 out2` пишет в оба файла и stdout; `tee -a` дописывает (`prefix` + строка) |
| `split` | `-l 2` → `spaa` (one/two), `spab` (three/four), `spac` (five); `-b 4 -d` → `b00..b02` |
| `csplit` | `/SECTION/` → `cs00`/`cs01`; `3` печатает `11`/`20`; `-n 3` → `cn000`/`cn001` |
| `join` | default → `a 1 A`, `d 4 D`; `-1 2 -2 1 -t :` → `1:x:p`, `4:d:r` |
| `fmt` | `-w 20` reflow; `-s -w 20` сохраняет короткие строки; `-w 0` → exit 2 |
| `od` | `-A x -t x1` → `000000 68 65 6c 6c 6f 0a` + `000006`; `d`/`c`/`o` формы |
| `xxd` | `00000000: 6865 6c6c 6f0a ... hello.`; `-l 4 -s 1`; `-r` → `hello` |
| `uuencode`/`uudecode` | `begin 644 hello.txt ... end`; decode через файл и `-o -` → `hello` |
| `curl` | body `network ok`; `-i` → `HTTP/1.0 200 OK` + body; `-o /mnt/c.out`; 404 → exit 1 |
| `wget` | basename `demo.txt`; `-O /mnt/w.out`; summary `wget: saved ... (11 bytes)`; 404/refused → exit 1 |
| `kill` | `-0 0 0` → exit 0; `999999` → `kill: 999999: No such process`, exit 1; `-BOGUS` → exit 2 |
| usage/ошибки | 15 новых: неизвестная опция/аргумент → exit 2; bad URL/hostname → exit 2; missing file/HTTP → exit 1 |

Дополнительно (опционально) проверен `pkg install paghbox` на чистом 64 MiB
ext2 с `repo/` на хосте: `pkg: installed paghbox 1.4.0`, затем
`run /mnt/bin/whoami` → `root`, `run /mnt/bin/xxd /mnt/hello.txt` → дамп, а
`ls /mnt/bin` показывает `paghbox` и 82 симлинка. Это подтверждает, что
`commands=` манифеста и `MAX_COMMANDS = 160` сходятся с реальным пакетом.

## Ограничения среды и ABI

* **Шелл ядра** делит строку по пробелам и не понимает кавычки, пайпы и
  редиректы. Поэтому `awk`-программа с пробелами недоступна: в живой проверке
  используются компактные формы `{print$1,$2}` и `-F:`; `sed`-скрипт тоже
  передаётся одним словом.
* **У консольного fd 0 нет EOF**, а пайпов нет. Ветки «файл или stdin» есть у
  файловых инструментов (`-` = stdin через `util::Input`), но живым запуском
  проверены файловые операнды; `tee` использует `-i FILE`, а stdin-режим `tr`
  проверен только разбором аргументов и остаётся ограничением среды.
* **`shuf`** намеренно использует фиксированный seed
  (`0x9E3779B97F4A7C15`, xorshift64*), потому что в целевом ABI нет
  `getrandom`; одинаковый вход даёт одинаковый порядок. Это записано в
  `programs/src/applets/shuf.rs`.
* **Подмножества:** `sed` без `-i`, `-e`, групп/backrefs, `y/a/i/c/r/w`;
  `awk` без массивов, функций, арифметики, `BEGIN`/`END`, `printf`, `getline`
  и переменных; `diff` только normal-формат (без `-u` и рекурсии); `cmp` при
  ошибке чтения возвращает 1 по правилу задания, а не GNU-е 2; `numfmt` не
  читает stdin.
* **`numfmt`** считает в `u128`/десятичных долях, без плавающей точки; при
  выбранной единице печатает один десятичный знак с округлением от нуля
  (GNU `from-zero`) и переносом в следующую единицу (`999999` → `1.0M`).
  `--to=auto` — политика порта: IEC, если значение ≥ 1024 и делится на 1024
  нацело, иначе SI от 1000.
* **`diff`** использует LCS (Hirschberg), поэтому его разбиение на hunks может
  отличаться от GNU при нескольких равных LCS; формат и применяемость патча
  проверены фаззингом на хосте (300 случайных пар, `patch` восстанавливает
  правый файл).
* **FPU:** новые инструменты не используют `f32`/`f64`; `numfmt` и `awk`
  сравнивают десятичные строки/`u128` точно, поэтому результат не зависит от
  FPU-политики ядра.
* **Идентичность:** в ABI нет uid/gid syscall, поэтому `whoami` всегда печатает
  `root`, а `id` — нулевые uid/gid и группу `root`. Это не заглушка: команды
  реализуют интерфейс, а значения станут реальными, когда появится syscall.
* **`tee`:** обычный режим читает stdin, но у консольного fd 0 нет EOF, а
  пайпов/редиректа в шелле нет. Поэтому порт добавил `-i FILE` (файл-источник);
  без `-i` команда ждёт ввод вечно, как и остальные stdin-фильтры. `-i` —
  расширение порта, отмеченное в модуле и README.
* **`split`/`csplit`:** суффиксы ограничены двумя символами (676 alphabetic
  или 100 numeric частей); у `csplit` только `N`, `/RE/`, `-f`, `-n`, `-s`
  (нет `%RE%`, смещений `/RE/+N` и `{N}`). `fmt` не сохраняет отступы
  reflow-абзацев и не реализует sentence-spacing GNU; ширина считается в
  байтах. `od` по умолчанию печатает `x1`, а не GNU-е `o2`; `xxd -r` — базовый
  (без `*`-сжатия); `uuencode` только classic (без `-m`/base64), режим 644.
* **`curl`/`wget`:** только `http://` и IPv4-литерал (DNS/TLS/редиректов нет),
  HTTP/1.0 с `Connection: close`; тело читается до закрытия соединения, без
  chunked-декодера. `curl` не следует 3xx и считает любой не-2xx ошибкой
  (exit 1); `wget` пишет summary в stderr, чтобы не портить `-O -`.
* **`kill`:** ядро 0.16.1 адресует только потоки собственного процесса
  (`pid > 0`), `0` — вся группа; чужой pid отвечает ESRCH. В замороженной
  таблице `libpagh` нет ESRCH, поэтому `Errno::Unknown` на этом пути
  печатается как `No such process`. Шелл не раскрывает `$$`; безопасная проба —
  `kill -0 0`.
* **Ядро 0.16.1** уже даёт fork/exec/wait, файловые мутации, mmap, pipes, poll,
  sockets, threads и signals (52 syscall'а), но `paghbox` остаётся набором
  CLI-фильтров: шелл ядра не создаёт конвейеров, а `pkg` ставит один bundle.
  Соседние репозитории (`pagh-riscv`, `pagh-api`) не изменялись.

## `repo/` и воспроизводимость

`target/` и `repo/` в [`.gitignore`](.gitignore): бинарники не коммитятся.
`repo/` полностью воспроизводится командой `python3 tools/build_repo.py` из
исходников и пинованного тулчейна; индекс детерминирован по содержимому
(единственный недетерминизм — сам ELF от rustc). Текущий пакет — 610960
байт payload / 611641 байт `.ppkg`, 82 команды, SHA-256
`3a4b2e0826856c4519783ef2c8edd87cf3f83120e0a52097ed96b1b528c68b28`; в git он не попал (`.gitignore`), его место — на
HTTP-зеркале.

`catalog.md` — ровно 120 уникальных имён; `[x]` стоят у 82 реализованных,
остальные `[ ]`. Список проверяется `tools/live_test_140.py` и `grep`-ом по
`NAMES`: все 82 имени входят в `commands=` пакета.