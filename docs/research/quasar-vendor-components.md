# Исследование компонентов `/system/vendor/quasar/` и совместимости со стеком Quasar в эмуляторе `kuyase`

>[!note]
>Исследование выполнено по первичным источникам: живому устройству по сетевому ADB (`10.1.1.56:5555`, режим только чтение), локальным дампам в `backups/` (`bergamot-0.369.1.50.zip`, `bergamot-0.373.1.13.zip`, `mini3/rootfs.tar.gz`, `station-lite/station-lite-rootfs-2026-10-24.tgz`, `yandex-mini2-backup.zip`, `Yandex-Station3-Orion-factory-13/system.img`), всем 11 собранным гостевым образам `work/*-system.ext4` и исходному коду репозитория (`guest-init/`, `hw-bridge/`, `kuyase/`). Серийные номера и приватные ключи физического образца обезличены (`<REDACTED>`).

---

## 1. Паспорт живого устройства (`10.1.1.56:5555`) и эволюция прошивок по поколениям

### 1.1. Первичные данные с живого устройства (`10.1.1.56:5555`)

- **Профиль платы (`Board Profile`)**: `yandexmini` (Яндекс Станция Мини 1, `YNDX-0004`, платформа Linkplay A98 / Amlogic AXG).
- **Device Tree (`/proc/device-tree/`)**:
  - `model`: `Amlogic`
  - `compatible`: `amlogic, axg`
  - `amlogic-dt-id`: `axg_s420_v03`
- **Ядро и ОС (`/proc/version`, `/etc/os-release`)**:
  - `uname -a`: `Linux YandexStationMini_4E18 4.9.68 #1 SMP PREEMPT Mon Oct 21 16:43:35 MSK 2024 aarch64 GNU/Linux` (64-битное ядро `aarch64` с 32-битным userspace `ELF32 ARM` `/lib/ld-linux-armhf.so.3`).
  - `/etc/os-release`: `NAME=Buildroot`, `VERSION=2017.02-01622-ge6fff0cfdc-dirty`, `VERSION_ID=2017.02`.
- **Параметры загрузки (`/proc/cmdline`)**:
  ```text
  rootfstype=ramfs init=/init console=ttyS0,115200 no_console_suspend earlycon=aml_uart,0xff803000 ramoops.pstore_en=1 ramoops.record_size=0x8000 ramoops.console_size=0x4000 logo=,loaded,androidboot.selinux=enforcing androidboot.firstboot=1 jtag=apao androidboot.hardware=amlogic slot_suffix=_a androidboot.serialno=<REDACTED> androidboot.rpmb_state=0 root=/dev/ubi0_0 rootfstype=ubifs init=/sbin/init loglevel=3
  ```
- **Версия ПО Quasar**:
  - `/system/vendor/quasar/VERSION`: `3.21`
  - `/system/vendor/quasar/version_full`: `1.266.4.40.2633723173.20241021`
  - `maind --version`: `Software Revision: 71dc5354c0a60fa01ea56fdcadadfddb2263820c`, `Build Timestamp: 1729516715`.
- **Точки монтирования (`mount`, `df -h`)**:
  - `/dev/ubi0_0` -> `/` (`ubifs`, `rw,relatime`, `64.8M`, занято `57.0M` / `88%`)
  - `/dev/ubi1_0` -> `/bkupgrade` (`ubifs`, `rw,relatime`, `87.5M`, занято `8.0M` / `10%`)
  - `/data` -> симлинк `/bkupgrade/data` (создаётся в `/system/vendor/quasar/onboot.sh`), `/vendor` -> симлинк `/bkupgrade` (создаётся в `/etc/init.d/S05linkplay.sh`)
  - `tmpfs` -> `/tmp`, `/run`, `/dev/shm` (`118.2M`)
  - `/dev/nvram` -> символьное устройство `c 211 0` (создаётся `mknod /dev/nvram c 211 0` в `/etc/init.d/S05linkplay.sh`; хранит `mv_uuid` для `libnvram.so` / `libwmdb.so`).

### 1.2. Сравнительная таблица версий прошивок в `work/*-system.ext4` и `backups/`

| Профиль платы (`board_slug`) | Поле `common.deviceType` в `quasar.cfg` | `VERSION` | `version_full` / `softwareVersion` | Архитектура `maind` (размер) | Поколение стека |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `yandexmini` (`10.1.1.56`) | `yandexmini` | `3.21` | `1.266.4.40.2633723173.20241021` | `ELF32 ARM` (`26 395 308 Б`) | Gen 1 (Linkplay A98, 2024 EOL) |
| `orion` | `orion` | `3.21` | `0.13.1.36.3322101745.20250823.factory-13` | `ELF64 AArch64` (`49 598 840 Б`) | Gen 2 (Factory A5 64-bit, 08.2025) |
| `yandexmini2` | `yandexmini_2` | `3.21` | `0.369.1.25.4712742930.20260928.299` | `ELF32 ARM` (`21 929 376 Б`) | Gen 3 (`0.369`, 09–10.2026) |
| `yandexmicro` | `yandexmicro` | `3.21` | `0.369.1.46.4778262442.20261013.297` | `ELF32 ARM` (`21 819 288 Б`) | Gen 3 (`0.369`, 10.2026) |
| `yandexmini2nc` | `yandexmini_2` | `3.21` | `0.369.1.50.4794659041.20261021.299` | `ELF32 ARM` (`21 949 504 Б`) | Gen 3 (`0.369`, 10.2026) |
| `plum` | `plum` | `3.21` | `0.369.1.50.4795647582.20261021.133` | `ELF32 ARM` (`22 239 424 Б`) | Gen 3 (`0.369`, 10.2026) |
| `plumnc` | `plum` | `3.21` | `0.369.1.50.4794654278.20261021.133` | `ELF32 ARM` (`22 239 424 Б`) | Gen 3 (`0.369`, 10.2026) |
| `mango` | `mango` | `3.21` | `0.369.1.50.4794659377.20261021.93` | `ELF32 ARM` (`18 694 000 Б`) | Gen 3 (`0.369`, 10.2026) |
| `station2` | `yandexmidi` | `3.21` | `0.369.1.50.4794649087.20261021.295` | `ELF32 ARM` (`24 302 016 Б`) | Gen 3 (`0.369`, 10.2026) |
| `cucumber` | `cucumber` | `3.21` | `0.369.1.50.4794663100.20261021.160` | `ELF32 ARM` (`27 618 596 Б`) | Gen 3 (`0.369`, 10.2026) |
| `bergamot` | `bergamot` | `3.21` | `0.373.1.13.4818984688.20261030.121` | `ELF32 ARM` (`21 323 228 Б`) | Gen 3 (`0.373`, 10.2026, модули `4.9.113`) |

### 1.3. Сравнение состава `/system/vendor/quasar/` между старыми и новыми прошивками

