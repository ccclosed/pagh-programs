# Живые прогоны paghbox

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
