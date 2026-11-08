# Каталог: 120 программ + threads

Цель — 120 applet'ов плюс `threads`-демо, запускаемых на pagh-riscv через `libpagh`/`paghbox`.
Статус: `[x]` реализовано и собирается в составе `paghbox`, `[~]` в работе,
`[ ]` запланировано. Ядро — POSIX-подобные CLI-утилиты; «портировать» =
реализовать семантику под `libpagh` (не тащить GNU/Linux-исходники).

> Статус: 120 целевых + `threads`, все отмечены `[x]`; собираются в `paghbox` 1.9.0.
> Батч 9 (6 утилит: sha3sum, calc, base85, readelf, uuidgen, paghfetch)
> проверен живым запуском в QEMU на ядре 0.16.1: SHA3-256 сверен с
> векторами FIPS 202, `-b`/`-c`/`--status` — по check-файлам; calc —
> приоритеты, скобки, унарный минус и коды 1/2; base85 — round-trip в
> обычном и Adobe-виде; readelf — заголовок/секции/символы на синтетическом
> ELF64 RISC-V; uuidgen — v4-формат и `-n`; paghfetch — реальные uname,
> uptime, nproc=1 и PWD. Список — 120 целевых имён + `threads`, все 121 входят
> в `commands=` пакета.
>
> Ранее: батч 8 (7 утилит: sha224sum, sha384sum, crc32, base58, hex2bin,
> bin2hex, zcat) — 42 PASS на 14 загрузках; SHA-224/SHA-384 сверены с
> эталонами, CRC-32 — по контрольному значению `cbf43926`,
> base58/hex2bin/bin2hex — round-trip, zcat — на host stored-block `.gz`.
> `sha224sum`/`sha384sum` переиспользуют существующие SHA-256/SHA-512 с
> другим IV и усечением; `zcat` — тот же хранимый декодер, что у `gunzip`.
>
> Ранее: батч 7 (13 утилит: timeout, watch, xargs, parallel, install, shred,
> command, type, time, exit, nice, more, nm) — 67 PASS на 23 загрузках, 107
> реализованных. Процессные команды используют общий `spawn_wait`
> (fork/execve/wait4) и фиксированный поиск `/mnt/bin` → `PWD`; `timeout`
> возвращает 124 на дедлайне (ядро 0.16.1 не умеет слать сигнал чужому
> процессу), `parallel` сериализован, `nice` — no-op, у `watch`/`more` нет
> TTY-детекта, stdin без EOF. Ограничения записаны в README.

## Текстовый вывод и идентичность (1–20)

- [x] echo — напечатать аргументы (`-n`, `-e`, `-E`)
- [x] printf — форматированный вывод
- [x] true — exit 0
- [x] false — exit 1
- [x] yes — повторять строку
- [x] pwd — напечатать PWD
- [x] whoami — напечатать пользователя (всегда root: uid syscall в ABI нет)
- [x] id — uid/gid (всегда 0: uid/gid syscall в ABI нет; `-u/-g/-G`)
- [x] uname — sysname/nodename/release/machine
- [x] hostname — nodename
- [x] date — время из clock_gettime
- [x] seq — последовательность чисел (целые, `-s SEP`)
- [x] expr — арифметические выражения
- [x] test — проверки файлов/строк
- [x] basename — имя файла из пути
- [x] dirname — каталог из пути
- [x] realpath — нормализованный абсолютный путь
- [x] readlink — цель симлинка
- [x] tee — разветвить stdin/stdout/файл (`-a`, `-i FILE` из-за отсутствия пайпов)
- [x] env — напечатать окружение (`PWD=...` из envp; без exec-режима)

## Файлы (21–45)