Уже в поколении `1.266` (`yandexmini`) бинарник `/system/vendor/quasar/maind` представляет собой единый мультибинарник (диспетчеризация по `argv[0]` для утилит-симлинков и по `argv[1]` `--<option>` для демонов и CLI-команд). В прошивках `0.369.1.*` / `0.373.1.13` (2026 г.) из `/system/vendor/quasar/` были вычищены остатки проприетарного окружения Linkplay (`/system/workdir`) и устаревшие отдельные бинарники (`updater`, `updater_switcher`, `ntpclient`, `openssl`), а набор апплетов `maind` и подкаталогов расширился:

| Файл / каталог в `/system/vendor/quasar/` | `yandexmini` (`1.266.4.40`) | `orion` (`0.13.1.36`) | Линейка `0.369` / `0.373` (2026) | Тип и назначение |
| :--- | :--- | :--- | :--- | :--- |
| `maind` | ELF32 (`26.4 МБ`) | ELF64 (`49.6 МБ`) | ELF32 (`18.7–27.6 МБ`) | Единый мультибинарник сервисов Quasar (`yandex_io`) и апплетов |
| `quasar_launcher2` | ELF32 (`289 КБ`) | ELF64 (`602 КБ`) | ELF32 (`318 КБ`) | Супервизор процессов, читающий `launch.json` |
| `q` | Скрипт (`2324 Б`) | Скрипт (`2478 Б`) | Скрипт (`2478 Б`) | CLI-обёртка управления `quasar_launcher2` и `maind` (в `2478 Б` добавлена команда `glagol-connections`) |
| `quasar.cfg` | JSON (`50.3 КБ`) | JSON (`80.6 КБ`) | JSON (`58.2–88.7 КБ`) | Главный конфигурационный файл всех сервисов, портов, ALSA/GStreamer и LED |
| `launch.json` | JSON (`2284/3118 Б`) | JSON (`3392 Б`) | JSON (`1318–3034 Б`) | Граф запуска процессов (`order`, `delay`, `allowedRestartIntervalSeconds`) для `quasar_launcher2` |
| `led_util` | `-> maind` | `-> maind` | `-> maind` | Проигрыватель анимаций `.led` на основном кольце (`led_util <file.led>`) |
| `quasar_client` | `-> maind` | `-> maind` | `-> maind` | Консольный IPC-клиент для отправки Protobuf-сообщений в сервисы `quasar.cfg` |
| `call_util` | `-> maind` | Удалён | Удалён | Интерактивный REPL-клиент к `calld` (`:9998`); в новых прошивках вызовы перенесены внутрь `--platformd` |
| `clock_led_util` | Нет | Нет | `-> maind` (`bergamot`, `cucumber`, `plum*`, `yandexmini2*`) | Апплет `sc_led_util.cpp` (`segment_clock_led_util`) для сегментного экрана часов |
| `battery_led_util` | Нет | Нет | `-> maind` (`mango`) | Апплет индикации заряда на `/dev/fb1` (`logo`) для Станции Стрит |
| `frame_buffer_ledpattern_analyser` | Нет | Нет | `-> maind` (`bergamot`, `cucumber`, `mango`, `plum*`, `station2`) | Валидатор кадров `/dev/fb*` (`BGR24`, `BGR48`, `BGRW32`, `RGBW32`) по `.led`-файлам |
| `tcp_alsa_writer` | Нет | Нет | `-> maind` (все `0.369`/`0.373`) | TCP-сервер для прямой записи сырого PCM-потока в указанное ALSA-устройство |
| `metrics_collector` | Нет (внутри `maind`) | `-> maind` | `-> maind` (все `0.369`/`0.373`) | Сборщик системных метрик CPU/RAM/UBI/PSI (`metrics_collector.cpp`) |
| `keymaster_proxy_app` | Нет | `-> maind` | `-> maind` (все `0.369`/`0.373`) | Клиент UNIX-сокета `/usr/bin/keymaster_svc` (OP-TEE Keymaster TA: `sign`, `decrypt`, `show-key`, `upgrade-key`) |
| `begemot_client_util` | Нет | `-> maind` | `-> maind` (`cucumber`) | Тестовый клиент к локальному NLU-серверу `begemot` |
| `begemot/` | Нет | `begemot` (`51 МБ` ELF64) + `begemot.cfg` + `search/` | `begemot.cfg` + `search/` (`cucumber`) | Локальный NLU/Wizard-движок (`NBg.NProto.TConfig`, порт `:8866`, gRPC `:8867`) |
| `bulbik/` | Нет | `bulbik` (`258 МБ` ELF64) | `bulbik` (`128.7 МБ` ELF32, `cucumber`) | Локальный рантайм Умного дома (Matter / Zigbee / сценарии), порт `localhost:9337` |
| `updater`, `updater_switcher` | ELF32 (`3.29 МБ` и `240 КБ` в `.orig`, на живой заменены стабами `93 Б`) | `updater_switcher` (`497 КБ` ELF64) | Удалены | Переключатель между старым `updater` + `maind --updater_gateway` и `maind --updatesd`. В `0.369+` остался только `maind --updatesd` |
| `a01localupdate`, `factory_test`, `enter_factory_mode.sh`, `data_reset.sh`, `bsa_server_run.sh` | Присутствуют | Удалены | Удалены | Утилиты Linkplay SDK (`libwm_upgrade.so`, `libmvmsg.so`, Broadcom `bsa_server`). В Gen 2/3 заменены на BlueZ (`bluetoothd` + `bluealsa`) и `swupdate` |
| `ntpclient`, `ntp_sync.sh` | ELF32 (`18 КБ`) + скрипт | Заменены на `chrony-status` | Заменены на `chrony-status` | Переход со скриптового опроса `ntpclient` на системный демон `chronyd` (`/etc/init.d/S49chrony`) |
| `openssl`, `gen_keys.sh` | ELF32 (`416 КБ`) + скрипт | Удалены | Удалены | Переход с RSA-ключей в `/data/quasar/private.pem` на аппаратный OP-TEE `keymaster_svc` |
| `yio_iptables_init.sh`, `yio_iptables_count.sh` | Нет | Нет | Скрипты (`2775 Б` и `421 Б`, кроме `mango`) | Инициализация и снятие счётчиков трафика в цепочках `yio_INPUT_*` / `yio_OUTPUT_*` (`tcp`, `udp`, `icmp`, `dns:53`, `ntpd:123`, `glagol:1961`, `netclock:11125`) |
| `activate_aux_as_usb.sh` | Нет | Скрипт (`607 Б`) | Скрипт (`607 Б`, `yandexmini2*`) | Переключение пина `gpio457` (`GPIOA_20`) и `/sys/module/card/parameters/suppress_hp_det` для мультиплексирования разъёма AUX 3.5 мм в режим USB |
| `increment_boot_id.sh` | Нет | Нет | Скрипт (`149 Б`, `bergamot`) | Инкремент счётчика загрузок `/data/quasar/boot_id` при старте `onboot.sh` |
| `demo_mode.sh` | Нет | Нет | Скрипт (`62 Б`, `station2`) | Создаёт `/data/quasar/data/demo_mode` и перезапускает `maind` |
| `zstd.dict` | Нет | Нет | Файл (`49 152 Б`, все `0.369`/`0.373`) | Словарь Zstandard для сжатия WebSocket-трафика UniProxy (`wsUseZstd: true`) |
| `ledconfigs/`, `vqe_models/`, `matter/`, `zigbee/`, `asr_lingware/` | Нет | Присутствуют | Присутствуют (по профилю платы) | 3×3 матрицы цветокоррекции корпуса (`beige.json`, `blue.json`, `green.json`, `pink.json`, `purple.json`, `red.json`, `yellow.json`), модели VQE, сертификаты Matter PAA, прошивки Zigbee NCP (`gateway.ybl`) |

