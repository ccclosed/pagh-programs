# pagh-programs — портированные программы для pagh-riscv

Репозиторий портов: программы написаны **под нативный API `libpagh`**
(соседний checkout `../pagh-api`), а не под Linux-совместимый слой. Один
busybox-style ELF `paghbox` содержит 12 applet'ов; `tools/build_repo.py`
упаковывает его в `.ppkg` и пишет `repo/index`. Установщик `pkg` в ядре
pagh-riscv — потребитель этого репозитория: скачивает пакет по HTTP, сверяет
SHA-256/размер, ставит ELF в `/mnt/bin` и создаёт симлинки на команды.

## Структура

```
catalog.md                 целевые 120 программ и честный статус (12 [x])
docs/package-format.md     формат .ppkg и index (контракт с ядром)
programs/                  crate paghbox: dispatch + applets/
  src/main.rs              argv[0]/`paghbox <applet>` + exit codes
  src/util.rs              EAGAIN-retry чтение, Input, вывод
  src/applets/             echo true false yes pwd basename dirname seq cat wc head tail
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
`repo/pkg/paghbox-1.0.0.ppkg` и `repo/index`. `--elf PATH` упаковывает готовый
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

## Applet'ы (12)

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

`cat/wc/head/tail` читают через `libpagh`: `read` на fd файла или 0, а на
`EAGAIN` (консольный ввод без байта) делают `sched_yield` и повторяют. `0`
байт = EOF. У консольного fd 0 в текущем ядре EOF не наступает, поэтому
`cat` без файлов ждёт ввод вечно — это свойство ядра, записанное в отчёт.

Не поддерживается намеренно: пайпы и редиректы (их нет в шелле ядра),
отрицательные `head -n -N`, `tail -f`, `wc -m`, суффиксы/опции GNU сверх
перечисленных. Форматирование `wc` — политика порта, а не ABI.

## Статус и тестирование

Компиляция проверена: `cargo build --release --target
riscv64gc-unknown-none-elf` даёт статический RV64 ELF с entry `0x10000` и
тремя PT_LOAD (R+X / R / R+W), как требует `pagh-user.ld`; `readelf` и `nm`
подтверждают `_start`/`main`. Пакет и `--check` проверены.

Живой QEMU-прогон делался на **копии** ветки `native-abi` ядра
(`1ce872f` + незакоммиченные правки; оригинал не изменялся). На этой копии
нашлись баги ядра, а не порта:

* коммитнутая раскладка кладёт `.rodata` на страницу (`p_offset` 0xa000,
  конец 0xc91c). Чтение ext2 за 12-м прямым блоком (файловое смещение ≥ 48 KiB)
  в этом ядре **зависает без исключения**, поэтому `paghbox` с коммитнутой
  раскладкой не загружается; файл на 221 KiB с маленькими PT_LOAD грузится;
* оболочка после завершения user-программы получает store page fault
  (`stval=0x201000000`, `Arc::clone`) и останавливается — даже если программа
  вызвала только `getpid`. Поэтому один `run` на загрузку;
* часть загрузок падает в `mmu.rs:206` (misaligned `read_volatile`) или в
  загрузчике ELF (`0x1a000 is not mapped`); повторный запуск проходит.

Чтобы проверить сами applet'ы, собран **тестовый, не закоммиченный** вариант с
`.rodata` сразу после `.text` (два PT_LOAD, оба файловых смещения < 48 KiB; тот
же исходник). На нём все 12 applet'ов дали корректный вывод/код:

| Проверка | Результат |
|---|---|
| `echo hello world`, `echo -n`, симлинк `/mnt/echo` | корректно |
| `true` / `false` | exit 0 / exit 1 |
| `pwd` | `/` |
| `basename /a/b/c.txt .txt`, `basename /` | `c`, `/` |
| `dirname /a/b/c.txt` | `/a/b` |
| `seq 1 5`, `seq 3 -1 1` | 1..5, 3 2 1 |
| `cat` одного/двух файлов, отсутствующего файла | вывод; `cat: …: No such file…`, exit 1 |
| `wc /mnt/hello.txt`, `wc -l /mnt/twenty.txt` | `3 6 35 …`, `20 …` |
| `head -n 2`, `tail -n 2`, `head` двух файлов с баннерами | корректно |
| `yes` | 596 строк `y` до таймаута |
| неизвестный applet / без аргументов | 127 / usage, exit 1 |

Первый автоматический прогон (20 команд, по одной на загрузку) дал 18/20;
два «провала» (`basename` и `wc -l`) были зависаниями/паниками ядра и прошли на
повторе. Итог: **applet-логика проверена вживую, коммитнутая раскладка ждёт
исправления ext2-чтения**. Smoke-тест после фикса ядра:

```sh
run /mnt/bin/paghbox echo hello
run /mnt/bin/paghbox seq 1 3
run /mnt/bin/paghbox wc /mnt/hello.txt
```

## `repo/` и воспроизводимость

`target/` и `repo/` в [`.gitignore`](.gitignore): бинарники не коммитятся.
`repo/` полностью воспроизводится командой `python3 tools/build_repo.py` из
исходников и пинованного тулчейна; индекс детерминирован по содержимому
(единственный недетерминизм — сам ELF от rustc). Текущий пакет — около 185 KiB
payload / 186 KiB `.ppkg`, то есть меньше 2 MB, но в git он всё равно не
попал: его место — на HTTP-зеркале, а не в истории.

`catalog.md` — ровно 120 уникальных имён; `[x]` стоят только у 12
реализованных, остальные `[ ]`. Дубликаты (`printf`, `awk`, `base64`, `free`)
и не-команды (`tail -f`, `yes | head`, `login`, `sudo`) заменены на реальные
CLI-имена (`factor`, `nm`, `numfmt`, `cal`, `more`, `shuf`, `addr2line`,
`arch`).