- [x] ls — список каталога
- [x] cat — вывести файлы
- [x] head — первые строки
- [x] tail — последние строки
- [x] wc — строки/слова/байты
- [x] cp — копировать
- [x] mv — переместить/переименовать
- [x] rm — удалить
- [x] mkdir — создать каталог
- [x] rmdir — удалить пустой каталог, `-p` для родителей
- [x] touch — создать/обновить файл
- [x] ln — жёсткая/символическая ссылка
- [x] stat — метаданные файла
- [x] du — размер дерева
- [x] sha224sum — SHA-224 (SHA-256 с другим IV), тот же интерфейс и `-c`
- [x] find — обход дерева по предикатам
- [x] which — найти исполняемый файл в PATH
- [x] file — тип файла по содержимому
- [x] truncate — изменить размер
- [x] split — разделить файл на части (`-l N`, `-b N`, `-d`, PREFIX)
- [x] csplit — разделить по номеру строки/`/RE/` (`-f`, `-n`, `-s`)
- [x] join — соединение строк по ключу (`-1`, `-2`, `-t`)
- [x] paste — склеить строки
- [x] comm — общие/различные строки
- [x] tac — обратный порядок строк

## Текст и поиск (46–75)

- [x] grep — поиск строки/шаблона
- [x] sed — подмножество (`s///`, `p`, `d`, адреса `N`/`N,M`)
- [x] awk — подмножество (поля, `$N`, `NR`, `NF`, `/re/`, сравнения)
- [x] cut — выбор колонок
- [x] tr — замена/удаление символов
- [x] sort — сортировка строк
- [x] uniq — уникальные строки
- [x] nl — нумерация строк
- [x] fold — перенос строк
- [x] fmt — переформатирование абзаца (`-w N`, `-s`)
- [x] expand — табы в пробелы
- [x] unexpand — пробелы в табы
- [x] rev — обратный порядок символов
- [x] od — дамп байтов (`-A x|o|d`, `-t x1|o1|c`)
- [x] hexdump — hex-дамп
- [x] sum — BSD/SysV контрольная сумма (`-s` — SysV)
- [x] cksum — POSIX CRC-32 и размер
- [x] md5sum — MD5 файлов/stdin, `-c` (проверка по `md5sum`-файлу)
- [x] sha1sum — SHA-1, тот же интерфейс и `-c`
- [x] sha256sum — SHA-256, тот же интерфейс и `-c`
- [x] sha512sum — SHA-512, тот же интерфейс и `-c`
- [x] b2sum — BLAKE2b-512 (unkeyed), тот же интерфейс и `-c`
- [x] base32 — base32 encode/decode (`-d`, `-w N`)
- [x] base64 — base64 encode/decode (`-d`, `-w N`)
- [x] basenc — base16/base32/base32hex/base64/base64url, `-d`, `-w N`
- [x] uudecode — декодировать classic uuencode (`-o FILE`)
- [x] uuencode — classic uuencode (mode 644, имена, группы по 45)
- [x] strings — печатные строки из файла
- [x] diff — normal-формат (`NcN`, `<`/`>`)
- [x] cmp — побайтовое сравнение (`-s`, `-l`)
- [x] patch — unified diff (`---/+++/@@`), `-p N`, `-o`, `.orig`-бэкап

## Система и процессы (76–100)