---

## 2. Детальный разбор `/system/vendor/quasar/q`, апплетов `maind`, скриптов и конфигураций

### 2.1. Устройство `/system/vendor/quasar/q`

`/system/vendor/quasar/q` — это **POSIX shell-скрипт** (`#!/bin/sh`), а не скомпилированный бинарник:
- На `yandexmini` (`1.266.4.40`) его размер составляет `2324 Б` (12 подкоманд).
- На всех остальных платах (`0.13.1.36`, `0.369.1.*`, `0.373.1.13`) его размер — `2478 Б` (13 подкоманд: добавлена `glagol-connections`).

Полное содержимое `/system/vendor/quasar/q` (версия `2478 Б`; на `yandexmini` отсутствует только `glagol_connections`):

```sh
#!/bin/sh

function quasar_run() {
    binary=$1
    shift
    LD_LIBRARY_PATH=/system/vendor/quasar/libs /system/vendor/quasar/$binary "$@"
}

function quasar_status() {
    pidof quasar_launcher2 > /dev/null
}

function quasar_start() {
    if quasar_status; then
        echo "quasar is already running"
        return 1
    fi

    echo "Starting quasar..."
    /system/vendor/quasar/run.sh
}

function quasar_stop() {
    if ! quasar_status; then
        echo "quasar is not running"
        return 1
    fi

    echo "Stopping quasar..."
    killall quasar_launcher2
    sleep 5
    killall -9 maind
    killall -9 begemot 2> /dev/null
}

function quasar_restart() {
    quasar_stop
    quasar_start
}

function quasar_version() {
    quasar_run maind --version
}

function quasar_device_id() {
    quasar_run maind --device-id
}

function quasar_wifi_info() {
    quasar_run maind --wifi-info
}

function glagol_connections() {
    quasar_run maind --glagol-connections
}

function quasar_full_info() {
    quasar_run maind --full-info
}

function quasar_set_dev_backend_flag() {
    quasar_run maind --set-dev-backend-flag
}

function quasar_remove_dev_backend_flag() {
    quasar_run maind --remove-dev-backend-flag
}

function quasar_cli() {
    quasar_run maind --cli
}

function quasar_save_last_monotonic_time() {
    quasar_run maind --save-last-monotonic-time
}

function quasar_get_prop() {
    quasar_run maind --getprop /data/quasar/props &> /dev/null
    cat /data/quasar/props
    rm /data/quasar/props
}

function usage() {
    echo "Usage: q {start|stop|restart|version|device-id|wifi-info|glagol-connections|full-info|backend-dev|backend-prod|cli|save-last-monotonic-time|getprop}"
}
```

Как работает каждая подкоманда `q`:
1. `q start` / `q stop` / `q restart`: проверяют `pidof quasar_launcher2 > /dev/null`. При остановке посылают `SIGTERM` в `quasar_launcher2`, ждут 5 с и добивают `killall -9 maind` и `killall -9 begemot`.
2. `q version` (`maind --version`): выводит git-ревизию сборки Аркадии (`Software Revision: ...`) и строку `Quasar Software Version: ...`.
3. `q device-id` (`maind --device-id`):
   - На `yandexmini` (`1.266.4.40`) вызывает `wm_db_fnvram_get("mv_uuid")` из `libwmdb.so` / `libnvram.so` (читает `/bkupgrade/new_nvram_config` и `/dev/nvram`).
   - На платах Gen 2/3 читает `androidboot.deviceid` / `androidboot.serialno` из `/proc/cmdline` либо unifykeys `deviceid` (в QEMU возвращает `Device ID: EMU0001`).
4. `q wifi-info` (`maind --wifi-info`): опрашивает сетевой интерфейс `wlan0` через `ioctl`/`nl80211` и выводит JSON со статистикой `bss`, `interface`, `iwstats`, `rx`, `signal`, `tx`.
5. `q glagol-connections` (`maind --glagol-connections`): подключается по локальному IPC к сервису `glagold` (`127.0.0.1:9899`) и запрашивает список активных подключений Glagol.
6. `q full-info` (`maind --full-info`): объединяет вывод `--version`, `--build-timestamp`, `--device-id` и `--device-localization` (`localization={.country=COUNTRY_RU, .locale=ru-RU}`).
7. `q backend-dev` / `q backend-prod`: создаёт или удаляет файл флага `/data/quasar/devFlag` (`devConfig.devFlag` в `quasar.cfg`, переключающий `backendUrl` на `https://quasar.yandex.ru/dev`) и выполняет `quasar_restart`.
8. `q cli` (`maind --cli`): запускает интерактивный консольный клиент к `aliced` (`:9875`) и `mediad` (`:9900`) с однобуквенными командами:
   - `'c'` Cancel Conversation (`AliceCapabilityMethod`: `STOP_CONVERSATION = 3`)
   - `'l'` Player Prev Track (`PlaybackControlCapability`)
   - `'m'` Toggle mute (`DeviceStateCapability`)
   - `'n'` Player Next Track (`PlaybackControlCapability`)
   - `'o'` Player Pause (`PlaybackControlCapability`)
   - `'p'` Toggle PlayPause (`PlaybackControlCapability`)
   - `'q'` Text Input Query (`AliceCapabilityMethod`: `START_REQUEST = 0` с JSON VINS-запросом)
   - `'s'` Start Conversation (`AliceCapabilityMethod`: `START_CONVERSATION = 2`)
   - `'t'` Toggle Conversation
   - `'x'` Exit
