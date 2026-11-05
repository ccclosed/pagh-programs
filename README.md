# pagh-programs — портированные программы для pagh-riscv

Репозиторий портов: программы написаны **под нативный API `libpagh`**
(соседний checkout `../pagh-api`), а не под Linux-совместимый слой. Один
busybox-style ELF `paghbox` содержит 37 applet'ов; `tools/build_repo.py`
упаковывает его в `.ppkg` и пишет `repo/index`. Установщик `pkg` в ядре
pagh-riscv — потребитель этого репозитория: скачивает пакет по HTTP, сверяет
SHA-256/размер, ставит ELF в `/mnt/bin` и создаёт симлинки на команды.

## Структура

```
catalog.md                 целевые 120 программ и честный статус (37 [x])
docs/package-format.md     формат .ppkg и index (контракт с ядром)
programs/                  crate paghbox: dispatch + applets/
  src/main.rs              argv[0]/`paghbox <applet>` + exit codes
  src/util.rs              EAGAIN-retry чтение, Input, вывод
  src/applets/             37 модулей: echo true false yes pwd basename dirname seq
                           cat wc head tail ls stat uname hostname arch nproc sleep
                           uptime date readlink realpath uniq sort nl fold expand
                           unexpand rev cut tr comm paste strings hexdump grep
tools/build_repo.py        cargo build -> .ppkg -> index, и --check
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
`repo/pkg/paghbox-1.1.0.ppkg` и `repo/index`. `--elf PATH` упаковывает готовый
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

В `index` поле `size` — длина **payload'а** (ELF), а не всего `.ppkg`: в
манифесте `sha256` определён как digest payload'а, и контракт говорит
«проверить payload против SHA-256 и size из index». Полная длина файла
выводится из заголовка: `16 + manifest_len + payload_len`. Это единственная
неоднозначность формата; она продублирована в отчёте Lead'у. `--check`
проверяет именно такую трактовку.

## Applet'ы (37)

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
классы, `grep` без alternation/групп/ERE, `hexdump` только `-C`-вид. В
user-mode не включён FPU, поэтому `sort -n` не использует плавающую точку:
сравниваются десятичные знаки точно (без экспоненты и `inf`). Форматирование
`wc` — политика порта, а не ABI.

## Статус и тестирование

Сборка проверена: `cargo build --release --target riscv64gc-unknown-none-elf`
даёт статический RV64 ELF с entry `0x10000` и тремя PT_LOAD (R+X / R / R+W),
как требует `pagh-user.ld`; `readelf` подтверждает раскладку, `llvm-objdump -d`
не находит ни одной FP-инструкции (ядро не включает FPU в U-mode, поэтому
`sort -n` сравнивает десятичные точно, без плавающей точки). Пакет
`paghbox 1.1.0` собран, `python3 tools/build_repo.py --check` зелёный.

Живой прогон сделан на **ядре 0.10.1** (`pagh-riscv/target/.../debug/pagh`,
native ABI + pkg installer), QEMU `virt`, 256 MiB. Диск 64 MiB собран на хосте
`mke2fs -q -t ext2 -F -b 4096 -d rootfs disk.img 64M`; в корне образа —
`paghbox` 1.1.0 и фикстуры (файлы, подкаталоги, симлинк, двоичные данные).
Драйвер — Python + Unix-сокет QEMU; одна загрузка, 81 команда
`run /mnt/paghbox ...` подряд, после каждой проверялись вывод и
`run: /mnt/paghbox exited with code N`.

**81 PASS, 0 FAIL.** 24 из 25 новых applet'ов проверены функционально;
`tr` по спецификации читает stdin, а у консольного fd 0 нет EOF и нет
пайпов/редиректа, поэтому для него живым запуском проверен только разбор
аргументов (четыре случая exit 2). Полный список PASS-строк и сырой транскрипт:
[`docs/live-testing.md`](docs/live-testing.md),
[`docs/live-1.1.0-transcript.txt`](docs/live-1.1.0-transcript.txt).

| Проверка | Результат |
|---|---|
| `uname -a` | `pagh pagh-riscv 0.10.0 0.10.1 riscv64` |
| `ls -l /mnt` | `drwxr-xr-x 4096 dir1/`, `-rw-r--r-- 35 hello.txt`, `-rwxr-xr-x 298288 paghbox` |
| `stat /mnt/hello.txt` | `/mnt/hello.txt: file, size=35, mode=0o644, ino=23` |
| `readlink /mnt/link` | `hello.txt`; не-симлинк — exit 1 |
| `realpath /mnt/dir1/../hello.txt` | `/mnt/hello.txt` |
| `uniq`, `uniq -c`, `-d`, `-u` | `apple banana cherry date`; counts `2/1/3/1` |
| `sort`, `sort -r`, `sort -n`, `sort -n -r`, `sort -u` | точные последовательности, включая `-3 -0.5 0 02 2 3.5 7 10` |
| `nl`, `nl -b a` | номера 1..3 / 1..6 с табами |
| `fold -w 4`, `expand -t 4`, `unexpand`, `unexpand -a` | переносы/табы совпали побайтно |
| `rev`, `cut -d : -f ...`, `comm`, `paste` | точные последовательности; `comm` с позиционными табами |
| `strings -n 4/6`, `hexdump -C` | `hello`, `world!`, `longstring1234`; `|Hello, world!.|` |
| `grep` (12 сценариев) | литерал, `-i/-v/-n/-c/-l/-F`, `^ $ . * [] [^]`, exit 0/1/2 |
| ошибки/usage | `ls /mnt/missing` exit 1; `grep [` exit 2; `fold -w 0`, `cut` без `-f`, `sleep nope` exit 2 |
| `sleep 0.2`, `sleep 1.5` | exit 0 (nanosleep, округление до тика) |

Несколько `run` на одну загрузку работают. Прежние замечания README о зависании
ext2 после 48 KiB и о падении шелла после первой user-программы относились к
старому ядру и на 0.10.1 не воспроизвелись: ELF на 298 288 байт с `.rodata` по
файловому смещению `0x16000` загружается и все 81 команды проходят. Smoke-тест:

```sh
run /mnt/paghbox echo hello
run /mnt/paghbox ls -l /mnt
run /mnt/paghbox wc /mnt/hello.txt
```

## Ограничения ABI для следующих партий

Эти 25 applet'ов намеренно read-only. ABI (`pagh-api/docs/abi.md`) даёт только
`read`, `write`, `openat`, `close`, `lseek`, `fstat`, `getdents64`, `brk`,
`clock_gettime`, `nanosleep`, `uname`, `readlinkat`, `getpid`, `sched_yield` и
`exit`. Нет `unlinkat`, `renameat`, `mkdirat`, `symlinkat`, `linkat`,
`ftruncate`, `fchmodat`, `statfs`; нет `poll`, `pipe`, `dup`, `exec`. Поэтому
`rm`, `mv` (rename), `mkdir`, `ln`, `chmod`, `truncate`, `df`, `find -delete`
нельзя честно реализовать в userland до расширения ABI. Шелловые `mkdir`, `rm`,
`cp` в ядре — внутренние команды, а не доступные программам syscall'ы.
`cp`/`tee`/`split`/`touch` (create) технически возможны на `openat(O_CREAT)` уже
сейчас, но выходят за рамки этой партии.

Установщик `pkg` в 0.10.1 принимает не больше 32 команд на `applet-bundle`
(`MAX_COMMANDS` в `src/pkg/mod.rs`), поэтому `pkg install paghbox` для
37-командного пакета скачивает и проверяет его, но затем печатает
`pkg: install: package manifest is invalid` и ничего не устанавливает. Прямой
`run /mnt/paghbox ...` работает. Чтобы установка через `pkg` заработала, нужно
поднять лимит в ядре или разделить пакет — это решение вне этого репозитория;
формат `.ppkg` такого ограничения не содержит.

## `repo/` и воспроизводимость

`target/` и `repo/` в [`.gitignore`](.gitignore): бинарники не коммитятся.
`repo/` полностью воспроизводится командой `python3 tools/build_repo.py` из
исходников и пинованного тулчейна; индекс детерминирован по содержимому
(единственный недетерминизм — сам ELF от rustc). Текущий пакет — 298 288 байт
payload / 298 732 байт `.ppkg` (около 292 KiB), то есть меньше 2 MB, но в git
он всё равно не попал: его место — на HTTP-зеркале, а не в истории.

`catalog.md` — ровно 120 уникальных имён; `[x]` стоят у 37 реализованных,
остальные `[ ]`. Дубликаты (`printf`, `awk`, `base64`, `free`)
и не-команды (`tail -f`, `yes | head`, `login`, `sudo`) заменены на реальные
CLI-имена (`factor`, `nm`, `numfmt`, `cal`, `more`, `shuf`, `addr2line`,
`arch`).