- [x] sha384sum — SHA-384 (SHA-512 с другим IV), тот же интерфейс и `-c`
- [x] sleep — пауза
- [x] timeout — таймаут команды (`-s SIG`, `DURATION`; exit 124)
- [x] nice — приоритет (`-n N` принят; ABI без приоритетов, no-op)
- [x] nproc — число CPU (1)
- [x] uptime — время работы
- [x] crc32 — IEEE CRC-32 (`0xEDB88320`), `crc size [name]`, `-c`
- [x] base58 — Bitcoin base58 encode/decode (`-d`), файл/stdin
- [x] hex2bin — hex → бинарный вывод (пробелы/переводы строк игнорируются), `-o FILE`
- [x] bin2hex — бинарный вход → hex (`-u`, `-w N`), файл/stdin
- [x] zcat — распаковать stored-block gzip в stdout (`gzip -dc`)
- [x] kill — сигналы TERM/KILL/INT/USR1/USR2/STOP/CONT/0 и номера
- [x] sha3sum — SHA3-256 (Keccak-f[1600], rate 136, pad 0x06), `-b`/`-c`/`--status`, файлы/stdin
- [x] calc — целочисленные выражения (`+ - * / %`, скобки, унарный минус), аргументы или `-e`; деление на 0 → 1, синтаксис → 2
- [x] watch — периодический вывод (`-n SEC`, `-t`, `-c N`)
- [x] xargs — аргументы из файла/stdin (`-a`, `-n`, `-I{}`, `-r`)
- [x] parallel — последовательный запуск (`-j N` принят, сериализуется)
- [x] mktemp — временный файл
- [x] install — копировать с правами (`-m`, `-D`, `-f`)
- [x] shred — затереть файл (`-n`, `-u`, `-z`, `-s`)
- [x] command — запуск/проверка команды (`-v`, `-V`)
- [x] type — как command (`-v`, `-V`; отдельное имя)
- [x] factor — разложение на простые множители
- [x] time — время выполнения (`clock_gettime`, real/user/sys)
- [x] exit — завершить с кодом (`exit [N]`, `sys::exit`)

## Утилиты разработчика и разное (101–120)

- [x] chmod — сменить права (per-run override, не persist)
- [x] xxd — hex dump/reverse (`-r`, `-l N`, `-s OFF`)
- [x] more — постраничный вывод (`-n`, `-d`; без TTY)
- [x] shuf — перемешать строки (`-n`, `-e`, `-i`; фиксированный seed)
- [x] nm — таблица символов ELF (`-D`, `-u`, `-n`)
- [x] curl — HTTP/1.0 GET (IPv4, `-o`, `-i`)
- [x] wget — HTTP/1.0 GET в файл (`-O`)
- [x] base85 — ASCII85 encode/decode (`-d`, `-a` для `<~ ~>`), файл/stdin, round-trip; ошибка → 1
- [x] readelf — ELF64 LE: `-h`, `-S`, `-s`, `-l`; ошибка → 1
- [x] numfmt — SI/IEC (`--to`, `--from`, `--to-unit`, `--suffix`, `--padding`)
- [x] gzip — gzip-поток с DEFLATE stored blocks (без сжатия), `-d/-c/-k/-f`
- [x] gunzip — `gzip -d`, отдельное имя
- [x] tar — ustar: create/list/extract, `-f`, `-C`, `-v`, файлы/каталоги/симлинки
- [x] untar — `tar -x`
- [x] uuidgen — RFC-4122 v4-shaped UUID от xorshift (clock_gettime(1)/pid), `-r`, `-n N`; уникальность не гарантирована
- [x] cal — месяц/год по григорианскому календарю, `-y`; время — epoch
- [x] paghfetch — баннер: uname (sysname/nodename/release/machine), uptime, nproc=1, PWD
- [x] arch — архитектура из uname
- [x] tree — дерево каталога
- [x] threads — 4 потока × 250 инкрементов под futex Mutex; counter=1000 (демо ABI, сверх 120)

> Примечание: 120 целевых `[x]` + `threads` реализованы в `programs/src/applets/` и
> собираются в `paghbox` 1.9.0. Живая проверка ведётся по ядру 0.16.1
> (`run /mnt/paghbox <applet> ...`), см. README и
> `docs/live-testing.md`. Слот `dns` (DNS-резолв, для которого в ABI нет
> syscall) отдан `sha512sum`, чтобы список остался ровно из 120 имён. Слоты
> `pgrep`/`pkill`/`ping`/`netstat`/`lsblk`/`addr2line` заменены на
> `sha3sum`/`calc`/`base85`/`readelf`/`uuidgen`/`paghfetch`: сигналы чужому
> процессу, ICMP, перечисление блочных устройств и DWARF-адреса в ABI
> 0.16.1 отсутствуют, а эти шесть утилит полностью реализуемы на нём.