9. `q save-last-monotonic-time` (`maind --save-last-monotonic-time`): записывает текущее время в `/tmp/power_off_timestamp` (`maind.lastMonotonicClockFile`) и `/data/quasar/data/monotonic_time` (`ntpd.monotonicClockFile`).
10. `q getprop` (`maind --getprop /data/quasar/props`): генерирует однострочный JSON с Android-подобными свойствами платформы во временный файл `/data/quasar/props`, выводит его в `stdout` и удаляет файл:
    ```json
    {"gsm.operator.alpha":"","gsm.sim.operator.alpha":"","ro.board.platform":"Linux","ro.build.version.preview_sdk":"0","ro.build.version.release":"0.373.1.13.4818984688.20261030.121","ro.build.version.sdk":"0.373.1.13.4818984688.20261030.121","ro.opengles.version":"1","ro.product.brand":"bergamot","ro.product.cpu.abi":"arm64-v8a","ro.product.cpu.abilist":"arm64-v8a,armeabi-v7a,armeabi","ro.product.cpu.abilist32":"armeabi-v7a,armeabi","ro.product.cpu.abilist64":"arm64-v8a","ro.product.device":"bergamot","ro.product.manufacturer":"bergamot","ro.product.model":"bergamot","ro.product.name":"bergamot","sys.boot_completed":"1"}
    ```

### 2.2. Опции мультибинарника `maind` и апплеты `argv[0]`

Вывод `/system/vendor/quasar/maind --help` по поколениям:
- **`yandexmini` (`1.266.4.40`)**:
  ```text
  Usage: /system/vendor/quasar/maind <OPTION>
  Tools name options: --build-timestamp, --cli, --device-id, --device-localization, --full-info, --getprop, --nettest, --remove-dev-backend-flag, --save-last-monotonic-time, --set-dev-backend-flag, --version, --wifi-info
  Daemons name options: --calld, --fluent-bitd, --mediad, --platformd, --telemetry, --updater_gateway, --updatesd
  ```
- **`bergamot`, `yandexmini2`, `plum`, `mango`, `station2`, `yandexmicro` (`0.369` / `0.373`)**:
  ```text
  Usage: /system/vendor/quasar/maind <OPTION>
  Tools name options: --build-timestamp, --cli, --device-id, --device-localization, --full-info, --getprop, --glagol-connections, --gst-launch, --nettest, --remove-dev-backend-flag, --save-last-monotonic-time, --set-dev-backend-flag, --version, --wifi-info
  Daemons name options: --fluent-bitd, --mediad, --platformd, --telemetry, --updatesd
  ```
- **`cucumber` (`0.369.1.50`) и `orion` (`0.13.1.36`)**:
  В `Daemons name options` дополнительно присутствует `--loggerd` (используется в `launch.json` как `"loggerPath": "/system/vendor/quasar/maind", "loggerArgs": ["--loggerd", "bulbik"]`).

Сигнатуры апплетов, вызываемых через симлинки на `maind` (по проверке `argv[0]`):
- **`quasar_client`**:
  ```text
  Usage: /system/vendor/quasar/quasar_client [OPTIONS]
    -s, --service SERVICE     Service we have to connect to
    -c, --config-file PATH    Config file from which we take service port (default: /system/vendor/quasar/quasar.cfg)
    -m, --message MESSAGE     Send a message to the host: pass as raw bytes encoded as base64
    -h, --host HOST           Host we have to connect to
    -p, --port PORT           Port we have to connect to
  ```
- **`led_util` и `battery_led_util`**:
  Принимают один позиционный аргумент — путь к текстовому `.led`-файлу (`led_util /system/vendor/quasar/ledpatterns/oknix/alice_listening.led`). На `yandexmini` `led_util` работает через `libwmlighteffect.so` -> `/dev/i2c-0` (`SN3236`), на новых платах — открывает `/dev/fb0` (`led_util`) или `/dev/fb1` (`battery_led_util`) и применяет матрицу цветокоррекции из `/system/vendor/quasar/ledconfigs/<color>.json`.
- **`frame_buffer_ledpattern_analyser`**:
  ```text
  Usage: /system/vendor/quasar/frame_buffer_ledpattern_analyser [OPTIONS]
  Required parameters:
    {-d|--fb-device} VAL       Framebuffer device path
    {-m|--memory-format} VAL   Memory format: BGR24, BGR48, BGRW32, RGBW32
    {-p|--pattern-dir} VAL     Directory with .led pattern files
  Optional parameters:
    --led-shift VAL, --led-count VAL, --config VAL, --config-entry VAL, {-t|--duration} VAL, --pattern-duration VAL, --fps VAL, --threshold VAL
  ```
- **`tcp_alsa_writer`**:
  ```text
  Usage: /system/vendor/quasar/tcp_alsa_writer -d <pcm-device-name> -r <sample-rate> -c <channels> -s <bits-per-sample> -p <port>
  ```
- **`metrics_collector`**:
  ```text
  Usage: /system/vendor/quasar/metrics_collector -t <time-period-sec> [-p <process>...] [-f <dump-file>] [--logs-to-stdout]
  ```
- **`keymaster_proxy_app`**:
  ```text
  Usage: keymaster_proxy_app path_to_socket operation [input file] [output file]
  Possible operations: [sign|decrypt|show-key|upgrade-key|generate-key|force-generate-key]
  ```

### 2.3. Скрипты в `/system/vendor/quasar/` и модификации на живом устройстве

На живом устройстве `10.1.1.56:5555` в `/bkupgrade/data/opt/backup/` и `/bkupgrade/data/opt/tweak-backups/` сохранены заводские оригиналы файлов до применения комплекта `yandex-micro-kit.sh` и твиков `01_disable_telemetry.sh`:

1. **`onboot.sh`** (вызывается из `/etc/init.d/S99onboot.sh` на `yandexmini`, `/etc/init.d/S99quasar` на `bergamot`/`yandexmini2`/`plum`/`mango`/`station2`/`yandexmicro`, `/etc/init.d/S97quasar` на `cucumber`/`orion`):
   - Создаёт симлинк `/data -> /bkupgrade/data` (если `/data` отсутствует), включает `echo 1 > /proc/sys/vm/overcommit_memory`.
   - Очищает `/bkupgrade/update_*` и `/bkupgrade/ota/*`.
   - Экспортирует `LD_LIBRARY_PATH=/system/vendor/quasar:/system/vendor/quasar/libs:$LD_LIBRARY_PATH`, `PATH=/system/vendor/quasar:$PATH`, `GST_PLUGIN_PATH=/system/vendor/quasar/libs`.
   - При первом старте (`! -f /data/quasar/initialized`) создаёт `/data/quasar/{logs,daemons_logs,data}` (на `yandexmini` также вызывает `gen_keys.sh`).
   - На новых платах вызывает `/system/vendor/quasar/yio_iptables_init.sh` и (на `bergamot`) `/system/vendor/quasar/increment_boot_id.sh`.
   - Вызывает `/system/vendor/quasar/set_monotonic_time.sh` (сверяет `date +%s` с `maind --build-timestamp` и `/data/quasar/data/monotonic_time`), `/system/vendor/quasar/activate_adb.sh check`, `/system/vendor/quasar/q save-last-monotonic-time` и запускает `/system/vendor/quasar/run.sh`.
