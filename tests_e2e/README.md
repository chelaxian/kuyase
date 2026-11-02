# E2E-тесты (`tests_e2e/`)

Контур сквозных (E2E) смоук-тестов поверх headless-интерфейса эмулятора `kuyase` (`:18091` `qemu-hw-bridge` + `:15555` `adbd`) для проверки актуальной экосистемы пакетов:

1. **`test_01_yam.py`** — установка пакетного менеджера [`nyakokitsu/yam`](https://github.com/nyakokitsu/yam) (v0.2+), сверка автоопределения платформы (`detect_platform`), подключение реестра `https://repo.yse.wtf/v2` и проверка `yam doctor`.
2. **`test_02_luna.py`** — установка и запуск рантайма [`nyakokitsu/luna`](https://github.com/nyakokitsu/luna) (`:8099`), проверка `/api/device`, управления световым кольцом (`/api/led/ring`), сегментными часами (`/api/clock`), громкостью (`/api/volume/set`) и перехвата распознанной речи через `:9875` (`/api/alice/speech?cancel=true`).
3. **`test_03_airplay.py`** — смоук-тест пилотного плагина [`Station-airplay`](https://github.com/Yandex-Station-Universal-Repo/Station-airplay) поверх `luna` (старт супервизора, ожидание `/run/station-airplay/private-bus` и RTSP `:5000`/`:7000`, проверка `org.gnome.ShairportSync.GetControlStatus` и тихих голосовых команд транспорта `"пауза"` / `"дальше"`).

>[!note]
>Скачанные релизы кешируются на хосте в `work/e2e-cache/`. Сами ВМ запускаются в режиме `--snapshot` (раздел `/data` монтируется в `tmpfs` в ОЗУ и очищается при завершении теста).

## Запуск

```bash
# Запустить все E2E-тесты на одной плате (например, station2):
KUYASE_E2E_BOARD=station2 python -m unittest discover -s tests_e2e -v

# Запустить только смоук-тест YAM v0.2+ по всем образам в work/:
python -m unittest tests_e2e/test_01_yam.py -v

# Запустить смоук-тест Station-airplay с локальным пакетом .yampkg:
KUYASE_E2E_BOARD=station2 KUYASE_AIRPLAY_PKG=/path/to/station-airplay.yampkg python -m unittest tests_e2e/test_03_airplay.py -v
```

## Переменные окружения

| Переменная | Назначение |
| :--- | :--- |
| `KUYASE_E2E_BOARD` | Профиль платы (например, `station2`, `cucumber`, `orion`) или список через запятую. По умолчанию обходит все `work/*-system.ext4`. |
| `KUYASE_YAM_BIN` | Путь к локальному бинарнику `yam-aarch64-unknown-linux-musl` (опционально). |
| `KUYASE_LUNA_PKG` | Путь к локальному пакету `luna.yampkg` (опционально; по умолчанию ставится `yam/luna` из реестра). |
| `KUYASE_AIRPLAY_PKG` | Путь к локальному пакету `station-airplay.yampkg` для `test_03_airplay.py`. |
