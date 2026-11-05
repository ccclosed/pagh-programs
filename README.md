# pagh-programs — портированные программы для pagh-riscv

Репозиторий портов: программы написаны **под нативный API `libpagh`**
(соседний checkout `../pagh-api`), а не под Linux-совместимый слой. Один
busybox-style ELF `paghbox` содержит 120 applet'ов; `tools/build_repo.py`
упаковывает его в `.ppkg` и пишет `repo/index`. Установщик `pkg` в ядре
pagh-riscv — потребитель этого репозитория: скачивает пакет по HTTP, сверяет
SHA-256/размер, ставит ELF в `/mnt/bin` и создаёт симлинки на команды.

## Структура

```
catalog.md                 целевые 120 программ и честный статус (120 [x])
docs/package-format.md     формат .ppkg и index (контракт с ядром)
programs/                  crate paghbox: dispatch + applets/
  src/main.rs              argv[0]/`paghbox <applet>` + exit codes
  src/util.rs              EAGAIN-retry, Input, which/spawn/wait, duration
  src/applets/             120 модулей:
                           echo true false yes pwd basename dirname seq
                           cat wc head tail ls stat uname hostname arch nproc sleep
                           uptime date cal readlink realpath uniq sort nl fold expand
                           unexpand rev cut tr comm paste strings hexdump grep tac
                           cp mv rm mkdir rmdir touch ln truncate chmod find
                           du tree file printf expr calc test factor which mktemp
                           hex2bin bin2hex sum cksum crc32
                           md5sum sha1sum sha224sum sha256sum sha3sum sha384sum
                           sha512sum b2sum base58 base85 base64 base32 basenc
                           diff cmp patch sed awk shuf numfmt whoami id env
                           tee split csplit join fmt od xxd uuencode uudecode
                           curl wget kill gzip gunzip zcat tar untar
                           timeout watch xargs parallel install shred command
                           type time exit nice more nm readelf
                           uname-родственные: uuidgen paghfetch
                           (+ bre/codec/digest/elf/hashsum/httpc/uu helpers)
tools/build_repo.py        cargo build -> .ppkg -> index, и --check
tools/live_test_170.py     QEMU live suite 1.7.0 (14 загрузок, 42 проверки)
tools/live_test_180.py     QEMU live suite 1.8.0 (13 загрузок)
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
`repo/pkg/paghbox-1.7.0.ppkg` и `repo/index`. `--elf PATH` упаковывает готовый
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

## Applet'ы (120)

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
| `paghfetch` | баннер из реальных `uname` (sysname/nodename/release/machine), uptime, nproc=1, PWD; без выдуманных данных | 0; 1 (uname); 2 |
| `date` | `epoch=<sec>.<nsec>` из `clock_gettime(0)`; часового пояса нет | 0; 1; 2 |
| `cal` | месяц/год, `-y`; григорианская сетка 7 колонок; «сейчас» — epoch | 0; 1 (clock); 2 |
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
| `calc` | целочисленные `+ - * / %`, скобки, унарный минус; операнды или `-e EXPR`; i64 checked | 0; 1 (деление на 0/overflow); 2 (синтаксис) |
| `test` / `[` | `-e -f -d -s -L -r -w -x -z -n`, `= != -eq…` | 0/1/2 |
| `factor` | trial division u64; `N: p q …` | 0; 1; 2 |
| `which` | `/mnt/bin/<name>`, затем `<PWD>/<name>`; PATH не задан | 0; 1; 2 |
| `mktemp` | `/mnt/tmp.XXXXXX` или `-p DIR`; pid+счётчик, без O_EXCL | 0; 1; 2 |
| `rmdir` | пустые каталоги, `-p` — родители; `ENOTEMPTY` в диагностике; отказ для `.`/`..`/`/` | 0; 1; 2 |
| `tac` | обратный порядок строк файла/stdin; последняя строка без `\n` дополняется | 0; 1; 2 |
| `sum` | BSD 16-бит rotate (по умолчанию) и SysV (`-s`); `checksum blocks [name]` | 0; 1; 2 |
| `cksum` | POSIX CRC-32 + размер: `crc size [name]` | 0; 1; 2 |
| `crc32` | IEEE/zlib CRC-32 (`0xEDB88320`) + размер: `cccccccc size [name]`, `-c` | 0; 1; 2 |
| `md5sum` | MD5 файлов/stdin, `-b`, `-c` по `md5sum`-файлу; `hex  name` | 0; 1 (несовпадение/файл); 2 |
| `sha1sum` | SHA-1, тот же интерфейс и `-c` | 0; 1; 2 |
| `sha224sum` | SHA-224 (SHA-256 с другим IV, 28 байт), тот же интерфейс и `-c` | 0; 1; 2 |
| `sha256sum` | SHA-256, тот же интерфейс и `-c` | 0; 1; 2 |
| `sha3sum` | SHA3-256 (Keccak-f[1600], rate 136, pad `0x06`), `-b`, `-c`, `--status`; файлы/stdin | 0; 1; 2 |
| `sha384sum` | SHA-384 (SHA-512 с другим IV, 48 байт), тот же интерфейс и `-c` | 0; 1; 2 |
| `sha512sum` | SHA-512, тот же интерфейс и `-c` | 0; 1; 2 |
| `b2sum` | BLAKE2b-512 (unkeyed), тот же интерфейс и `-c` | 0; 1; 2 |
| `base64` | encode/decode (`-d`), `-w N` (76; 0 — без переноса) | 0; 1 (ввод); 2 |
| `base58` | Bitcoin base58 encode/decode (`-d`); ведущие нули → `1`; whitespace при decode | 0; 1 (ввод); 2 |
| `base85` | ASCII85 encode/decode (`-d`), `-a` — обернуть в `<~ ~>`; decode принимает оба вида и whitespace | 0; 1 (ввод); 2 |
| `base32` | RFC 4648 base32, те же `-d`/`-w N` | 0; 1 (ввод); 2 |
| `basenc` | base16/base32/base32hex/base64/base64url; `-d`, `-w N`, `-i` | 0; 1 (ввод); 2 |
| `diff` | normal-формат `NaM`/`NdM`/`NcM`, `<`/`>`/`---`; LCS (Hirschberg) | 0 одинаковы; 1 различия; 2 ошибка |
| `cmp` | первый байт: `byte N, line L, offset 0xH`; `-s`, `-l` (octal); EOF-диагностика | 0; 1; 2 |
| `patch` | unified diff `---/+++/@@`, `-p N`, `-o OUT`/`.orig`; контекст без fuzz | 0; 1 (hunk/формат); 2 |
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
| `hex2bin` | hex → binary, пробелы/переводы строк игнорируются, `-o FILE` | 0; 1 (hex/io); 2 |
| `bin2hex` | binary → lowercase hex, `-u`, `-w N` (0 — без переноса) | 0; 1 (io); 2 |
| `uuencode` | classic: `begin 644 NAME`, группы 45, `end`; без `-m` | 0; 1; 2 |
| `uudecode` | classic decode в файл из `begin` или `-o FILE` (`-` = stdout) | 0; 1 (ввод); 2 |
| `curl` | HTTP/1.0 GET по IPv4, `-i`, `-o FILE`; `Connection: close` | 0; 1 (сеть/HTTP); 2 |
| `wget` | тот же GET, файл по basename или `-O FILE`; summary в stderr | 0; 1; 2 |
| `kill` | TERM/KILL/INT/USR1/USR2/STOP/CONT/0 и номера, несколько pid | 0; 1 (в т.ч. ESRCH); 2 |
| `gzip` | gzip-поток: заголовок/CRC32/ISIZE, DEFLATE **stored blocks** (без сжатия); `-d/-c/-k/-f` | 0; 1; 2 |
| `gunzip` | `gzip -d`, отдельное имя | 0; 1; 2 |
| `zcat` | `gzip -dc`: stored-block gzip в stdout, файлы/stdin | 0; 1 (gzip/io); 2 |
| `tar` | ustar: `-c/-x/-t`, `-f`, `-C`, `-v`; файлы/каталоги/симлинки | 0; 1; 2 |
| `untar` | `tar -x` | 0; 1; 2 |
| `timeout` | `-s SIG` (имена/номера, default TERM), `DURATION` (sleep-грамматика); fork+exec, poll `wait4(WNOHANG)` 10 ms; дедлайн → 124 | 0 код ребёнка; 124 таймаут; 127 не найдено; 1/2 |
| `watch` | `-n SEC` (2), `-t` без заголовка, `-c N` (расширение); ANSI-очистка `ESC[2J ESC[H`, fork/exec каждый интервал | 0 (finite) ; 1 ошибка spawn; 2 |
| `xargs` | `-a FILE` (обход stdin без EOF), `-n N`, `-I{}`, `-r`; токены через пробелы, default `echo` | 0; 123 команда упала; 124 не запускается; 2 |
| `parallel` | `-j N` принят, но строго последовательно; как `xargs -n 1`; `-a FILE`, `-r` | 0; 123; 124; 2 |
| `install` | `-m MODE` (per-run), `-D` родители, `-f` перезапись; копия файла + `set_permissions` | 0; 1; 2 |
| `shred` | `-n N`, `-u`, `-z`, `-s SIZE`; детерминированный xorshift (printable), open/write/seek | 0; 1; 2 |
| `command` | без опции — запуск через helper; `-v` путь, `-V` `name is path` | 0/код; 127 не найдено; 1; 2 |
| `type` | тонкий alias `command -V`/`-v`, отдельное имя | 0/1; 2 |
| `time` | fork/exec + `clock_gettime(1)`; `real X.XXXs`, `user/sys 0.000s` (учёта CPU нет) | код ребёнка; 127; 1; 2 |
| `exit` | `exit [N]` (0..255) вызывает `sys::exit`; осмысленно только как entry | N; 2 |
| `nice` | `-n N` принят; приоритетов в ABI нет — no-op; запускает команду | код ребёнка; 127; 2 |
| `more` | `-n LINES` (24), `-d`; `--More--` после каждого чанка, без интерактива/TTY | 0; 1; 2 |
| `nm` | ELF64 LE: `.symtab`/`.dynsym`, `-D`, `-u`, `-n`; `value T name`, без demangle | 0; 1 (ELF/файл); 2 |
| `readelf` | ELF64 LE: `-h` (по умолчанию), `-S` секции, `-s` символы (общий парсер с `nm`), `-l` program headers | 0; 1 (ELF/файл); 2 |
| `uuidgen` | RFC-4122 v4-shaped из xorshift64* (seed: `clock_gettime(1)` + pid), `-r`, `-n N`; не CSPRNG | 0; 1 (запись); 2 |


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
`repo/pkg/paghbox-1.8.0.ppkg`, а `python3 tools/build_repo.py --check` зелёный.
Пакет `paghbox 1.8.0` содержит 120 команд; его размер и SHA-256 приведены в
разделе [`repo/`](#repo-и-воспроизводимость) ниже. `MAX_COMMANDS = 160` в
установщике 0.16.1 с запасом покрывает 120.

Живой прогон сделан на **ядре 0.16.1** (`/home/oleg/pagh-riscv/target/.../debug/pagh`,
native ABI + pkg installer), QEMU `virt`, 256 MiB, диск 64 MiB ext2
(`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`). Драйвер —
[`tools/live_test_180.py`](tools/live_test_180.py): он собирает образ, копирует
`paghbox` и фикстуры (SHA3-векторы, base85-строки, синтетический ELF) и делает
**13 загрузок по 2–3 команды** (короткие группы обязательны: serial-драйвер
теряет длинные строки вывода), проверяя вывод, коды и, для UUID, регулярное
выражение. Результат: **36 PASS, 0 FAIL**, все 6 новых applet'ов проверены
функционально; история батчей 8 и 7 остаётся в `docs/live-testing.md`.

Сырые транскрипты:
[`docs/live-1.8.0-transcript.txt`](docs/live-1.8.0-transcript.txt) (13 загрузок),
[`docs/live-1.8.0-pkg-install.txt`](docs/live-1.8.0-pkg-install.txt)
(`pkg install` 1.8.0, 120 симлинков); история 1.7.0:
[`docs/live-1.7.0-transcript.txt`](docs/live-1.7.0-transcript.txt) (14 загрузок);
история 1.6.0:
[`docs/live-1.6.0-transcript.txt`](docs/live-1.6.0-transcript.txt) (23 загрузки),
[`docs/live-1.6.0-pkg-install.txt`](docs/live-1.6.0-pkg-install.txt)
(`pkg install` и симлинки); история батча 6:
[`docs/live-1.5.0-transcript.txt`](docs/live-1.5.0-transcript.txt) (34 загрузки),
[`docs/live-1.5.0-pkg-install.txt`](docs/live-1.5.0-pkg-install.txt);
история батча 5:
[`docs/live-1.4.0-transcript.txt`](docs/live-1.4.0-transcript.txt),
[`docs/live-1.4.0-pkg-install.txt`](docs/live-1.4.0-pkg-install.txt).
PASS-строки и таблица: [`docs/live-testing.md`](docs/live-testing.md).

| Проверка | Результат |
|---|---|
| хэш-векторы | `md5sum`/`sha1sum`/`sha224sum`/`sha256sum`/`sha384sum`/`sha512sum`/`b2sum` для `empty`/`abc`/`fox` совпадают с эталонами; `-b` → `hex *name`; `-c` → `OK`, неверная сумма → `FAILED`, `--status` без вывода |
| `sha224sum`/`sha384sum` | FIPS 180-4: `d14a…`/`2309…`/`730e…` и `38b0…`/`cb00…`/`ca73…` для `empty`/`abc`/`fox`; `-b`, `-c`, `--status`; тот же SHA-256/SHA-512, но с IV стандарта |
| `crc32` | `123456789` → `cbf43926 9 /mnt/numbers.txt`; `abc` → `352441c2 3`; пусто → `00000000 0`; `-c` `OK`/`FAILED`, кривая строка → 1 |
| `base58` | `hello\n` → `tzCkV5DK`; `00 01` → `12`; decode `hello`/`abc`; `0OIl` → `invalid input` (1); лишний операнд → 2 |
| `hex2bin`/`bin2hex` | `61 62\n63\t0a` → `decoded.bin` → `6162630a`; `-u` → `68656C6C6F0A`; `-w 4` → `6865`/`6c6c`/`6f0a`; неверный hex → 1, плохая ширина → 2 |
| `zcat` | `stored.gz` → `zcat stored payload`; concatenated members; `compressed.gz` → `compressed DEFLATE blocks are not supported` (1); `bad.gz` → `bad gzip magic` (1); `-c` → 2 |
| `sha3sum` | FIPS 202 SHA3-256: `empty` → `a7ff…434a`, `abc` → `3a98…1532`, `fox` → `6907…fc04`; `-b` → `hex *name`; `-c` `OK`/`FAILED`, `--status` без вывода, `-z` → 2; границы rate 135/136/137/272/273 сверены с `hashlib` |
| `calc` | `2+3*4` → `14`, `(2+3)*4` → `20`, `-5+2` → `-3`; два выражения — две строки, `-e 10%3` → `1`; `1/0`, `7%0` → 1; `2+` → 2 |
| `base85` | `hello\n` → `BOu!rD[H`, `00 00 00 00` → `z`; `-a abc` → `<~@:E^~>`; decode обоих видов → `hello`; `<~v~>` → `invalid input` (1) |
| `readelf` | синтетический ELF64 RISC-V: `-h` (EXEC/RISC-V/entry/shoff/shnum), `-S` 8 секций, `-s` те же 3 строки, что `nm`; `-l` → `Program headers: none`; не-ELF → 1, опции/операнды → 2 |
| `uuidgen` | UUID версии 4 и варианта `[89ab]`; `-n 3` — три строки, `-n x` → 2; seed `clock_gettime(1)`+pid (не CSPRNG) |
| `paghfetch` | реальные `sysname`/`nodename`/`release`/`machine`/`uptime`/`nproc=1`/PWD, рамка из `sysname`; лишний операнд → 2 |
| `basenc` | base64 `aGVsbG8K`, base32 `NBSWY3DPBI======`, base32hex `D1IMOR3F18======`, base16 `68656C6C6F0A`; decode всех пяти; `-w 4` → `aGVs`/`bG8K`; base64url с `-`/`_` |
| `gzip`/`gunzip` | `gzip -k` → `gunzip -f` → исходный текст; host stored-block `.gz` прочитан; guest `.gz` распакован Python `gzip.decompress` в `hello\n` |
| `tar`/`untar` | create/list/extract `tree/` (5 записей), `cat tree/a.txt` → `tree file`, symlink → `a.txt`; host ustar с prefix-именем >100 символов прочитан; guest tar проверен Python `tarfile` (включая long name) |
| `patch` | in-place + `.orig`, два hunks, `-p1`, `-o`, новый файл из `/dev/null`, `\ No newline` (проверено `xxd`), неверный контекст → exit 1 |
| `cal` | `2 2024`, `1 1970`, `9 1752`, `-y 2024`, `cal`, `cal 2024` сверены с моделью Python `calendar` (7 колонок, воскресенье первым) |
| usage/ошибки | неизвестная опция/лишний аргумент → 2; отсутствующий файл, неверная сумма, сломанный hunk/архив → 1 |
| `timeout` | `1 sleep 0.2` → 0; `0.1 sleep 2` → 124 (TERM/KILL/9); нет команды → 127, плохая длительность/сигнал → 2 |
| `xargs`/`parallel` | `-n 2` → две строки; `-I{}` → 4 подстановки; default echo; пустой `-r` → 0; нет команды → 124; `false` → 123; `parallel -j 2` последователен |
| `install` | `-D -m 600` создаёт `/mnt/inst/new/copy.txt`, `cat` → `install me`; повтор без `-f` → 1 (`File exists`), `-f` → 0 |
| `shred` | `-n 1 -s 10` → printable-паттерн в `cat`; `-u` → файла нет (exit 1 у `cat`); `-z` → `xxd` показывает 10 нулей + `0a` |
| `command`/`type` | `-v sleep` → `/mnt/bin/sleep`; `-V echo` → `echo is /mnt/bin/echo`; запуск `echo direct`; missing → 127 (запуск) / 1 (`-v`) |
| `time`/`nice` | `time echo` → `timed` + `real X.XXXs`, `user/sys 0.000s`, код 0; `time false` → 1; `nice -n 5` → `nice`, без приоритетов |
| `exit`/`more`/`nm` | `exit 7` → 7, `exit` → 0, `exit 300` → 2; `more -n 3` и `-d` — точные чанки; `nm`, `-D`, `-u`, `-n` — точные строки, не-ELF → 1 |
| `watch` | `-t -c 2 -n 0` → две ANSI-очистки + `hi`; с заголовком `Every 0s: /mnt/bin/echo hi` |
| `pkg install` | `pkg: installed paghbox 1.6.0`; 107 симлинков; `/mnt/bin/timeout` и `/mnt/bin/type` работают |

В 1.6.0 дополнительно был проверен `pkg install paghbox` на чистом 64 MiB
ext2 с `repo/` на хосте: `pkg: installed paghbox 1.6.0`, затем
`run /mnt/bin/timeout 1 /mnt/bin/echo installed` → `installed` и
`run /mnt/bin/type -v sleep` → `/mnt/bin/sleep`; `debugfs` показывает
`paghbox` (818936 байт) и 107 симлинков в `/mnt/bin`. Для 1.7.0 отдельный
`pkg install` не перепроверялся; `build_repo.py --check` подтверждает пакет,
114 команд и согласованность манифеста, а `MAX_COMMANDS = 160` покрывает 114.

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
* **`gzip`/`gunzip`/`zcat`:** только DEFLATE stored blocks (BTYPE=00) — поток
  валиден, CRC-32 и ISIZE корректны, но данные не сжимаются; декодер отклоняет
  BTYPE 1/2, `-t`/`-l`/`-r` нет, MTIME=0, уровни `-1..-9` принимаются без
  эффекта. `zcat` — тот же декодер с выводом в stdout; суффикс `.gz` не
  требуется, поддерживаются concatenated members и нулевой padding.
* **`tar`/`untar`:** только ustar (без GNU longname/PAX, hardlinks, sparse и
  сжатия); имя 100 + prefix 155 байт, size < 8 GiB; mtime из boot-часов; права
  не восстанавливаются. При извлечении симлинк с абсолютной целью или `..`
  отвергается, а `mkdirat('/mnt')` в 0.16.1 отвечает ENOTSUP, поэтому каталоги
  создаются через stat-before-mkdir (см. `tar.rs`). Архив читается целиком.
* **`patch`:** только unified diff на один файл; строгий контекст без fuzz и
  поиска смещения, по умолчанию `-p0`; `.orig` перезаписывается; git-бинарные
  патчи, context diff и переименования не поддерживаются.
* **`basenc`:** base16/base32/base32hex/base64/base64url; нет base2/base8/
  zbase32; decode принимает unpadded, но требует нулевые хвостовые биты.
* **`md5sum` и остальные `*sum`:** файл читается целиком в память; MD5/SHA-1 —
  только для совместимости; SHA-224/SHA-384 переиспользуют SHA-256/SHA-512 с
  другим IV и усечением; BLAKE2b без ключа; check-формат coreutils
  (`hex  name`, `hex *name`), без `--tag` и «умного» поиска файла.
* **`crc32`:** это IEEE/zlib CRC-32 (reflected `0xEDB88320`), а не POSIX
  `cksum`; CRC печатается восемью hex-цифрами (`cbf43926`), затем размер и имя.
  `-c` понимает только этот формат (`crc size name`), а не вывод `cksum`.
* **`base58`:** Bitcoin-алфавит без checksum (base58check не реализован);
  decode игнорирует ASCII whitespace, но любой другой не-алфавитный символ —
  ошибка. Ведущие нулевые байты кодируются как `1`.
* **`hex2bin`/`bin2hex`:** whitespace в hex-входе игнорируется, поэтому
  round-trip не зависит от `-w`; `-w N` у `bin2hex` считает hex-символы, а не
  байты. У `bin2hex` нет `-o`, у `hex2bin` нет выбора uppercase (вход
  принимает оба регистра).
* **`cal`:** пролептический григорианский календарь, UTC, английские имена;
  «сегодня» — из epoch-часов ядра (RTC нет), поэтому после загрузки это
  1970-01-01 + uptime; поддерживаются годы 1..9999.
* **Процессы (батч 7):** `spawn_wait` делает fork/execve/wait4; поиск команды
  фиксирован (/mnt/bin → PWD → литеральный путь). `timeout` на дедлайне
  возвращает 124, но **не может убить ребёнка**: `kill(pid)` в 0.16.1 трактует
  положительный pid как thread id собственного run, а чужой процесс даёт
  ESRCH (попытка kill(child_pid) наблюдаемо убивала сам timeout). Живой
  ребёнок остаётся, и его снимает `cleanup_orphans` раннера после выхода
  foreground-процесса. `-s SIG` парсится и валидируется, но эффекта не имеет.
* **`watch`/`more` и TTY:** `isatty` в ABI нет, поэтому `watch` всегда пишет
  ANSI `ESC[2J ESC[H` в fd 1, а `more` не ждёт клавиш и печатает `--More--`
  как маркер. `-c N` у `watch` — расширение для тестируемости; Ctrl-C с
  консоли завершает процесс.
* **`xargs`/`parallel`:** у консольного fd 0 нет EOF, поэтому основной рабочий
  вход — `-a FILE` (расширение); stdin-режим ждёт вечно. Вход делится по
  ASCII-пробелам без кавычек; `-I{}` подставляет токен в аргументы (не в имя
  команды). `parallel` всегда последователен, `-j N` только принимается.
* **`nice`** — no-op: в ABI нет `setpriority`/`getpriority`; `-n N`
  валидируется, но не влияет на планировщик. **`time`** печатает честные
  `user/sys 0.000s`, потому что per-process CPU accounting в ABI отсутствует;
  `real` измерен по `clock_gettime(1)`.
* **`install`:** `-m` — per-run override (`fchmodat` не пишет inode, см.
  `fs::set_permissions`); без `-f` существующий файл — ошибка, с `-f` —
  перезапись. `-D` использует stat-before-mkdir helper: `fs::create_dir_all`
  спотыкается о синтетический `/mnt` (mkdirat(`/mnt`) → ENOTSUP).
* **`shred`:** best-effort, не secure erase: ABI без `fsync`, ext2-кэш и
  переназначение секторов не контролируются; паттерн детерминированный
  xorshift в printable ASCII, `-s` задаёт число байт (не усекает длинный
  файл), `-u` удаляет после прохода.
* **`nm`/`readelf`:** только ELF64 little-endian; `nm` предпочитает
  `.symtab`, без demangle/`-S`/`-l`; битый ELF — 1. `readelf` — компактное
  подмножество GNU-вывода, `-S`/`-s`/`-l` требуют таблицу секций, DWARF не
  читается (поэтому `addr2line` в наборе нет). `-s` и `nm` используют один
  парсер `elf.rs`, поэтому расхождений в символах и буквах типов нет.
* **`sha3sum`:** один SHA3-256; SHA3-224/384/512, SHAKE и cSHAKE не
  реализованы. Файл читается целиком (та же политика памяти, что у
  `sha256sum`), режимы `-b`/`-c`/`--status` общие с остальными `*sum`.
* **`calc`:** только `i64` и целые литералы — без плавающей точки, переменных
  и функций. Деление/остаток на ноль и overflow — exit 1, синтаксис — 2;
  вложенность скобок ограничена 64 уровнями. Каждый операнд — отдельное
  выражение (`-e` — то же самое), поэтому `calc 2 + 2` не склеивается.
* **`base85`:** без переносов строк; `-a` добавляет Adobe-делимитеры,
  decode принимает и их, и «голый» текст. Это не `btoa`-совместимый
  checksum-формат: кодируются/декодируются только данные.
* **`uuidgen`:** **не** источник энтропии. xorshift64* засевается
  `clock_gettime(1)` и pid, поэтому UUID детерминированы для одного seed,
  два процесса в одном тике могут совпасть; криптостойкость и уникальность
  не гарантированы (в ABI нет `getrandom`).
* **`paghfetch`:** показывает только то, что есть в ABI: `uname`
  (sysname/nodename/release/machine), uptime, `nproc=1` и PWD. Названия
  дистрибутива, модели CPU, объёма RAM и числа пакетов не выдумываются;
  `release` — поле uname (0.10.0), а не версия сборки ядра (0.16.1).
* **`exit`** вызывает `sys::exit`, то есть завершает весь `paghbox`; это
  осмысленно только как entry point. `command`/`type` не знают builtins —
  только файлы в фиксированном поиске.
* **Ядро 0.16.1** уже даёт fork/exec/wait, файловые мутации, mmap, pipes, poll,
  sockets, threads и signals (52 syscall'а), но `paghbox` остаётся набором
  CLI-фильтров: шелл ядра не создаёт конвейеров, а `pkg` ставит один bundle.
  Соседние репозитории (`pagh-riscv`, `pagh-api`) не изменялись.

## `repo/` и воспроизводимость

`target/` и `repo/` в [`.gitignore`](.gitignore): бинарники не коммитятся.
`repo/` полностью воспроизводится командой `python3 tools/build_repo.py` из
исходников и пинованного тулчейна; индекс детерминирован по содержимому
(единственный недетерминизм — сам ELF от rustc). Текущий пакет — 843944
байта payload / 844839 байт `.ppkg`, 114 команд, SHA-256
`54bda1f816c3fdd4a0c82f0e9a77e5d3ee4b54146270dd08e961dc0642f24082`; в git
он не попал (`.gitignore`), его место — на HTTP-зеркале.

`catalog.md` — ровно 120 уникальных имён; `[x]` стоят у 114 реализованных,
остальные 6 `[ ]`. Список проверяется `tools/live_test_170.py` и `grep`-ом по
`NAMES`: все 114 имён входят в `commands=` пакета.