2. **`run.sh`**:
   - Переходит в `cd /data/quasar`, выставляет `CA_CERTIFICATES=/system/vendor/quasar/ca-certificates.crt`, `LD_LIBRARY_PATH=/system/vendor/quasar/libs`, `ulimit -n 10240`, `ulimit -s 1024`.
   - На `yandexmini` дополнительно очищает `daemons_logs/*.log.*`, `/data/quasar/_*` и создаёт FIFO `mkfifo /tmp/loop_capture_pcm`.
   - Запускает `nohup /system/vendor/quasar/quasar_launcher2 /system/vendor/quasar/launch.json > launcher2.out 2>&1 &`.
3. **`activate_adb.sh` и `adb_options.inc`**:
   - `adb_options.inc` парсит флаги `{--sound|--quasmodrom|--glagol|--all} [--network] {activate|deactivate|check}` и управляет файлами-маркерами в `/data/quasar_activate_adb_{sound,quasmodrom,glagol}` и `/data/quasar_activate_network_adb`.
   - Заводской `activate_adb.sh.orig` на `yandexmini` при `deactivate` выполняет `killall adbd && mv /usr/bin/adbd /usr/bin/adbd_backup`. На `yandexmini2`/`station2` вызывает `/usr/bin/adb.sh stop` и `start-stop-daemon -K -b -x adbd`. На `bergamot`/`cucumber`/`mango`/`orion`/`plum` вызывает `yandex-debugctl stop` (`adb-net` / `adb-usb`).
   - На живой колонке `10.1.1.56:5555` скрипт `activate_adb.sh` пропатчен `yandex-micro-kit`: он игнорирует облачную команду деактивации из `quasar.cfg` (`adbd.allDeactivateCmd` / `quasmodromDeactivateCmd`), восстанавливает `/usr/bin/adbd` из `/usr/bin/adbd_backup`, удаляет `/dev/usb-ffs/adb` (чтобы `adbd` слушал TCP `:5555`), очищает паразитный маршрут `10.0.0.0/8 dev wlan1` от Linkplay AP и держит запущенными `adbd` и `dropbear` (`:22`, `:2222`).
4. **`device_ota_update.sh`, `updater`, `updater_switcher`**:
   - Заводской `device_ota_update.sh.orig` останавливает `quasar_launcher2` и `maind`, сбрасывает кеши (`echo 3 > /proc/sys/vm/drop_caches`), выключает ЦАП/светодиоды и вызывает `apply_packed_ota "$OTA_FILE_NAME"` с последующим `reboot`.
   - На живом устройстве `10.1.1.56:5555` `device_ota_update.sh` заменён заглушкой `exit 0`, а `updater` и `updater_switcher` заменены бесконечным циклом `while :; do sleep 86400; done` (оригиналы `updater.orig` `3 290 884 Б` и `updater_switcher.orig` `240 552 Б` сохранены в `/bkupgrade/data/opt/backup/`).
5. **`set_volume.sh`**:
   - На `yandexmini`: `LD_LIBRARY_PATH=/system/vendor/quasar/libs /system/workdir/bin/amixer sset 'Master' $1%`
   - На `bergamot`, `mango`, `plum*`, `station2`, `yandexmicro`, `yandexmini2*`: `amixer set 'Master' $1%`
   - На `orion`: управляет 6 аппаратными каналами усилителей ACM8625S (`amixer sset 'wf Acm8625 AMP ChR' $1%`, `ChL`, `fr2`, `fr1`).
   - На `cucumber`: скрипт `set_volume.sh` отсутствует в `/system/vendor/quasar/` (громкостью управляет демон `/usr/bin/hardvold`).

### 2.4. Процессы, сокеты и матрица IPC-портов на живом устройстве (`10.1.1.56:5555`)

На живой колонке `10.1.1.56:5555` в `launch.json` (после удаления `telemetry` и `fluent-bitd` скриптом `01_disable_telemetry.sh`, перенаправившим `clickdaemonHost` и `appMetricaStartupHost` в `quasar.cfg` на `127.0.0.1`) супервизор `quasar_launcher2` (PID `1973`) держит запущенными:
- `maind --platformd` (PID `1975`)
- `maind --mediad` (PID `2106`)
- `maind --calld` (PID `2107`)
- `/bin/sh /system/vendor/quasar/updater_switcher` (PID `2108`)

Помимо стека Quasar, на `10.1.1.56:5555` работают:
- `rootApp` (PID `1187`) и `mv_ioguard` (PID `1371`, слушает TCP `0.0.0.0:8899`, UART `/dev/ttyS3` и UNIX-сокеты `/tmp/RequestIOGuard`, `/tmp/volumeprompt`, `/tmp/volumeprompt1`) — демоны Linkplay SDK (`/system/workdir/bin/`).
- `/system/workdir/bin/bsa_server` (PID `2061`) — Bluetooth-стек Broadcom (`/dev/ttyS1`, `/tmp/bt-daemon-socket`, `/tmp/bt-avk-fifo`).
- `/usr/sbin/unbound` (PID `2798`, пользователь `nobody`) — локальный DNS-резолвер (`127.0.0.1:53`, `::1:53`).
- `/bkupgrade/data/opt/bin/alinad` (PID `1840`, запущен из `/etc/init.d/S99alinad`) — сторонний демон-компаньон, который:
  - держит открытыми `/dev/i2c-0` (контроллер кольца `SN3236`), `/dev/input/event0..2` и `/tmp/daemons_logs/maind.log`;
  - слушает внешний порт `0.0.0.0:8090` (TCP и UDP);
  - держит активные клиентские TCP-соединения к `maind --platformd` на **`127.0.0.1:9850`** (`yio_audio`, `socket:[356]`) и **`127.0.0.1:9875`** (`aliced`, `socket:[18511]`).

Полная сверка TCP/UDP-портов из `quasar.cfg` с таблицей сокетов ядра (`/proc/net/tcp`, `/proc/net/tcp6`, `/proc/net/udp` и `/proc/<pid>/fd`) на `10.1.1.56:5555`:

