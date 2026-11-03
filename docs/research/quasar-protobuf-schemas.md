# Исследование Protobuf-схем прошивок Quasar (`v3rt3p/quasar-protos` и дампы бинарников `1.266` / `0.13` / `0.369` / `0.373`)

>[!note]
>Исследование выполнено по первичным источникам без модификации исходного кода репозитория: свежему клону [`v3rt3p/quasar-protos`](https://github.com/v3rt3p/quasar-protos) (ревизия `047f930` от `2026-10-31`), инструментам [`nitrog0d/ProtoDumper`](https://github.com/nitrog0d/ProtoDumper) и [`v3rt3p/protodump`](https://github.com/v3rt3p/protodump) (ветка `feat/metadata`, ревизия `35374a4` от `2026-10-31`), а также ELF-бинарникам (`/system/vendor/quasar/maind`, `quasar_launcher2`, `updater_switcher`, `bulbik/bulbik`, `begemot/begemot`, `libs/libbegemot.so`), извлечённым из всех 11 гостевых образов `work/*-system.ext4` (включая дизассемблирование `MessageLite`-таблиц виртуальных методов `_InternalParse` и `_InternalSerialize` через `capstone`). Все временные репозитории и дампы сохранены в `C:\Users\Georgiy\AppData\Local\Temp\opencode\proto-research\`.

---

## 1. Анализ репозитория `v3rt3p/quasar-protos` и инструментария дампа

### 1.1. История коммитов `v3rt3p/quasar-protos` и представленные прошивки

В репозитории `v3rt3p/quasar-protos` (автор — Егор Коледа, `v3rt3p` / `radioegor146`) представлены три хронологических среза Protobuf-схем Quasar:

| Коммит | Дата (ISO) | Изменения (`--stat`) | Источник схем и ключевые изменения |
| :--- | :--- | :--- | :--- |
| `190bb14` | `2025-07-23` | `487 files changed, 32214 insertions(+)` | Первичный дамп `FileDescriptorProto` из `/system/vendor/quasar/maind` Яндекс Станции 2 (`station2`, поколение лета 2025 г., близкое к `orion 0.13.1.36`). Монолитный `QuasarMessage` со вложенными полями `1..245` (`remoting = 210`). |
| `5edc301`, `2b866ae` | `2026-04-21` | `296 files changed, 3611(+), 9523(-)` и `26 files changed, 976(+), 56(-)` | Обновление весенней прошивки 2026 г.: удалены `mssngr/router/*`, `calld_state_* (139..142)`, в `QuasarMessage` добавлены поля `246..251` (`database_metrica_message = 246`, `audio_storage_event = 247`, `vqe_zone = 248`, `wifi_reassociate = 249`, `rss_status = 250`, `equalizer_params = 251`), добавлены `smart_devices/libs/zigbee/*`. |
| `1044fbd` | `2026-10-21` | `1 file changed, 10(+)` | Добавлен файл лицензии `LICENSE`. |
| `560d873` | `2026-10-31 04:43` | `537 files changed, 23326(+), 6140(-)` | Слияние дампов из 3 прошивок осени 2026 г.: **Яндекс Станция 2** (`yandexmidi/0.368.1.5.4738413199.20261002.294`), **Яндекс Станция Миди** (`cucumber/0.368.1.10.4679338126.20260921.159`) и **Яндекс Станция Дуо Макс** (`chiron/5000001364`). Переход `QuasarMessage` на конверт `payload = 252` / `type = 253`, добавление `GlagolQuasarMessage`, `alice/iot/bulbik/protos/*` и корневых файлов `quark.*`. |
| `4cf6352`, `ef42630`, `ae219f0` | `2026-10-31 05:01–05:05` | `12 files`, `3 files`, `2 files` | Добавление стабов недостающих импортов и фиксы компиляции `protoc`. |
| `047f930` (`HEAD`) | `2026-10-31 17:14` | `646 files changed, 28410(+), 10414(-)` | Полная регенерация `.proto` с сохранением кастомных опций `EnumValueOptions` (`extend .google.protobuf.EnumValueOptions { optional string payload_type = 58600; }`), флагов `[deprecated = true]`, `[packed = true]`, `json_name`, `(.NYT.column_name)` и `reserved`-диапазонов в `enum`. |

### 1.2. Различие между `nitrog0d/ProtoDumper` и `v3rt3p/protodump`

>[!warning]
>В первоначальном `README.md` коммита `190bb14` (`v3rt3p/quasar-protos`) была указана ссылка `Extracted with [ProtoDumper](https://github.com/nitrog0d/ProtoDumper.git) from /system/vendor/quasar/maind binary` (удалена автором в коммите `560d873`). Проверка исходного кода `nitrog0d/ProtoDumper` (`Program.cs`, `ProtoParser.cs`, `ProtoDumper.csproj`) показала, что **`nitrog0d/ProtoDumper` — это C#-утилита на базе `Mono.Cecil` для дампа управляемых `.dll`-сборок Unity IL2CPP (`Assembly-CSharp.dll`)**, которая ищет наследников C#-класса `Google.Protobuf.MessageBase` и не способна читать нативные ELF-бинарники C++.

Фактическим инструментом, которым были извлечены схемы для `v3rt3p/quasar-protos`, является репозиторий того же автора **[`v3rt3p/protodump`](https://github.com/v3rt3p/protodump)** (Go-форк утилиты [`arkadiyt/protodump`](https://github.com/arkadiyt/protodump)). В ветке `feat/metadata` (коммит `35374a4` от `2026-10-31 03:08:12 +0300`, сделанный за 1.5 часа до коммита `560d873` в `quasar-protos`) автор `radioegor146` доработал генератор `pkg/protodump/proto.go` для восстановления `map<K, V>` вместо синтетических `MapEntry`-сообщений и сохранения `json_name`. Утилита сканирует ELF-файл по сигнатуре `.proto` (`scan.go`), находит начало сериализованной структуры `google.protobuf.FileDescriptorProto` (байт `0x0a` — тег поля `1 name`) и десериализует встроенные в `.rodata` дескрипторы Protobuf.

### 1.3. Структура каталогов `v3rt3p/quasar-protos`

В актуальном `HEAD` (`047f930`) содержатся **653 `.proto`-файла**, разделённых на 7 основных подсистем:
1. **`yandex_io/protos/`** — ядро межпроцессного взаимодействия (IPC) демонов Quasar:
   - `quasar_proto.proto` (`package quasar.proto`) — главный транспортный контейнер `QuasarMessage` и системные сообщения (`WifiConnect`, `MediaMessage`, `MetricaMessage`, `SimplePlayerState`, `LogMessage`, `NsdEnable`, `BrickState`, `NetworkStats`, `UpdateHealthCheck`, `EqualizerParameters`).
   - `remoting.proto` (`package quasar.proto`) — RPC-мультиплексор `Remoting` (27 методов в `oneof method`) для взаимодействия с возможностями (`AliceCapabilityMethod`, `DeviceStateCapabilityMethod`, `PlaybackControlCapabilityMethod`, `FilePlayerCapabilityMethod`, `SpotterCapabilityMethod`, `VocalizerCapabilityMethod`, `DirectiveSequencerMethod`, `EndpointStorageMethod`, `EndpointMethod`, `AudioPlayerMethod`).
   - `alice_state.proto` (`package quasar.proto`) — состояние голосового ассистента `AliceState` (`State`, `Error`, `TextState`, `ResponseState`, `RequestState`) и `VinsResponse`.
   - `yandex_io.proto` (`package quasar.proto`) — сообщения шины `IOHub` и аудиоконвейера: `IOEvent` (`SDKState`, `Spectrum`, `BluetoothState`, `AlarmsState`), `IOControlCommand`, `IOHubMessage`, `IOAudioChannelData`, `IOAudioChannelsData`, `IOChannelRequest`, `ExternalDirectives`.
   - `model_objects.proto` (`package quasar.proto`, 2070 строк) — доменные модели (`AppState`, `MediaRequest`, `Audio`, `Radio`, `Video`, `GlagoldState`, `GlagolDiscoveryItem`, `GlagolDiscoveryRequest`, `GlagolDiscoveryResult`, `GlagolClusterMessage`, `GlagolClusterSign`, `MultiroomBroadcast`, `MultiroomDirective`, `MultiroomState`, `StereoPair`, `VolumeState`, `VolumeSetMessage`, `VolumeManager`, `Alice`, `UserConfig`, `AccountDevicesList`, `LedScreenCommand`).
   - `capabilities/device_state_capability.proto` (`package NAlice`) — proto3-модель `TDeviceStateCapability`.
2. **`yandex_io/sdk/`**:
   - `private/endpoint_storage/protos/endpoint.proto` — `Endpoint`, `Capability`, `DirectiveHandler`, `enum AudioChannel`.
   - `directive_sequencer/protos/directive.proto` — `Directive` (поля `1..31`, `oneof payload_type { string json_payload = 6; bytes raw_payload = 25; }`).
3. **`yandex_io/services/glagold/endpoints/protos/protocol.proto` и `yandex_io/interfaces/glagol/protos/glagol_quasar_message.proto`**:
   - `GlagolEndpointProtocol` (`oneof message`, поля `1..11`) и сетевой конверт `GlagolQuasarMessage` (поля `1, 128, 148, 152, 182, 198, 204`).
4. **`alice/protos/`** — общие контракты экосистемы Алисы (`NAlice`):
   - `alice/protos/endpoint/capability.proto`, `any_capability.proto`, `endpoint.proto` и свыше 150 файлов `alice/protos/endpoint/capabilities/*/capability.proto` (`alice`, `audio_player`, `audio_file_player`, `playback_control`, `volume`, `equalizer`, `color`, `color_animation`, `led_mask`, `clock_app`, `multiroom`, `stereo_pair`, `local_iot`, `iot_discovery`, `iot_scenarios` и др.).
   - `alice/protos/api/alicekit/` — протокол UniProxy / AliceKit (`TAliceRequest`, `TAliceResponseProto`, `TDeviceState`, `TEnvironmentState`, `TDirective`, `TCard`, `TSuggest`, `events/*`).
5. **`alice/megamind/protos/`** — фреймы сценариев (`music_frame`, `media_frame`, `iot_frame`, `radio_frame`), `TExperimentsProto`, `TEnrollmentHeaders`, `common/iot.proto` (3303 строки).
6. **`alice/iot/bulbik/protos/` и `smart_devices/libs/{matter,zigbee,zenoh}/`** — схемы локального хаба Умного дома `bulbik` (`request.proto`, `response.proto`, модели устройств Zigbee и Matter).
7. **Корневые файлы `*.proto` (`package quark.core`, `quark.core.proto`, `quark.android.proto`)** — `quark_config.proto`, `alice_state_compact.proto`, `alice_capability_compact.proto`, `io_raw_audio.proto`, извлечённые из Android-оболочки `Quark` на Станции Дуо Макс (`chiron/5000001364`).

---

## 2. Результаты дампа бинарников из `work/*-system.ext4` по поколениям прошивок

Из всех 11 образов `D:\Project\kuyase\work\*-system.ext4` были извлечены исполняемые файлы и разделяемые библиотеки из `/system/vendor/quasar/` и обработаны `protodump` (в каталог `C:\Users\Georgiy\AppData\Local\Temp\opencode\proto-research\dumped\<board>\`), а для бинарников с `LITE_RUNTIME` дополнительно выполнен дизассемблерный дамп таблиц виртуальных методов (`vtable`) и парсеров `_InternalParse`.

### 2.1. Сводная матрица бинарников и дескрипторов по всем 11 платам

| Профиль платы | Версия прошивки (`version_full`) | Бинарник в `/system/vendor/quasar/` | Архитектура и размер | Рантайм Protobuf в бинарнике | Сдамплено `.proto` (`FileDescriptorProto`) | Классов `quasar::proto` в RTTI / `GetTypeName` |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`yandexmini`** | `1.266.4.40.2633723173.20241021` | `maind`<br>`quasar_launcher2` | ELF32 (`26 395 308 Б`)<br>ELF32 (`289 012 Б`) | Full (`Message` + `TextFormat`)<br>Нет Protobuf | **`396` файлов** (`30` в `yandex_io/`)<br>`0` | `444` RTTI<br>`0` |
| **`orion`** | `0.13.1.36.3322101745.20250823.factory-13` | `maind`<br>`bulbik/bulbik`<br>`begemot/begemot`<br>`quasar_launcher2`<br>`updater_switcher` | ELF64 (`49 598 840 Б`)<br>ELF64 (`258 103 904 Б`)<br>ELF64 (`51 007 048 Б`)<br>ELF64 (`601 664 Б`)<br>ELF64 (`497 000 Б`) | Full (`Message` + `TextFormat`)<br>Full (`Message`)<br>Full (`Message`)<br>Нет Protobuf<br>Нет Protobuf | **`508` файлов** (`34` в `yandex_io/`)<br>**`980` файлов**<br>**`358` файлов**<br>`0`<br>`0` | `467` RTTI<br>Схемы `NAlice`/`iot`<br>Схемы `NBg`<br>`0`<br>`0` |
| **`yandexmini2`** | `0.369.1.25.4712742930.20260928.299` | `maind`<br>`quasar_launcher2` | ELF32 (`21 929 376 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `488` RTTI / `352` `GetTypeName` |
| **`yandexmicro`** | `0.369.1.46.4778262442.20261013.297` | `maind`<br>`quasar_launcher2` | ELF32 (`21 819 288 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `488` RTTI / `352` `GetTypeName` |
| **`yandexmini2nc`** | `0.369.1.50.4794659041.20261021.299` | `maind`<br>`quasar_launcher2` | ELF32 (`21 949 504 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `488` RTTI / `352` `GetTypeName` |
| **`plum`** | `0.369.1.50.4795647582.20261021.133` | `maind`<br>`quasar_launcher2` | ELF32 (`22 239 424 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `488` RTTI / `352` `GetTypeName` |
| **`plumnc`** | `0.369.1.50.4794654278.20261021.133` | `maind`<br>`quasar_launcher2` | ELF32 (`22 239 424 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `488` RTTI / `352` `GetTypeName` |
| **`mango`** | `0.369.1.50.4794659377.20261021.93` | `maind`<br>`quasar_launcher2` | ELF32 (`18 694 000 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `488` RTTI / `352` `GetTypeName` |
| **`station2`** | `0.369.1.50.4794649087.20261021.295` | `maind`<br>`quasar_launcher2` | ELF32 (`24 302 016 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `489` RTTI / `352` `GetTypeName` |
| **`cucumber`** | `0.369.1.50.4794663100.20261021.160` | `maind`<br>`bulbik/bulbik`<br>`libs/libbegemot.so`<br>`quasar_launcher2` | ELF32 (`27 618 596 Б`)<br>ELF32 (`128 695 960 Б`)<br>ELF32 (`55 843 660 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Full (`Message`)<br>Full (`Message`)<br>Нет Protobuf | `0` (в `maind`)<br>**`1479` файлов**<br>**`356` файлов**<br>`0` | `489` RTTI / `352` `GetTypeName`<br>Схемы `NAlice`/`iot`<br>Схемы `NBg`<br>`0` |
| **`bergamot`** | `0.373.1.13.4818984688.20261030.121` | `maind`<br>`quasar_launcher2` | ELF32 (`21 323 228 Б`)<br>ELF32 (`317 944 Б`) | `LITE_RUNTIME` (`MessageLite`)<br>Нет Protobuf | `0` (дескрипторы вырезаны)<br>`0` | `489` RTTI / `351` `GetTypeName` |

### 2.2. Переход `maind` на `LITE_RUNTIME` в `0.369.1.*` и `0.373.1.13`

1. В поколениях **`1.266.4.40` (`yandexmini`)** и **`0.13.1.36` (`orion`)** бинарник `maind` скомпилирован со стандартным рантаймом `google::protobuf::Message` (в RTTI присутствуют `N6google8protobuf10TextFormat...`, а в `.rodata` встроены сериализованные `FileDescriptorProto`). При этом виртуальный метод `GetTypeName()` не хранит отдельных строковых литералов `quasar.proto.<Message>` в `.rodata`, а берёт их из `DescriptorPool`.
2. В поколениях **`0.369.1.*`** и **`0.373.1.13`** на всех Linux-колонках (`yandexmini2*`, `yandexmicro`, `plum*`, `mango`, `station2`, `cucumber`, `bergamot`) бинарник `maind` переведён на **`option optimize_for = LITE_RUNTIME`** (`google::protobuf::MessageLite`):
   - Из `maind` полностью исключены таблицы `FileDescriptorProto` и `TextFormat` (экономия более `20 МБ` оперативной памяти и места на разделе), поэтому стандартный `protodump` на `maind` версий `0.369` / `0.373` возвращает `0` файлов.
   - Вместо дескрипторов каждый сгенерированный класс хранит в `.rodata` плоский строковый литерал `"quasar.proto.<Message>"` для `MessageLite::GetTypeName()` (`352` строки в `0.369.1.*` и `351` в `0.373.1.13`) и полную таблицу виртуальных методов (`vtable`) с адресами `_InternalParse` (`vfunc[10]`), `_InternalSerialize`, `ByteSizeLong` (`vfunc[8]`) и `MergeFrom` (`vfunc[7]`).
   - В то же время демон Умного дома `/system/vendor/quasar/bulbik/bulbik` (`1479` `.proto`-файлов на `cucumber 0.369.1.50` против `980` на `orion 0.13.1.36`) и библиотека NLU `/system/vendor/quasar/libs/libbegemot.so` (`356` `.proto`-файлов) продолжают собираться с полными `FileDescriptorProto`, а схемы `yandex_io/protos/*` с полными дескрипторами в `v3rt3p/quasar-protos` (`047f930`) были взяты автором из сборки `0.368.1.*` / Android-прошивки `chiron/5000001364` (Дуо Макс).

---

## 3. Почему изменились номера полей в `QuasarMessage` (`210 -> 252/253`) и что такое поля `252` и `253`

### 3.1. Причина рефакторинга `QuasarMessage` между `1.266`/`0.13` и `0.368`/`0.369`/`0.373`

В прошивках **`1.266.4.40` (`yandexmini`)**, **`0.13.1.36` (`orion`)** и весенней сборке 2026 г. (коммит `5edc301` в `quasar-protos`) сообщение `quasar.proto.QuasarMessage` представляло собой гигантскую структуру-объединение из **125+ опциональных полей вложенных сообщений** (номера полей от `1` до `245` в `0.13.1.36` и до `251 equalizer_params` в `5edc301`):

```protobuf
// Старая схема QuasarMessage (1.266.4.40 и 0.13.1.36):
message QuasarMessage {
  optional string request_id = 1;
  optional .quasar.proto.AliceState alice_state = 3;
  optional .quasar.proto.MediaRequest media_request = 8;
  optional .quasar.proto.MediaMessage media_message = 27;
  optional .quasar.proto.AppState app_state = 33;
  optional .quasar.proto.VolumeState volumeState = 42;
  optional .quasar.proto.VolumeSetMessage volume_set_message = 78;
  optional .quasar.proto.GlagoldState glagold_state = 82;
  optional .quasar.proto.SimplePlayerState simple_player_state = 84;
  optional .quasar.proto.IOEvent io_event = 85;
  optional .quasar.proto.IOControlCommand io_control = 88;
  optional .quasar.proto.AudioClientEvent audio_client_event = 127;
  optional string service_name_ready = 157;
  optional .quasar.proto.IOAudioChannelsData io_audio_channels_data = 158;
  optional .quasar.proto.IOChannelRequest io_channel_request = 159;
  optional .quasar.proto.GlagolClusterMessage glagol_cluster_message = 181;
  optional .quasar.proto.GlagolClusterSign glagol_cluster_sign = 182;
  optional .quasar.proto.IOHubMessage io_hub_message = 183;
  optional .quasar.proto.GlagolDiscoveryRequest glagol_discovery_request = 185;
  optional .quasar.proto.EmptyMessage glagol_discovery_updated = 186;
  optional .quasar.proto.GlagolDiscoveryResult glagol_discovery_result = 187;
  optional .quasar.proto.Remoting remoting = 210;
  optional .quasar.proto.Directive directive = 211;
  optional .quasar.proto.ConfigCapabilityEvents config_capability_events = 236;
  optional .quasar.proto.ConfigCapabilitySubscriptions config_capability_subscriptions = 237;
  // ... вплоть до 245 (в 0.13.1.36) и 251 (в 5edc301)
}
```

У такой архитектуры было два критических недостатка:
1. **Связанность на уровне компиляции и размер объекта в памяти**: заголовок `quasar_proto.pb.h` транзитивно тянул за собой определения всех ~125 типов сообщений прошивки, а каждый экземпляр `QuasarMessage` в C++ содержал свыше 120 указателей на подсообщения.
2. **Накладные расходы при маршрутизации IPC**: промежуточный IPC-роутер (`MixedIpcServer`, `IOHub`, `GlagolCluster`) не мог переслать или отфильтровать `QuasarMessage` по типу без полной десериализации вложенного дерева Protobuf-объектов.

Поэтому сразу после поля `251` (`equalizer_params`) в поколении **`0.368` / `0.369` / `0.373`** разработчики Quasar зарезервировали все старые поля полезной нагрузки (`reserved 3..251`, кроме метаданных `1`, `182`, `236`, `237`) и заменили их **типобезопасным конвертом из двух новых полей `252` и `253`**:

```protobuf
extend .google.protobuf.EnumValueOptions {
  optional string payload_type = 58600;
}

message QuasarMessage {
  // Все старые поля (alice_state, remoting, io_control и др.) помечены как reserved:
  reserved 2 to 181, 183 to 235, 238 to 251, 254 to 18999, 20000 to max;

  enum Type {
    TYPE_UNSPECIFIED = 0;
    TYPE_ALICE_STATE = 3 [(.quasar.proto.payload_type) = "quasar.proto.AliceState"];
    TYPE_MEDIA_REQUEST = 8 [(.quasar.proto.payload_type) = "quasar.proto.MediaRequest"];
    TYPE_MEDIA_MESSAGE = 27 [(.quasar.proto.payload_type) = "quasar.proto.MediaMessage"];
    TYPE_APP_STATE = 33 [(.quasar.proto.payload_type) = "quasar.proto.AppState"];
    TYPE_VOLUME_STATE = 42;
    TYPE_VOLUME_SET_MESSAGE = 78 [(.quasar.proto.payload_type) = "quasar.proto.VolumeSetMessage"];
    TYPE_GLAGOLD_STATE = 82 [(.quasar.proto.payload_type) = "quasar.proto.GlagoldState"];
    TYPE_SIMPLE_PLAYER_STATE = 84 [(.quasar.proto.payload_type) = "quasar.proto.SimplePlayerState"];
    TYPE_IO_EVENT = 85 [(.quasar.proto.payload_type) = "quasar.proto.IOEvent"];
    TYPE_IO_CONTROL = 88 [(.quasar.proto.payload_type) = "quasar.proto.IOControlCommand"];
    TYPE_ON_MEDIA_ERROR = 124 [(.quasar.proto.payload_type) = "quasar.proto.OnMediaErrorHolder"];
    TYPE_AUDIO_CLIENT_EVENT = 127 [(.quasar.proto.payload_type) = "quasar.proto.AudioClientEvent"];
    TYPE_SERVICE_NAME_READY = 157 [(.quasar.proto.payload_type) = "std::string"];
    TYPE_IO_AUDIO_CHANNELS_DATA = 158 [(.quasar.proto.payload_type) = "quasar.proto.IOAudioChannelsData"];
    TYPE_IO_CHANNEL_REQUEST = 159 [(.quasar.proto.payload_type) = "quasar.proto.IOChannelRequest"];
    TYPE_GLAGOL_CLUSTER_MESSAGE = 181 [(.quasar.proto.payload_type) = "quasar.proto.GlagolClusterMessage"];
    TYPE_IO_HUB_MESSAGE = 183 [(.quasar.proto.payload_type) = "quasar.proto.IOHubMessage"];
    TYPE_GLAGOL_DISCOVERY_REQUEST = 185 [(.quasar.proto.payload_type) = "quasar.proto.GlagolDiscoveryRequest"];
    TYPE_GLAGOL_DISCOVERY_UPDATED = 186 [(.quasar.proto.payload_type) = "void"];
    TYPE_GLAGOL_DISCOVERY_RESULT = 187 [(.quasar.proto.payload_type) = "quasar.proto.GlagolDiscoveryResult"];
    TYPE_REMOTING = 210 [(.quasar.proto.payload_type) = "quasar.proto.Remoting"];
    TYPE_DIRECTIVE = 211 [(.quasar.proto.payload_type) = "quasar.proto.Directive"];
    // ... всего 132 значения (0..257) в 0.369.1.* и 135 значений (0..260) в 0.373.1.13
  }

  optional string request_id = 1;
  optional bytes payload = 252;
  optional .quasar.proto.QuasarMessage.Type type = 253;
  optional .quasar.proto.GlagolClusterSign glagol_cluster_sign = 182;
  optional .quasar.proto.ConfigCapabilityEvents config_capability_events = 236;
  optional .quasar.proto.ConfigCapabilitySubscriptions config_capability_subscriptions = 237;
}
```

### 3.2. Как именно работают поля `252` (`payload`) и `253` (`type`) на проводе

Дизассемблирование `QuasarMessage::_InternalParse` (`0xa8116e` в `station2 0.369.1.50`, `0x99f03a` в `bergamot 0.373.1.13`) и `QuasarMessage::_InternalSerialize` (`0xa812fe` в `station2`) подтверждает, что во всех прошивках `0.369.1.*` и `0.373.1.13` класс `QuasarMessage` имеет размер всего `40 байт` (`0x28`) и обрабатывает ровно **6 полей**: `[1, 182, 236, 237, 252, 253]`:

| Поле `QuasarMessage` в `0.369` / `0.373` | Номер поля | Wire Type | Wire Tag (байты) | Смещение в C++-объекте (`ARM32`) | Назначение |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `request_id` | `1` | `2` (Len) | `0x0a` | `[this + 0x10]` (`ArenaStringPtr`) | Идентификатор запроса IPC |
| `glagol_cluster_sign` | `182` (`0xb6`) | `2` (Len) | `0xb2 0x0b` | `[this + 0x18]` (`GlagolClusterSign*`) | Подпись отправителя `from_device_id` при пересылке в кластере Glagol |
| `config_capability_events` | `236` (`0xec`) | `2` (Len) | `0x62 0x0f` | `[this + 0x1c]` (`ConfigCapabilityEvents*`) | События `config_capability` (`:9882`) |
| `config_capability_subscriptions` | `237` (`0xed`) | `2` (Len) | `0x6a 0x0f` | `[this + 0x20]` (`ConfigCapabilitySubscriptions*`) | Подписки `config_capability` (`:9882`) |
| **`payload`** | **`252` (`0xfc`)** | `2` (Len) | **`0xe2 0x0f`** | `[this + 0x14]` (`ArenaStringPtr`) | Сырые байты сериализованного подсообщения (например, `Remoting`), `std::string` или пусто для `"void"` |
| **`type`** | **`253` (`0xfd`)** | `0` (Varint) | **`0xe8 0x0f`** | `[this + 0x24]` (`int32_t`) | Дискриминатор `QuasarMessage::Type`, **равный старому номеру поля `QuasarMessage`** (проверяется через `QuasarMessage_Type_IsValid`) |

**Пример сравнения одного и того же пакета `Remoting` на проводе**:
- В **`1.266.4.40` (`yandexmini`)** и **`0.13.1.36` (`orion`)**:
  - `QuasarMessage` содержит поле `210` (`wire tag = (210 << 3) | 2 = 0x0692` -> varint `92 0d`):
  - На проводе: `92 0d <len> <Remoting bytes>`
- В **`0.369.1.*` (`yandexmini2*`, `yandexmicro`, `plum*`, `mango`, `station2`, `cucumber`)** и **`0.373.1.13` (`bergamot`)**:
  - `QuasarMessage` содержит поле `252` (`wire tag = (252 << 3) | 2 = 0x07e2` -> varint `e2 0f`) с байтами `Remoting` и поле `253` (`wire tag = (253 << 3) | 0 = 0x07e8` -> varint `e8 0f`) со значением `210` (`TYPE_REMOTING`, varint `d2 01`):
  - На проводе: `e2 0f <len> <Remoting bytes> e8 0f d2 01`

Особые случаи упаковки в `payload = 252`:
1. **`(.quasar.proto.payload_type) = "void"`** (ранее `EmptyMessage`, например `TYPE_GLAGOL_DISCOVERY_UPDATED = 186`, `TYPE_RESET_WIFI = 60`, `TYPE_WIFI_REPORT = 252`): поле `payload` (`252`) либо опускается, либо имеет длину `0`, передаётся только `type` (`253`).
2. **`(.quasar.proto.payload_type) = "std::string"`** (`TYPE_SERVICE_NAME_READY = 157`, `TYPE_VQE_ZONE = 248`): в поле `payload` (`252`) записываются непосредственно UTF-8 байты строки без дополнительной Protobuf-обёртки.
3. **Замена голых `enum`-полей на сообщения-обёртки (`Holder`)**: в старом `QuasarMessage` поля `98` (`ConfigurationState configuration_state = 98`) и `124` (`MediaContentType on_media_error = 124`) имели тип `enum` (wire type `0`), поэтому при переходе к `bytes payload = 252` для них были введены однополевые сообщения-обёртки `ConfigurationStateHolder { optional ConfigurationState state = 1; }` и `OnMediaErrorHolder { optional MediaContentType media_content_type = 1; }`.

### 3.3. Обратная совместимость по сети: зачем появилось `GlagolQuasarMessage`

Так как колонки разных поколений (например, `yandexmini 1.266.4.40` и `station2 0.369.1.50`) могут объединяться в мультирум или кластер Glagol и пересылать друг другу `QuasarMessage` внутри `GlagolClusterMessage.quasar_message_base64`, отправка нового конверта `252/253` на старую колонку сломала бы декодирование. Для решения этой проблемы в `0.368` / `0.369` / `0.373` было выделено отдельное сообщение `quasar.proto.GlagolQuasarMessage` (`yandex_io/interfaces/glagol/protos/glagol_quasar_message.proto`), сохраняющее **старые номера полей** на проводе:

```protobuf
message GlagolQuasarMessage {
  optional string request_id = 1;
  optional bytes multiroom_broadcast = 128;
  optional bytes multiroom_directive = 148;
  optional bytes multiroom_state = 152;
  optional bytes glagol_cluster_sign = 182;
  optional bytes stereo_pair_message = 198;
  optional bytes alice_message = 204;
}
```

### 3.4. Эволюция `QuasarMessage` и `Remoting` от `1.266.4.40` до `0.373.1.13`

| Поколение прошивки | Представители (`work/*-system.ext4`) | Структура `QuasarMessage` | Макс. номер поля / `Type` в `QuasarMessage` | Ключевые отличия схемы от предыдущего поколения |
| :--- | :--- | :--- | :--- | :--- |
| **Gen 1 (`1.266.4.40`)** | `yandexmini` (`2024-10-21`) | Монолитная (`124` поля вложенных сообщений) | Поле `243` (`issue_auth_code_response`) | Присутствуют `messenger_send_heartbeat = 139`, `calld_state_request = 140`, `calld_state_response = 141`, `calld_state_changed = 142`, `current_weather = 53`, `features_config_message = 197`, `call_message = 205`. |
| **Gen 2 (`0.13.1.36`)** | `orion` (`2025-08-23`) | Монолитная (`122` поля вложенных сообщений) | Поле `245` (`force_check_updates`) | Зарезервированы `139..142` (`calld_state_*`), добавлены `update_health_check = 244`, `force_check_updates = 245`; в `MetricaMessage` добавлено `timestamp = 25`; в `SimplePlayerState` поле `extra = 100` оформлено как `map<string, string>`. |
| **Gen 3 (`0.369.1.*`)** | `yandexmini2` (`0.369.1.25`), `yandexmicro` (`0.369.1.46`), `yandexmini2nc`, `plum`, `plumnc`, `mango`, `station2`, `cucumber` (`0.369.1.50`) | Конверт `request_id = 1`, `payload = 252`, `type = 253` (+ `182`, `236`, `237`) | Поле `253` (`type`); в `enum Type` **132 значения**, макс. `257` (`TYPE_REMOTE_AUDIO_SOURCE`) | Все старые поля перенесены в `enum QuasarMessage.Type`. Добавлены `TYPE_DATABASE_METRICA_MESSAGE = 246`, `TYPE_AUDIO_STORAGE_EVENT = 247`, `TYPE_VQE_ZONE = 248`, `TYPE_WIFI_REASSOCIATE = 249`, `TYPE_RSS_STATUS = 250`, `TYPE_EQUALIZER_PARAMS = 251`, `TYPE_WIFI_REPORT = 252`, `TYPE_MINIDUMP_STATUS = 254`, `TYPE_VINYL_JOG_WHEEL_EVENT = 255`, `TYPE_LOG_MESSAGES = 256`, `TYPE_REMOTE_AUDIO_SOURCE = 257`. |
| **Gen 3 (`0.373.1.13`)** | `bergamot` (`2026-10-30`) | Конверт `request_id = 1`, `payload = 252`, `type = 253` (+ `182`, `236`, `237`) | Поле `253` (`type`); в `enum Type` **135 значений**, макс. **`260`** | В `QuasarMessage.Type` (`QuasarMessage_Type_IsValid` по адресу `0x9979d8`) добавлены значения **`258`, `259`, `260`** (включая `TYPE_HOLD_UPDATE_APPLY` с новым сообщением `quasar.proto.UpdateApplyHold { optional uint64 timeout_seconds = 1; }`). Из `Remoting` исключены классы `VocalizerCapabilityMethod` (`21`) и `VocalizerCapabilityListenerMethod` (`22`); в `AppState` удалено поле `14` (`alice_video_state`). |

---

## 4. Детальная спецификация номеров полей ключевых сообщений

### 4.1. `quasar.proto.Remoting` (`yandex_io/protos/remoting.proto`)

Сообщение `Remoting` передаётся:
- в `1.266.4.40` / `0.13.1.36`: в поле `QuasarMessage.remoting = 210`;
- в `0.369.1.*` / `0.373.1.13`: в поле `QuasarMessage.payload = 252` при `QuasarMessage.type = 253` со значением `TYPE_REMOTING = 210`.

| Номер поля | Имя поля в `Remoting` | Тип | Присутствие по версиям (`1.266` / `0.13` / `0.369` / `0.373`) |
| :--- | :--- | :--- | :--- |
| `7` | `remote_object_id` | `optional string` | Все версии (`"AliceCapability"`, `"PlaybackControlCapability"`, `"DeviceStateCapability"`, `"EndpointStorage_"`, `"FilePlayerCapability"`, `"SpotterCapability"` и др.) |
| `25` | `process_id` | `optional string` | Все версии (идентификатор процесса-отправителя) |
| `1` | `endpoint_storage_method` (`oneof method`) | `EndpointStorageMethod` | Все версии (`id = 1`, `method = 2`: `ADD_ENDPOINT = 0`, `REMOVE_ENDPOINT = 1`, `SYNC_ENDPOINTS = 2`, `endpoints = 3`, `storage_name = 4`) |
| `2` | `directive_handler_method` (`oneof method`) | `DirectiveHandlerMethod` | Все версии (`method = 1`: `HANDLE = 0`, `CANCEL = 1`, `PREFETCH = 2`, `directive = 2`, `handler_name = 3`, `endpoint_id = 4`) |
| `3` | `capability_state_changed_method` (`oneof method`) | `CapabilityStateChangedMethod` | Все версии (`capability = 2`) |
| `6` | `endpoint_method` (`oneof method`) | `EndpointMethod` | Все версии (`method = 2`: `ADD_CAPABILITY = 0`, `REMOVE_CAPABILITY = 1`, `SYNC_STATE = 2`, `capability = 3`, `state = 4`, `id = 5`, `storage_name = 6`) |
| **`8`** | **`alice_capability_method`** (`oneof method`) | **`AliceCapabilityMethod`** | Все версии (управление диалогом, TTS, ASR, VINS — см. п. 4.2) |
| `9` | `alice_request_events_method` (`oneof method`) | `AliceRequestEventsMethod` | Все версии (`method = 1`, `request_id = 2`, `vins_response_json = 3`, `error_code = 4`, `error_text = 5`, с `0.13+` добавлено `vins_response_proto = 6`) |
| `10` | `capability_events_method` (`oneof method`) | `CapabilityEventsMethod` | Все версии (`repeated NAlice.TCapabilityEvent events = 1`) |
| `12` | `file_player_capability_method` (`oneof method`) | `FilePlayerCapabilityMethod` | Все версии (`method = 1`: `PLAY_SOUND_FILE = 0`, `STOP_SOUND_FILE = 1`, `HANDSHAKE_REQUEST = 2`, `file_path = 2`, `channel = 3`, `play_params = 4`, `send_events = 5`, `stored_sound = 6`; поле `file_name = 7` удалено в `0.368+`) |
| **`13`** | **`playback_control_capability_method`** (`oneof method`) | **`PlaybackControlCapabilityMethod`** | Все версии (транспортные команды плеера — см. п. 4.4) |
| `14` | `spotter_capability_method` (`oneof method`) | `SpotterCapabilityMethod` | Все версии (`method = 1`: `SET_MODEL_PATHS = 0`, `SET_SPOTTER_WORD = 1`, `START = 2`, `STOP = 3`, `ENABLE = 4`, `DISABLE = 5`, `model_path = 2`, `spotter_word = 3`) |
| `15` | `spotter_capability_listener_method` (`oneof method`) | `SpotterCapabilityListenerMethod` | Все версии (`method = 1`: `ON_MODEL_ERROR = 0`, `ON_MODEL_SET = 1`, `ON_PHRASE_SPOTTED = 2`, `spotter_type = 2`, `spotted_phrase = 3`, в `0.368+` добавлено `spotter_model_path = 4`) |
| **`16`** | **`device_state_capability_method`** (`oneof method`) | **`DeviceStateCapabilityMethod`** | Все версии (обновление `DeviceState`, `mics_muted`, `volume_state` — см. п. 4.3) |
| `17` | `file_player_capability_events_method` (`oneof method`) | `FilePlayerCapabilityEventsMethod` | Все версии (`directive_event = 1`, `handshake_response = 2`, в `0.368+` добавлено `audio_client_event = 3`) |
| **`18`** | **`alice_capability_listener_method`** (`oneof method`) | **`AliceCapabilityListenerMethod`** | Все версии (события от `aliced` к слушателям — см. п. 4.2) |
| `20` | `capability_add_listener_method` (`oneof method`) | `CapabilityAddListenerMethod` | Все версии (`capability_id = 1`) |
| `21` | `vocalizer_capability_method` (`oneof method`) | `VocalizerCapabilityMethod` | В `1.266`, `0.13`, `0.369`; **в `0.373.1.13` (`bergamot`) класс удалён из `maind`** |
| `22` | `vocalizer_capability_listener_method` (`oneof method`) | `VocalizerCapabilityListenerMethod` | В `1.266`, `0.13`, `0.369`; **в `0.373.1.13` (`bergamot`) класс удалён из `maind`** |
| `23` | `directive_sequencer_method` (`oneof method`) | `DirectiveSequencerMethod` | Все версии (`method = 1`, `directive = 2`, `directives = 3`, `result = 4`, `directive_handler = 5`, с `0.13+` добавлено `meta_data = 6`) |
| `24` | `directive_sequencer_listener_method` (`oneof method`) | `DirectiveSequencerListenerMethod` | Все версии (`method = 1`, `directive = 2`, `result = 3`, `directives = 4`, в `0.368+` добавлено `meta_data = 5`) |
| `26` | `alarm_capability_listener_method` (`oneof method`) | `AlarmCapabilityListenerMethod` | Только в `1.266.4.40` (зарезервировано начиная с `0.13.1.36`) |
| `28` | `stereo_pair_capability_method` (`oneof method`) | `StereoPairCapabilityMethod` | В `1.266.4.40` и `0.13.1.36` (зарезервировано в `0.368+`) |
| `29` | `lyrics_capability_method` (`oneof method`) | `LyricsCapabilityMethod` | В `1.266.4.40` и `0.13.1.36` (зарезервировано в `0.368+`) |
| `30` | `tandem_capability_listener_method` (`oneof method`) | `TandemCapabilityListenerMethod` | Все версии (`method = 1`, `state = 2`, в `0.368+` добавлено `follower_video = 3`) |
| `31` | `clock_storage_method` (`oneof method`) | `ClockStorageMethod` | Все версии (`method = 1`, `clocks = 2`, `clock_id = 3`) |
| `32` | `fast_command_requester_method` (`oneof method`) | `FastCommandRequesterMethod` | Все версии (`method = 1`, `string_command = 2`, `fast_command = 3`, `event_source = 4`, в `0.368+` добавлено `request_id = 5`) |
| `33` | `fast_command_requester_listener_method` (`oneof method`) | `FastCommandRequesterListenerMethod` | Все версии (`method = 1`, `string_command = 2`, `fast_command = 3`, `states = 4`, `request_id = 5`, `passed = 6`, `disabled_reason = 7`) |
| `34` | `timings_manager_method` (`oneof method`) | `TimingsManagerMethod` | Добавлено в `0.13.1.36` (`1..4`), расширено в `0.368+` (`all_meta = 5`, `sub_events = 7`, `sub_meta = 8`) |
| `35` | `routing_audio_player_method` (`oneof method`) | `RoutingAudioPlayerMethod` | Добавлено в `0.368+` / `0.369+` / `0.373+` (`method = 1`, `endpoint_id = 2`, `player_object_id = 3`) |
| `36` | `audio_player_method` (`oneof method`) | `AudioPlayerMethod` | Добавлено в `0.368+` / `0.369+` / `0.373+` (`method = 1`: `PLAY..SET_VOLUME`, `sound_info = 2`, `buffer = 3`, `event_source = 4`, `volume = 5`) |
| `37` | `audio_player_listener_method` (`oneof method`) | `AudioPlayerListenerMethod` | Добавлено в `0.368+` / `0.369+` / `0.373+` (`method = 1`: `ON_PLAYING_BEGIN..ON_BUFFER_UNDERRUN`, `sound_info = 2`, `buffer = 3`, `error_code = 4`, `error_message = 5`) |

### 4.2. `Remoting.AliceCapabilityMethod` (поле `8`) и `AliceCapabilityListenerMethod` (поле `18`)

Поля `Remoting.AliceCapabilityMethod` (целевой объект `remote_object_id = "AliceCapability"`):
- `1`: `optional Method method = 1;`
- `2`: `optional .quasar.proto.VinsRequest vins_request = 2;` (где `VinsRequest` в `model_objects.proto` содержит `id = 1`, `event_json = 2`, `is_Spotter = 3`, `is_parallel = 16`, `event_source = 17` и др.)
- `3`: `optional bool events = 3;`
- `4`: `optional string source_id = 4;`
- `5`: `optional .NAlice.TAudioFilePlayerCapability.EStoredSound error_sound = 5;`
- `6`: `optional .quasar.proto.VoiceServiceSettings voice_service_settings = 6;`
- `7`: `optional bool silent = 7;`
- `8`: `optional string uuid = 8;`
- `9`: `optional string cookies = 9;`
- `10`: `optional string request_id = 10;`
- `11`: `optional string uni_proxy_url = 11;`
- `12`: `optional string icookie = 12;`
- `13`: `optional .NAlice.TAliceCapability.TVocalizeDirective vocalize_directive = 13;` (в `alice/protos/endpoint/capabilities/alice/capability.proto`: `string Text = 1`, `EVocalizeMode VocalizeMode = 2`: `Append = 0`, `Interrupt = 1`, `SynthesizeOnly = 2`, `string Voice = 3`, `string Emotion = 4`, `string Language = 5`, `double Speed = 6`, `string Effect = 7`, `bool Forced = 8`)
- `14`: `optional string dialog_id = 14;`
- `15`: `optional .NAlice.NData.EDialogType dialog_type = 15;`
- `16`: `optional .NAlice.NApi.NAlice2Mode.TAlice2ModeInfo alice_2_settings = 16;` (добавлено в `0.13.1.36`)
- `17`: `optional .NAlice.TAliceCapability.TStartDataInputDirective start_data_input_directive = 17;` (добавлено в `0.13.1.36`)
- `18`: `optional string vins_url = 18;` (добавлено в `0.13.1.36`)
- `19`: `optional string folder_id = 19;` (добавлено в `0.13.1.36`)
- `20`: `optional .NAlice.NAliceApi.TAliceRequest alice_request = 20;` (добавлено в `0.368+` / `0.369+`)
- `21`: `optional bool force = 21;` (добавлено в `0.368+` / `0.369+`)
- `22`: `optional .NAlice.TFileUploadCapability.TStartFileUploadDirective start_file_upload_directive = 22;` (добавлено в `0.368+` / `0.369+`)
- `23`: `optional .NAlice.TAliceCapability.TStartRecognizeDirective start_recognize_directive = 23;` (добавлено в `0.368+` / `0.369+`)

Значения `enum AliceCapabilityMethod.Method`:
- `0..21` (все версии): `START_REQUEST = 0`, `CANCEL_DIALOG = 1`, `START_CONVERSATION = 2`, `STOP_CONVERSATION = 3`, `TOGGLE_CONVERSATION = 4`, `FINISH_CONVERSATION_VOICE_INPUT = 5`, `CANCEL_DIALOG_AND_CLEAR_QUEUE = 6`, `BLOCK = 7`, `UNBLOCK = 8`, `SET_UUID = 9`, `CONNECT = 10`, `DISCONNECT = 11`, `SET_MEGAMIND_COOKIES = 12`, `CLEAR_REQUEST_SEQUENCE = 13`, `STOP_TTS = 14`, `RESET_PREV_REQ_ID = 15`, `SET_UNI_PROXY_URL = 16`, `SET_ICOOKIE = 17`, `START_VOCALIZE = 18`, `START_RECOGNIZE = 19`, `SET_DIALOG_ID = 20`, `SET_DIALOG_TYPE = 21`.
- `22..27` (добавлены в `0.13.1.36`): `SET_ALICE_2_SETTINGS = 22`, `START_DATA_INPUT = 23`, `SET_VINS_URL = 24`, `SET_FOLDER_ID = 25`, `START_SPEECH_INPUT = 26`, `START_RECORD_INPUT = 27`.
- `28..29` (добавлены в `0.368+` / `0.369+`): `START_FILE_UPLOAD = 28`, `START_TRANSLATE_INPUT = 29`.

Поля `Remoting.AliceCapabilityListenerMethod` (поле `18`):
- `1`: `optional Method method = 1;` (`STATE_CHANGED = 0`, `TTS_COMPLETED = 1`, `ON_ALICE_REQUEST = 2`, `ON_ALICE_RESPONSE = 3`, `ON_ALICE_ERROR = 4`, `ALL_STARTUP_SETTINGS_CHANGED = 5`, `TTS_STARTED = 6`, `RECOGNITION_COMPLETED = 7`, с `0.13+` `ON_ALICE_EVENT = 8`, с `0.368+` `ON_FILE_UPLOAD_EVENT = 9`)
- `2`: `optional .quasar.proto.AliceState state = 2;`
- `3`: `optional .quasar.proto.VinsRequest alice_request = 3;`
- `4`: `optional .quasar.proto.Directive alice_response = 4;`
- `5`: `optional .quasar.proto.AliceState.Error error_code = 5;`
- `6`: `optional string error_text = 6;`
- `8`: `optional .quasar.proto.AllStartupSettings all_startup_settings = 8;`
- `9`: `repeated string blocker_ids = 9;` (в `1.266` и `0.13`; зарезервировано в `0.368+`)
- `10`: `optional int64 speech_duration_ms = 10;`
- `11`: `optional string request_id = 11;` (с `0.13+`)
- `12`: `optional .quasar.proto.TAliceEvent alice_event = 12;` (с `0.13+`)
- `13`: `optional TFileUploadEvent file_upload_event = 13;` (с `0.368+`)

### 4.3. `Remoting.DeviceStateCapabilityMethod` (поле `16`)

Целевой объект `remote_object_id = "DeviceStateCapability"`. Содержит `oneof value`:
- `1`: `bluetooth_state` (`TDeviceState.TBluetooth`)
- `3`: `music_state` (`TDeviceState.TMusic`)
- `4`: `radio_state` (`google.protobuf.Struct`)
- `5`: `last_watched` (`TDeviceState.TLastWatched`)
- `6`: `add_supported_feature` (`string`)
- `7`: `supported_features` (`SupportedFeatures { repeated string values = 1; }`)
- `8`: `unsupported_features` (`UnsupportedFeatures { repeated string values = 1; }`)
- `9`: `video_state` (`TDeviceState.TVideo`)
- `10`: `rcu_state` (`TDeviceState.TRcuState`)
- `11`: `screen_state` (`TDeviceState.TScreen` — в `1.266` и `0.13`, зарезервировано в `0.368+`)
- `12`: `multiroom_state` (`TDeviceState.TMultiroom`)
- `13`: `packages_state` (`TDeviceState.TPackagesState`)
- `14`: `experiments` (`TExperimentsProto`)
- `15`: `enrollment_headers` (`TEnrollmentHeaders`)
- `16`: `environment_device_info` (`TEnvironmentDeviceInfo`)
- `17`: `is_tv_plugged_in` (`bool`)
- `19`: `alarm_state` / `20`: `timers_state` / `25`: `messenger_call` / `30`: `set_alarms` — только в `1.266.4.40` (зарезервированы в `0.13+`)
- **`21`**: **`bool mics_muted = 21;`** (wire tag `0xa8 0x01`)
- **`22`**: **`VolumeState volume_state = 22;`** (`sound_muted = 1`, `sound_level = 2`, `sound_max_level = 3`)
- `23`: `battery` (`TDeviceState.TBattery`)
- `24`: `clock_display_state` (`TDeviceState.TClockDisplayState`)
- `26`: `device_config` (`TDeviceState.TDeviceConfig`)
- `27`: `device_config_content_settings` (`DeviceConfigContentSettings`)
- `28`: `device_config_spotter` (`string`)
- `31`: `app_screen_info` (`TDeviceState.TAppScreenInfo`)
- `32`: `reminders_state` (`TDeviceRemindersState`)
- `33`: `is_default_assistant` (`bool`)
- `34`: `bass_options` (`TBassOptions`)
- `35`: `permissions` (`Permissions`)
- **`36`**: **`.NAlice.NAliceApi.TDeviceState device_state = 36;`** (wire tag `0xa2 0x02`; внутри `TDeviceState` поле `MicsMuted = 26`, `SoundLevel = 2`, `SoundMuted = 3` — используется `q cli` по клавише `'m'`)
- `37`: `device_id` (`string`)
- `38`: `internet_connection_state` (`InternetConnectionState`)
- `39`: `external_entity_descriptions` (`ExternalEntityDescriptions`)
- `40`: `active_actions` (`TDeviceState.TActiveActions`)
- `41`: `biometry_options` (`TOndeviceBiometryOptions`)
- `42`: `configuration_status` (`TConfigurationStatus`)
- `43`: `update_status` (`TUpdateStatus`)
- `44`: `screensaver_state` (`TScreensaverState`)
- `45`: `active_actions_with_entities` (`ActiveActionsWithEntities`)
- `46`: `remove_supported_feature` (`string` — добавлено в `0.13.1.36`)

### 4.4. `Remoting.PlaybackControlCapabilityMethod` (поле `13`)

Целевой объект `remote_object_id = "PlaybackControlCapability"`. Схема **не менялась** во всех четырёх поколениях (`1.266.4.40`, `0.13.1.36`, `0.369.1.*`, `0.373.1.13`):
- Поля сообщения:
  - `1`: `optional Method method = 1;`
  - `2`: `optional bool can_request_music = 2;`
  - `3`: `optional uint32 rewind_amount_ms = 3;`
  - `4`: `optional .quasar.proto.MediaRequest.RewindType rewind_type = 4;` (`FORWARD = 0`, `BACKWARD = 1`, `ABSOLUTE = 2`)
  - `5`: `optional .NAlice.TEventSource event_source = 5;`
  - `6`: `optional string track_id = 6;`
  - `7`: `optional bool disable_animation = 7;`
- Значения `enum PlaybackControlCapabilityMethod.Method`:
  - `PLAY = 0`, `PAUSE = 1`, `TOGGLE_PLAY_PAUSE = 2`, `REWIND = 3`, `LIKE = 4`, `DISLIKE = 5`, `NEXT = 6`, `PREV = 7`, `UNLIKE = 8`, `UNDISLIKE = 9`, `REPEAT_ONE = 10`, `REPEAT_ALL = 11`, `UNREPEAT = 12`, `SHUFFLE = 13`, `UNSHUFFLE = 14`, `ENABLE_KARAOKE_MODE = 15`, `DISABLE_KARAOKE_MODE = 16`.

---

## 5. Протоколы и поля локальных IPC-сервисов (`glagold`, `aliced`, `mediad`, `iohub_services`, `yio_audio`)

Все локальные TCP-сервисы Quasar используют единый фрейминг поверх `127.0.0.1:<port>`:
`[u32_le payload_len][QuasarMessage protobuf bytes]`.

### 5.1. `aliced` (`127.0.0.1:9875`)

Сервис голосового ассистента и центрального `EndpointStorage` / `Capability`-хаба внутри `maind --platformd`:

1. **Входящие пакеты (от клиентов `mediad`, `calld`, `q cli`, сторонних сервисов `luna` / `alinad` к `:9875`)**:
   - В `1.266` / `0.13`: `QuasarMessage { remoting = 210 }` (тег `92 0d`).
   - В `0.369` / `0.373`: `QuasarMessage { payload = 252 (<Remoting bytes>), type = 253 (210 = TYPE_REMOTING) }` (теги `e2 0f ... e8 0f d2 01`).
   - Основные вызовы внутри `Remoting`:
     - `remote_object_id = "AliceCapability"` (`field 7`) + `alice_capability_method` (`field 8`):
       - Перехват / отмена диалога: `method = 1` (`CANCEL_DIALOG`), `method = 3` (`STOP_CONVERSATION`), `method = 6` (`CANCEL_DIALOG_AND_CLEAR_QUEUE`), `method = 14` (`STOP_TTS`).
       - Синтез речи (TTS): `method = 18` (`START_VOCALIZE`) + `vocalize_directive` (`field 13`) -> `Text` (`field 1`).
       - Текстовый запрос VINS (`q cli` `'q'`): `method = 0` (`START_REQUEST`) + `vins_request` (`field 2`) -> `event_json` (`field 2`).
       - Управление слушанием: `method = 2` (`START_CONVERSATION`), `method = 4` (`TOGGLE_CONVERSATION`), `method = 5` (`FINISH_CONVERSATION_VOICE_INPUT`).
     - `remote_object_id = "PlaybackControlCapability"` (`field 7`) + `playback_control_capability_method` (`field 13`): транспортные команды плеера (`PLAY = 0`, `PAUSE = 1`, `TOGGLE_PLAY_PAUSE = 2`, `NEXT = 6`, `PREV = 7`).
     - `remote_object_id = "DeviceStateCapability"` (`field 7`) + `device_state_capability_method` (`field 16`): `mics_muted` (`field 21`), `volume_state` (`field 22`), `device_state` (`field 36`).
     - `remote_object_id = "EndpointStorage_"` (`field 7`) + `endpoint_storage_method` (`field 1`) / `endpoint_method` (`field 6`): регистрация и синхронизация локальных `Endpoint` и `Capability`.
2. **Исходящие пакеты (от `:9875` к подключившимся подписчикам)**:
   - Трансляция состояния ассистента `AliceState` (`yandex_io/protos/alice_state.proto`):
     - В `1.266` / `0.13`: `QuasarMessage { alice_state = 3 }` (тег `1a`).
     - В `0.369` / `0.373`: `QuasarMessage { payload = 252 (<AliceState bytes>), type = 253 (3 = TYPE_ALICE_STATE) }`, а для `Remoting`-слушателей — `Remoting { remote_object_id = "AliceCapability", alice_capability_listener_method = 18 { method = 0 (STATE_CHANGED), state = 2 (AliceState) } }`.
   - Поля `AliceState`:
     - `1`: `State state` (`IDLE = 0`, `LISTENING = 1`, `SPEAKING = 2`, `BUSY = 3`, `SHAZAM = 5`, `NONE = 6`)
     - `2`: `string recognized_phrase` (распознанный текст фразы; в `0.368+` помечено `[deprecated = true]`, дублируется в `request_state.request_text`)
     - `3`: `VinsResponse vins_response` (`output_speech = 1`, `suggests = 2`, `text_only = 3`, `div_card = 4`, `payload = 5`, `dialog_id = 8`, `is_streaming = 9`, `has_voice_response = 10`, `dialog_type = 11`)
     - `4`: `double doa_angle` (в `1.266` и `0.13`; в `0.368+` зарезервировано `reserved 4; reserved "doa_angle";`)
     - `5`: `bool has_startup_requirements`
     - `6`: `string activation_spotter_model`
     - `7`: `string request_id`
     - `8`: `double voice_power`
     - `9`: `bool has_speech`
     - `10`: `NAlice.TEventSource event_source`
     - `11`: `bool is_connected`
     - `12`: `bool is_reminder_active`
     - `13`: `TextState text_state` (`text = 1`, `message_view_id = 2`, `symbols_per_second = 3`, `is_end = 4`, `last_partial = 5`)
     - `14`: `ResponseState response_state` (`text = 1`, `cards = 2`, `is_final = 3`, `request_id = 4`, `suggests = 5`, `is_cancelled = 6`, `alice_2_mode_info = 7`, `alice_2_trial_state = 8`, `dialog_id = 9`, `request_annotation = 10`)
     - `15`: `RequestState request_state` (`request_text = 1`, `is_final = 2`, `dialog_id = 3` [с `0.13+`], `is_recognition_started = 4` [с `0.13+`], `files = 5` [с `0.368+`])
     - `16`: `bool spotter_configuring` (добавлено в `0.368+`).

### 5.2. `glagold` (`127.0.0.1:9899`)

Внутренний IPC-сервис локального управления протоколом Glagol (внешний TLS WebSocket-сервер Glagol слушает порт `:1961`), обнаружением устройств mDNS/Avahi и межколоночным кластером:

| Сообщение `quasar.proto` | Старое поле `QuasarMessage` (`1.266` / `0.13`) | Новый `QuasarMessage.Type` (`253` в `0.369` / `0.373`) | Тип в `payload` (`252`) | Направление и назначение на `:9899` |
| :--- | :--- | :--- | :--- | :--- |
| `GlagoldState` | `82` (`glagold_state`) | `TYPE_GLAGOLD_STATE = 82` | `quasar.proto.GlagoldState` | От `glagold` к подписчикам: активные подключения (`connections = 1`: `YANDEXIO_DEVICE = 0`, `GLAGOL_APP = 1`, `OTHER = 2`, `CLUSTER = 3`), `microphone_group_state = 6`, `cluster_device_list = 7`, `public_ip = 8`. |
| `GlagolClusterMessage` | `181` (`glagol_cluster_message`) | `TYPE_GLAGOL_CLUSTER_MESSAGE = 181` | `quasar.proto.GlagolClusterMessage` | Пересылка `QuasarMessage` (`quasar_message_base64 = 4`, `service_name = 3`) на другие колонки кластера (`target_device_all = 1`, `target_device_ids = 2`, `target_device_remote = 5`). |
| `GlagolClusterSign` | `182` (`glagol_cluster_sign`) | Прямое поле `182` в `QuasarMessage` (`reserved 182` в `Type`) | Прямое поле `182` | Метаданные отправителя `from_device_id = 1`, оставленные на верхнем уровне `QuasarMessage` даже в `0.369`/`0.373`. |
| `GlagolDiscoveryRequest` | `185` (`glagol_discovery_request`) | `TYPE_GLAGOL_DISCOVERY_REQUEST = 185` | `quasar.proto.GlagolDiscoveryRequest` | Запрос к `glagold`: `non_yio_devices = 1`, `subscribe_services = 2`, `unsubscribe_services = 3`, `add_announce = 4`, `remove_announce = 5`, `remove_announces_by_host = 7`, `debug_connections = 8`. |
| `EmptyMessage` / `void` | `186` (`glagol_discovery_updated`) | `TYPE_GLAGOL_DISCOVERY_UPDATED = 186` | `void` | Уведомление об обновлении таблицы обнаруженных устройств Glagol. |
| `GlagolDiscoveryResult` | `187` (`glagol_discovery_result`) | `TYPE_GLAGOL_DISCOVERY_RESULT = 187` | `quasar.proto.GlagolDiscoveryResult` | Ответ `glagold`: `repeated GlagolDiscoveryItem items = 1`, `repeated NsdInfo other_items = 2`, `EmptyMessage is_full_result = 3`. |
| `NsdInfo` / `NsdInfoList` / `NsdEnable` | `190..196` (`nsd_*`) | `TYPE_NSD_* = 190..196` | `NsdInfo`, `NsdInfoList`, `NsdEnable`, `void` | Управление mDNS/NSD-анонсами (`server_port = 1`, `stereopair = 2`, `tandem = 3`, `guest_mode = 4`, `discovered_request = 5`). |

- **Пример реального пакета `q glagol-connections` (`maind --glagol-connections`) на `:9899` в `0.369`/`0.373`**:
  Утилита отправляет `9 байт` (`09 00 00 00` + `e2 0f 02 42 00 e8 0f b9 01`):
  - `e2 0f 02 42 00` -> `payload = 252` длиной `2 байта`: `42 00` = поле `8` (`debug_connections`, wire tag `(8 << 3) | 2 = 0x42`) с пустым `EmptyMessage` (`0x00`);
  - `e8 0f b9 01` -> `type = 253` со значением `0xb9 0x01 = 185` (`TYPE_GLAGOL_DISCOVERY_REQUEST`).
- Кроме того, для межколоночного взаимодействия по WebSocket Glagol используется схема `quasar.proto.GlagolEndpointProtocol` (`yandex_io/services/glagold/endpoints/protos/protocol.proto`, `oneof message`: `endpoint_sync = 1`, `handle_directive = 2`, `add_capability_listener = 3`, `capability_state_changed = 4`, `protocol_settings_changed = 5`, `endpoint_sync_request = 6`, `add_capability = 7`, `remove_capability = 8`, `add_endpoint = 9`, `remove_endpoint = 10`, с `0.13+` `capability_events = 11`).

### 5.3. `mediad` (`127.0.0.1:9900`, а также `audioclient` `:9906`)

Отдельный процесс `maind --mediad` отвечает за GStreamer-плееры аудио/радио и синхронизацию состояния воспроизведения с `platformd`:

| Сообщение `quasar.proto` | Старое поле `QuasarMessage` (`1.266` / `0.13`) | Новый `QuasarMessage.Type` (`253` в `0.369` / `0.373`) | Направление и назначение на `:9900` / `:9906` |
| :--- | :--- | :--- | :--- |
| `MediaRequest` | `8` (`media_request`) | `TYPE_MEDIA_REQUEST = 8` | От `platformd` к `mediad` (`:9900` / `:9906`): запуск и управление потоком (`play_audio = 1`, `play_video = 2`, `pause = 4`, `resume = 5`, `next = 6`, `free_audio_focus = 7`, `take_audio_focus = 8`, `previous = 9`, `replay = 10`, `like = 11`, `dislike = 12`, `rewind = 13`, `uid = 14`, `play_radio = 16`, `vins_init_request_id = 19`, `player_descriptor = 22`, `is_prefetch = 27`, `is_download = 28`, `stream_data = 29`, `stream_data_end = 30`, `clean_players = 31`, `vins_request_id = 34`, `equalizer_config = 35`, `metadata = 37`, `current_focused_channel = 39`, `playback_starter_user_id = 40`, `account_device_count = 42`, `playback_speed = 44`, `recreate_players = 45`, `playback_options = 47`, `start_prefetch = 48`, `stop_prefetch = 49`, `update_fade_out = 50`, `download_time_out_ms = 51`, `reason_message = 52`, `jog_wheel = 53`, `resume_analytics_context = 54`). |
| `MediaMessage` | `27` (`media_message`) | `TYPE_MEDIA_MESSAGE = 27` | Экранные/медиа-события (`recommendations = 1`, `featured_new = 2`, `trending = 3`, `last_watched = 4`, `next_screen = 5`, `previous_screen = 6`, `to_start = 7`, `to_end = 8`, `to_home_screen = 9`, `show_video_list = 10`, `show_description = 11`, `show_bottom_sheet = 12`, `hide_bottom_sheet = 13`, `home_screen_suggests = 14`, `message_id = 15`, `show_payment = 16`, `music = 17`, `show_tv_announce = 18`, `leader_lost = 19`, `leader_connected = 20`, `hdmi_on = 21`, `hdmi_off = 22`, `go_up = 23`, `go_down = 24`). |
| `AppState` | `33` (`app_state`) | `TYPE_APP_STATE = 33` | От `mediad` к `platformd`: полное состояние плеера (`screen_state = 1`, `music_state = 2`, `video_state = 3`, `radio_state = 4`, `bluetooth_player_state = 5`, `audio_player_state = 6`, `media_info = 7`, `rcu_state = 9`, `packages_state = 13`, в `0.369` `alice_video_state = 14`). |
| `VolumeState` | `42` (`volumeState`) | `TYPE_VOLUME_STATE = 42` | Текущая громкость (`current_volume = 7`, `is_muted = 8`, `current_volume_double = 9`). |
| `LeaderOverride` | `77` (`leader_override`) | `TYPE_LEADER_OVERRIDE = 77` | Переопределение метаданных трека/радио от ведущей колонки мультирума (`music_state_override = 4`, `radio_state_override = 5`). |
| `VolumeSetMessage` | `78` (`volume_set_message`) | `TYPE_VOLUME_SET_MESSAGE = 78` | Установка громкости (`volume_value = 1`, `max_volume = 2`, `alice_volume_value = 3`, `request_id = 4`). |
| `SimplePlayerState` | `84` (`simple_player_state`) | `TYPE_SIMPLE_PLAYER_STATE = 84` | Сокращённое состояние плеера (`has_pause = 1`, `has_play = 2`, `has_next = 3`, `has_prev = 4`, `position = 5`, `duration = 6`, `title = 7`, `subtitle = 8`, `has_progress_bar = 9`, `has_like = 11`, `has_dislike = 12`, `show_player = 13`, `live_stream_text = 14`, `id = 15`, `type = 16`, `player_name = 17`, `entity_info = 18`, `provider_name = 19`, `hide_progress_bar = 20`, `player_type = 21`, `position_time_point = 22`, `playback_starter_user_id = 23`, `current_screen = 24`, `extra = 100`). |
| `OnMediaErrorHolder` | `124` (`on_media_error`) | `TYPE_ON_MEDIA_ERROR = 124` | Уведомление об ошибке воспроизведения (`media_content_type = 1`). |
| `AudioClientEvent` | `127` (`audio_client_event`) | `TYPE_AUDIO_CLIENT_EVENT = 127` | Событие жизненного цикла GStreamer-аудиоклиента (`event = 1`: `STATE_CHANGED`, ` TIMINGS`, `ERROR`, `UNDERFLOW`, `FIRST_CHUNK_DECODED` и др., `state = 2`, `player_descriptor = 4`, `audio = 5`). |
| `LegacyPlayerStateChanged` | `173` (`legacy_player_state_changed`) | `TYPE_LEGACY_PLAYER_STATE_CHANGED = 173` | Старое событие смены состояния плеера. |
| `EqualizerParameters` | `251` (`equalizer_params`) | `TYPE_EQUALIZER_PARAMS = 251` | Параметры DSP-эквалайзера (`json_data = 1`, `config_hash = 2` — добавлено в `0.368+` / `0.369+`). |

### 5.4. `iohub_services` (`127.0.0.1:9903`, а также `iohub_clients` `:9910`)

Центральная шина событий SDK и внешних директив (`yandex_io/protos/yandex_io.proto`):

| Сообщение `quasar.proto` | Старое поле `QuasarMessage` (`1.266` / `0.13`) | Новый `QuasarMessage.Type` (`253` в `0.369` / `0.373`) | Поля вложенного сообщения (`yandex_io.proto`) |
| :--- | :--- | :--- | :--- |
| `IOEvent` | `85` (`io_event`) | `TYPE_IO_EVENT = 85` | События от `iohub_services` к клиентам: `on_start_setup = 1`, `on_sound_data_transfer_start = 2`, `on_connecting_to_network = 3`, `on_setup_error = 4`, `on_finish_setup = 5`, `last_update_info = 31`, **`sdk_state = 34`** (`SDKState`: `alice_state = 2`, `bluetooth_state = 3`, `alarms_state = 5`, `configuration_state = 6`, `update_state = 7`, `app_state = 8`, `reminder_state = 10`, `notification_state = 12`, `calld_state = 13`, `ntpd_state = 14`, `all_startup_settings = 19`, `timers_timings = 22`, `do_not_disturb_state = 23`), `authentication_status = 35`, `on_conversation_error = 36`, `on_media_error = 42`, `spectrum = 46`, `sound_command = 51`, `starting_configure_success = 52`, `on_music_state_changed = 54`, `on_invalid_oauth_token = 55`, `on_audio_client_event = 56`, `brick_status = 58`, `registration_result = 60`, `device_group_state = 63`, `glagol_discovery_result = 64`, `on_invalid_authentication = 65`, `brick_info = 66`, `directives_preprocess_request = 77`, `issue_auth_code_request = 78`. |
| `IOControlCommand` | `88` (`io_control`) | `TYPE_IO_CONTROL = 88` | Команды клиентов в `iohub_services`: `set_setup_mode = 1`, `set_host_application_message = 7`, `stop_setup_mode = 11`, `toggle_setup_mode = 12`, `allow_init_configuration_state = 17`, `authenticate = 22`, `change_account = 23`, `allow_update = 25`, `provide_user_account_info = 44`, `revoke_user_account_info = 52`, `register_device_in_backend = 54`, `send_app_state = 55`, `call_message = 58`, `is_screen_active = 59`, `provide_device_state = 60`, `timezone = 61`, `location = 62`, `wifi_list = 63`, `vins_response = 68`, `directive_entity_list = 70`, `equalizer_config = 73`, `media_device_identifier = 75`, `setup_credentials = 80`, `provide_active_user_account_info = 82`, `directives_preprocess_response = 93`, `user_account_list = 94`, **`external_directives = 95`** (`ExternalDirectives { DirectiveList directive_list = 1; TEventSource event_source = 2; }`), `issue_auth_code_response = 96`. |
| `std::string` (`service_name_ready`) | `157` (`service_name_ready`) | `TYPE_SERVICE_NAME_READY = 157` | Строка с именем подключившегося сервиса при рукопожатии (`"mediad"`, `"calld"`, `"updatesd"` и др.). |
| `IOHubMessage` | `183` (`io_hub_message`) | `TYPE_IO_HUB_MESSAGE = 183` | Запрос начального снимка состояния при подписке: `optional EmptyMessage retrieve_state = 1;`. |

### 5.5. `yio_audio` (`127.0.0.1:9850`)

Сервис потоковой раздачи микрофонных и VQE-каналов (`yandex_io/protos/yandex_io.proto`):
- **Запрос подписки (от клиента к `:9850`)**:
  - В `1.266` / `0.13`: `QuasarMessage { io_channel_request = 159 }`.
  - В `0.369` / `0.373`: `QuasarMessage { payload = 252 (<IOChannelRequest bytes>), type = 253 (159 = TYPE_IO_CHANNEL_REQUEST) }`.
  - Поля `IOChannelRequest`: `request_main = 1`, `request_all = 2`, `request_vqe = 3`, `request_raw = 4`, `request_com = 5`, `unsubscribe = 6`.
- **Поток аудиофреймов (от `:9850` к подписчику каждые `40 мс`)**:
  - В `1.266` / `0.13`: `QuasarMessage { io_audio_channels_data = 158 }` (тег `f2 09`).
  - В `0.369` / `0.373`: `QuasarMessage { payload = 252 (<IOAudioChannelsData bytes>), type = 253 (158 = TYPE_IO_AUDIO_CHANNELS_DATA) }`.
  - Поля `IOAudioChannelsData`: `repeated IOAudioChannelData channels = 1;`.
  - Поля `IOAudioChannelData`:
    - `1`: `optional .voicetech.AudioInputChannelType type = 1;` (`RAW_MIC = 1`, `VQE_OMNI = 6` и др.)
    - `2`: `optional string name = 2;` (`"raw_mic_0"`, `"raw_mic_1"`, `"raw_mic_2"`, `"raw_mic_3"`, `"vqe_0"`)
    - `3`: `optional bytes data = 3;` (16-бит LE PCM)
    - `4`: `optional bool is_for_recognition = 4;`
    - `5`: `reserved 5;` (устаревшее `float doa_angle`)
    - `6`: `optional double doa_angle = 6;` (угол DOA в градусах, `wire type 1` `fixed64`)
    - `7`: `optional int32 sample_rate = 7;` (`16000`)
    - `8`: `optional .quasar.proto.SoundFormat.Type format = 8;` (`PCM = 1`, `OPUS = 2`, `RAW = 3`, `HEADERLESS_OPUS_STREAM = 4`)
    - `9`: `optional int64 record_start_timepoint_ms = 9;`
    - `10`: `optional uint32 duration_ms = 10;` (добавлено в `0.13.1.36`)
    - `11`: `repeated .quasar.proto.SoundBufferTag tags = 11;` (добавлено в `0.368+` / `0.369+`).

---

## 6. Выводы для реализации IPC-моков в `hw-bridge` (`kuyase`)

>[!tip]
>Для прозрачной поддержки как старых прошивок (`1.266.4.40` на `yandexmini`, `0.13.1.36` на `orion`), так и новых прошивок (`0.369.1.*` и `0.373.1.13` на `yandexmini2*`, `yandexmicro`, `plum*`, `mango`, `station2`, `cucumber`, `bergamot`) декодеры и генераторы `QuasarMessage` в `hw-bridge/src/proto.rs` могут опираться на инвариант эквивалентности номеров полей:
>1. **При приёме любого пакета `QuasarMessage` (`:9875`, `:9899`, `:9900`, `:9903`, `:9850`)**: если в сообщении присутствуют поля `252` (`PbField::Bytes(payload)`) и `253` (`PbField::Varint(type_id)`), его следует распаковывать точно так же, как старое сообщение `QuasarMessage` с полем номер `type_id as u32` и содержимым `payload`.
>2. **В `DeviceStateCapabilityMethod` (`field 16` внутри `Remoting`)**: помимо поля `21` (`bool mics_muted = 21`), необходимо учитывать поле `36` (`TDeviceState device_state = 36`, внутри которого `MicsMuted = 26`), отправляемое апплетом `q cli` (`maind --cli`).