| Ключ в `quasar.cfg` | Порт | Статус в `/proc/net/*` на `10.1.1.56` | Владелец (PID / процесс / FD) | Примечание по архитектуре `MixedIpcServer` |
| :--- | :--- | :--- | :--- | :--- |
| `glagold.externalPort` | `1961` (TCP6) | `LISTEN :::1961` | `1975` (`maind --platformd`, fd `155`) | Внешний TLS WebSocket-сервер протокола Glagol |
| `phone_calls.pjsip.sip_local_port` | `5081` (TCP) | `LISTEN 0.0.0.0:5081` | `1975` (`maind --platformd`, fd `86`) | Локальный SIP-порт PJSIP |
| `firstrund.httpPort` | `8080` (TCP/TCP6) | `LISTEN 0.0.0.0:8080` | `1975` (`maind --platformd`, fd `65`, `72`) | HTTP-сервер первоначальной настройки Wi-Fi |
| `fluent-bitd.port` | `9832` (TCP) | Не открыт | `maind --fluent-bitd` | Отключён в `launch.json` на живой колонке |
| `audio_commands.port` | `9849` (TCP) | Не открыт (inproc) | Внутри `platformd` | Удалён из `quasar.cfg` в `0.13+` / `0.369+` |
| `yio_audio.port` | `9850` (TCP) | `LISTEN 127.0.0.1:9850` | `1975` (`maind --platformd`, fd `18`) | Поток микрофонных каналов `raw_mic_0..3` и `vqe_0` (подключён `alinad`) |
| `aliced.port` | `9875` (TCP) | `LISTEN 127.0.0.1:9875` | `1975` (`maind --platformd`, fd `54`) | IPC `aliced` (`QuasarMessage.remoting` / `AliceState`; подключены `calld` и `alinad`) |
| `wifid.port` | `9877` (TCP) | `LISTEN 127.0.0.1:9877` | `1975` (`maind --platformd`, fd `17`) | В списке `common.ipc.net`; 6 внутренних соединений |
| `authd.port` | `9878` (TCP) | `LISTEN 127.0.0.1:9878` | `1975` (`maind --platformd`, fd `71`) | В списке `common.ipc.net`; подключены `mediad` и `calld` |
| `external.port` | `9880` (TCP) | Не открыт | Внутри `platformd` | Открывается только при активном `_quasar._tcp.` |
| `pushd.port` | `9881` (TCP) | `LISTEN 127.0.0.1:9881` | `1975` (`maind --platformd`, fd `53`) | Xiva WebSocket; в `0.369+` удалён из `common.ipc.net` |
| `config_capability.port` | `9882` (TCP) | `LISTEN 127.0.0.1:9882` | `1975` (`maind --platformd`, fd `80`) | Раздаёт конфигурацию процессам `mediad` и `calld` |
| `audioinit.port` | `9883` (TCP) | Не открыт (inproc) | Внутри `platformd` | Удалён в `0.373.1.13` (`bergamot`) |
| `firstrund.port` | `9884` (TCP) | `LISTEN 127.0.0.1:9884` | `1975` (`maind --platformd`, fd `83`) | В списке `common.ipc.net` |
| `alarmd.port` | `9886` (TCP) | Не открыт (inproc) | Внутри `platformd` | `useNewAlarmCapability: true`; удалён в `0.13+` / `0.369+` |
| `metricad.port` | `9887` (TCP) | Не открыт | `maind --telemetry` | Отключён в `launch.json` на живой колонке |
| `updatesd.port` | `9889` (TCP) | Не открыт | `maind --updatesd` | Отключён стабом `updater_switcher` на живой колонке |
| `external_commandsd.port` | `9891` (TCP) | Не открыт (inproc) | Внутри `platformd` | Удалён в `0.373.1.13` (`bergamot`) |
| `monitord.port` | `9895` (TCP) | Не открыт | `maind --telemetry` | Отключён в `launch.json` на живой колонке |
| `syncd.port` | `9897` (TCP) | `LISTEN 127.0.0.1:9897` | `1975` (`maind --platformd`, fd `79`) | В списке `common.ipc.net` |
| `brickd.port` | `9898` (TCP) | `LISTEN 127.0.0.1:9898` | `1975` (`maind --platformd`, fd `128`) | Контроль подписки / блокировки (`subscription_mode.dat`) |
| `glagold.port` | `9899` (TCP) | `LISTEN 127.0.0.1:9899` | `1975` (`maind --platformd`, fd `134`) | Внутренний IPC `glagold` (используется `q glagol-connections`) |
| `mediad.port` | `9900` (TCP) | `LISTEN 127.0.0.1:9900` | `2106` (`maind --mediad`, fd `32`) | Управление плеером GStreamer (`platformd` -> `mediad`) |
| `iohub_services.port` | `9903` (TCP) | `LISTEN 127.0.0.1:9903` | `1975` (`maind --platformd`, fd `48`) | Шина директив и событий `IOHub` (подключены `mediad` и `calld`) |
| `metrica2d.port` | `9904` (TCP) | Не открыт (inproc) | Внутри `telemetry` | Удалён в `0.373.1.13` (`bergamot`) |
| `audioclient.port` | `9906` (TCP) | `LISTEN 127.0.0.1:9906` | `2106` (`maind --mediad`, fd `29`) | Аудиоклиент GStreamer (`platformd` -> `mediad`) |
| `notificationd.port` | `9907` (TCP) | Не открыт (inproc) | Внутри `platformd` | Внутрипроцессная очередь в `MixedIpcServer` |
| `calld.port` / `update_health_check.port` | `9908` (TCP) | `LISTEN 127.0.0.1:9908` | `2107` (`maind --calld`, fd `20`) | В `1.266`/`0.13` — `calld.port`; в `0.369+`/`0.373+` переназначен на `update_health_check.port`! |
| `do_not_disturb.port` | `9909` (TCP) | Не открыт (inproc) | Внутри `platformd` | Удалён в `0.373.1.13` (`bergamot`) |
| `iohub_clients.port` | `9910` (TCP) | `LISTEN 127.0.0.1:9910` | `1975` (`maind --platformd`, fd `49`) | Клиентский порт шины `IOHub` |
| `ntpd.port` | `9911` (TCP) | `LISTEN 127.0.0.1:9911` | `1975` (`maind --platformd`, fd `50`) | Служба синхронизации времени Quasar |
| `setupd.port` | `9912` (TCP) | `LISTEN 127.0.0.1:9912` | `1975` (`maind --platformd`, fd `59`) | Ультразвуковая/акустическая инициализация (`secretMsgList`) |
| `ble_initd.port` | `9913` (TCP) | Нет в `1.266` | В `0.13+` / `0.369+` | BLE-онбординг на новых платах |
| `networkd.port` | `9917` (TCP) | `LISTEN 127.0.0.1:9917` | `1975` (`maind --platformd`, fd `62`) | В списке `common.ipc.net`; подключены `mediad` и `calld` |
| `stereo_pair.port` | `9921` (TCP) | `LISTEN 127.0.0.1:9921` | `1975` (`maind --platformd`, fd `102`) | Канал стереопары |
| `volume_manager.port` | `9922` (TCP) | Не открыт (inproc) | Внутри `platformd` | Удалён в `0.373.1.13` (`bergamot`) |
| `spectrogram_animation.port` | `9923` (TCP) | Нет в `1.266` | На `yandexmini2*`, `station2`, `cucumber`, `orion` | Спектрограмма свечения при воспроизведении |
| `backend.port` | `9924` (TCP) | `LISTEN 127.0.0.1:9924` | `1975` (`maind --platformd`, fd `56`) | Прокси запросов к бэкенду Quasar |
| `network_stats.port` | `9926` (TCP) | `LISTEN 127.0.0.1:9926` | `1975` (`maind --platformd`, fd `58`) | В списке `common.ipc.net` |
| `calld.utilPort` | `9998` (TCP) | Не открыт (по требованию) | `maind --calld` | Для утилиты `call_util`; удалён в `0.369+` |
| `multiroomd.port` | `11124` (TCP) | `LISTEN 127.0.0.1:11124` | `1975` (`maind --platformd`, fd `105`) | Локальный IPC мультирума |
| `net_clock.port` | `11125` (UDP6) | `UDP :::11125` | `1975` (`maind --platformd`, fd `127`) | Сетевая синхронизация часов мультирума |

---

## 3. Сравнение с реализацией эмулятора `kuyase` и диагностика несовместимостей

### 3.1. Что сейчас эмулируется в `kuyase` (`equasar-qemu-init.sh` + `qemu-hw-bridge`)

1. **Окружение и ФС (`guest-init/equasar-qemu-init.sh`, `kuyase/src/image_builder.rs`)**:
   - При сборке образа (`sanitize_tree`) очищаются `/data`, `/bkupgrade`, `/tee`, `/nv`, `/env`, `/ota`, `/tmp`, `/run`, `/var/run` и пользовательские SSH/init-скрипты (`S99zdropbear`, `S99alinad`, `S100privacy`, `S89usbgadget`, `S97adb`), инжектируются `/quasar-qemu-init.sh`, `/etc/init.d/S99kuyase-yam` и `/usr/local/bin/qemu-hw-bridge`.
   - При загрузке `equasar-qemu-init.sh` экспортирует `PATH` (включая `/system/vendor/quasar`) и `LD_LIBRARY_PATH` (включая `/bkupgrade/data/opt/lib` и `/system/vendor/quasar/libs`), поднимает `adbd` на `:5555`, подменяет `/proc/device-tree` (`model`, `compatible`, `amlogic-dt-id`), создаёт фейковое дерево `/sys/class/{leds,graphics,backlight,thermal,input,unifykeys,power_supply}` и `/sys/bus/{iio,i2c}`, генерирует `/etc/asound.conf` поверх `snd-aloop` и запускает `qemu-hw-bridge`.
2. **Аппаратный мост и IPC-стаб (`hw-bridge/src/`)**:
   - В режиме `quasar.mode=stub` (по умолчанию) `qemu-hw-bridge` поднимает 3 TCP-сервера:
     - `127.0.0.1:9850` (`yio_audio` в `hw-bridge/src/ipc.rs:14`): каждые `40 мс` стримит фреймы `[u32_le len][QuasarMessage { field 158: ChannelsData }]` с 4 каналами `raw_mic_0..3` и каналом `vqe_0` (`doa_angle`, `is_for_recognition = true`).
     - `127.0.0.1:9875` (`aliced` в `hw-bridge/src/ipc.rs:88`): рассылает подписчикам (например, `luna` `/speech/next`) события `QuasarMessage { field 3: AliceState }` при `HostCommand::InjectVoice`, принимает команды `QuasarMessage { field 210: Remoting }`, обрабатывает перехват диалога (`CancelDialog`: методы `1`, `3`, `6`; `StopTts`: метод `14`) в окне `1680 мс` (`Hijack Window`) и озвучивает `START_VOCALIZE` (`18`) формантным синтезатором в ALSA.
     - `127.0.0.1:9903` (`iohub_services` в `hw-bridge/src/ipc.rs:191`): принимает `QuasarMessage` с полями `88` (`io_control`, вкл. `external_directives` `95`), `183` (`io_hub_message`), `157` (`service_name_ready`).
   - В режиме `quasar.mode=full` (`--mode full`) `equasar-qemu-init.sh:781` запускает `/system/vendor/quasar/onboot.sh start`, а `qemu-hw-bridge` уступает порты `:9850/:9875/:9903` штатному `maind`.

### 3.2. Что происходит при вызове `/system/vendor/quasar/q`, апплетов `maind` и сторонних пакетов в `kuyase`

При тестировании вызовов `/system/vendor/quasar/q`, апплетов `maind` и режима `--mode full` внутри `kuyase` выявлены **9 технических расхождений**, объясняющих поведение старых и новых прошивок:

#### 1. Новый конверт Protobuf (`поля 252 / 253`) в прошивках `0.369+` / `0.373+` ломает `q cli` на `:9875`
В прошивках `0.369.1.*` и `0.373.1.13` (`bergamot`, `yandexmini2*`, `plum*`, `mango`, `station2`, `cucumber`) изменился формат сериализации `QuasarMessage` в IPC:
- Вместо прямой записи вложенного сообщения в поле `210` (`remoting`), `maind` упаковывает полезную нагрузку в **поле `252`** (`tag = 0xe2 0x0f`, wire type `2`), а номер целевого поля `QuasarMessage` передаёт следом в **поле `253`** (`tag = 0xe8 0x0f`, wire type `0` varint):
  - Для `remoting` (`:9875`): `[e2 0f <len> <Remoting bytes>][e8 0f d2 01]` (где `0xd2 0x01` = `210`).
  - Для `glagold` (`:9899`, `q glagol-connections`): `09 00 00 00 e2 0f 02 42 00 e8 0f b9 01` (где `0xb9 0x01` = `185`).
- Так как `decode_quasar_remoting_packet` в `hw-bridge/src/proto.rs:242` ищет только поле `210` на верхнем уровне, вызов `/system/vendor/quasar/q cli` на новых прошивках приводит к ошибке `ERROR IPC :9875 -> Invalid Protobuf packet: missing QuasarMessage.remoting (field 210)` и инкрементирует `ipc_error_count`.
- Дополнительно в `AliceCapabilityMethod` (`field 8` внутри `Remoting`) `q cli` отправляет `method = 0` (`START_REQUEST` с текстовым VINS-запросом в поле `2`: `{"text":"...","type":"text_input"}`), `method = 2` (`START_CONVERSATION` — клавиша `'s'`) и `method = 3` (`STOP_CONVERSATION` — клавиша `'c'`), а клавиша `'m'` (`Toggle mute`) отправляет метод в поле `36` (`0xa2 0x02 0x00`) внутри `DeviceStateCapability` (`field 16`).

#### 2. Отказ `q stop` (код возврата `1`) и конфликт `q start` / `q restart` в `stub`-режиме
- В штатном режиме `quasar.mode=stub` процесс `quasar_launcher2` не запущен.
- Любой пользовательский скрипт или установщик пакета, вызывающий `/system/vendor/quasar/q stop`, проверяет `pidof quasar_launcher2 > /dev/null`, выводит `quasar is not running` и завершается с **кодом возврата `1`** (прерывая скрипты с `set -e`).
- Вызов `q start` или `q restart` в `stub`-режиме запускает `/system/vendor/quasar/run.sh` (`quasar_launcher2` -> `maind`), который начинает конкурировать с `qemu-hw-bridge` за сокеты, ALSA и фреймбуферы.

#### 3. Падение `maind --platformd` в режиме `--mode full` на всех BlueZ-прошивках (`0.13+` / `0.369+` / `0.373+`)
При запуске `kuyase run --mode full` на новых прошивках (`bergamot`, `yandexmini2*`, `plum*`, `mango`, `station2`, `yandexmicro`, `cucumber`, `orion`) процесс `maind --platformd` циклически падает на старте ещё до инициализации логгера по цепочке из 3 причин в `guest-init/equasar-qemu-init.sh`:
1. **Раздельные `tmpfs` для `/run` и `/var/run` (`equasar-qemu-init.sh:14-15`) и отсутствие `dbus-daemon`**:
   `equasar-qemu-init.sh` монтирует два независимых `tmpfs` на `/run` и `/var/run` и запускает `dbus-daemon` только для `cucumber`/`orion` в самом конце скрипта (после `onboot.sh`). В результате `sdbus-c++` внутри `maind --platformd` не находит `/run/dbus/system_bus_socket`:
   `libc++abi: terminating due to uncaught exception of type sdbus::Error: [org.freedesktop.DBus.Error.FileNotFound] Failed to open bus (No such file or directory)`.
2. **Сброшенные права setuid на `/usr/libexec/dbus-daemon-launch-helper`**:
   Даже при запущенном `dbus-daemon --system` и объединённом `/run/dbus` бинарник `/usr/libexec/dbus-daemon-launch-helper` в образе имеет права `0755` `0:0` вместо `04750` `0:85` (`root:dbus`), из-за чего автозапуск D-Bus сервисов падает с:
   `sdbus::Error: [org.freedesktop.DBus.Error.Spawn.PermissionsInvalid] The permission of the setuid helper is not correct`.
3. **Зависимость `maind --platformd` от `org.bluez` на системной шине D-Bus**:
   После исправления прав `dbus-daemon-launch-helper` пытается запустить `/usr/libexec/bluetooth/bluetoothd -d` (из `/usr/share/dbus-1/system-services/org.bluez.service`), который немедленно завершается из-за отсутствия Bluetooth HCI в ядре QEMU, а `maind --platformd` падает с:
   `sdbus::Error: [org.freedesktop.DBus.Error.NoReply] Message recipient disconnected from message bus without replying`.

#### 4. Рассинхронизация пути логов `/data/quasar/daemons_logs/maind.log` и `/tmp/daemons_logs/maind.log`
Во всех 11 прошивках секция `"patterns"` в `/system/vendor/quasar/quasar.cfg` задаёт:
```json
"DATA" : "/data/quasar",
"LOGS" : "${DATA}/daemons_logs"
```
То есть настоящий `maind` всегда пишет логи в `/data/quasar/daemons_logs/maind.log` (на живой колонке `10.1.1.56:5555` скрипт `/etc/init.d/S100privacy` заменяет `/data/quasar/daemons_logs` симлинком на `/tmp/daemons_logs`).
В `guest-init/equasar-qemu-init.sh:75` оба пути (`/tmp/daemons_logs` и `/data/quasar/daemons_logs`) создаются как **две разные папки**:
- в `stub`-режиме `qemu-hw-bridge` (`MAIND_LOG_PATH = "/tmp/daemons_logs/maind.log"` в `hw-bridge/src/state.rs:12`) пишет только в `/tmp/daemons_logs/maind.log`, оставляя `/data/quasar/daemons_logs/` пустым;
- в `full`-режиме настоящий `maind` пишет в `/data/quasar/daemons_logs/maind.log`, а `qemu-hw-bridge` читает пустой `/tmp/daemons_logs/maind.log`.

#### 5. Перезапись `LD_LIBRARY_PATH` внутри `/system/vendor/quasar/q`
Функция `quasar_run()` в `/system/vendor/quasar/q` жёстко задаёт `LD_LIBRARY_PATH=/system/vendor/quasar/libs`, отбрасывая `/bkupgrade/data/opt/lib` (куда `ensure_alsa_compat_lib()` в `hw-bridge/src/audio.rs:201` записывает пропатченную `libasound.so.2` с восстановленными версиями символов `ALSA_0.9` / `ALSA_0.9.0rc4`). При прямом вызове `maind` с `LD_LIBRARY_PATH` из `equasar-qemu-init.sh` (где `/usr/lib` идёт после `/system/vendor/quasar/libs`, но `/system/vendor/quasar/libs/libasound.so.2` не имеет секции `VERDEF`) динамический линкер выводит в `stderr`:
`/system/vendor/quasar/maind: /usr/lib/libasound.so.2: no version information available (required by /system/vendor/quasar/maind)`

#### 6. Аварийное завершение `q wifi-info` (`network device wlan0 does not exists`)
При запуске без `--lan-tap` в гостевой ОС существует только бутафорский файл `/sys/class/net/wlan0/address` (`equasar-qemu-init.sh:356`), но нет сетевого интерфейса `wlan0` в ядре. Вызов `/system/vendor/quasar/q wifi-info` (`maind --wifi-info`) завершается `SIGABRT`:
`libc++abi: terminating due to uncaught exception of type std::runtime_error: network device wlan0 does not exists`

#### 7. Падение `clock_led_util` (`Json::LogicError` в `ClockLedController`)
Вызов `/system/vendor/quasar/clock_led_util` на `bergamot`, `yandexmini2`, `plum` падает при инициализации `ClockLedController.<board>`:
`libc++abi: terminating due to uncaught exception of type Json::LogicError: in Json::Value::find(begin, end): requires objectValue or nullValue`

#### 8. Ошибки `fnvram` и пустой `Device ID` на `yandexmini`
На `yandexmini` (`1.266.4.40`) бинарник `maind` при любом вызове (`q version`, `q device-id`, `q full-info`, `q save-last-monotonic-time`) обращается через `libnvram.so` / `libwmdb.so` к `/bkupgrade/new_nvram_config_status`, `/bkupgrade/new_nvram_config` и `/dev/nvram` (которые очищаются в `sanitize_tree`), выводя:
```text
fnvram status file not found!
wm_db_fnvram_get mv_uuid failed
Device ID:
```

#### 9. Шум `stderr` от `/usr/bin/yandex-wifichipmodel` и `pcba_ids` при любом запуске `maind`
На всех прошивках `0.13+` / `0.369+` / `0.373+` инициализация `maind` запускает `/usr/bin/yandex-wifichipmodel`, который читает `/sys/bus/sdio/devices/*/vendor` и `/sys/bus/sdio/devices/*/device`. Из-за отсутствия узлов `/sys/bus/sdio/devices/*` в `equasar-qemu-init.sh` каждый вызов `q version`, `q device-id`, `q getprop`, `led_util` выводит 6–8 строк ошибок `cat: can't open '/sys/bus/sdio/devices/*/vendor': No such file or directory`, а на платах без `/sys/devices/platform/pcba_ids/pcba_ids_drv/pcba_ids` (`yandexmini2*`, `plum*`, `station2`, `yandexmicro`, `cucumber`) — предупреждение `[warning] [pcba_ids.cpp:15] PcbaIds node doesn't exist`.
