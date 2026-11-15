# Код-ревью ECO AIOps Platform

- **Дата:** 2026-11-15
- **Ревизия:** `HEAD cb32dcb` — "Update README for install-time modes and agent repacker"
- **Область:** весь код на HEAD (незакоммиченных изменений нет): Rust-бэкенд `src-tauri/src`, UI `src/app.js` + `src/index.html`, установщик `src-tauri/windows/hooks.nsh`, утилиты `tools/pack`, `tools/signer`, `dev-apps/*`.
- **Метод:** статический анализ, чтение кода и проверка цепочек вызовов. Каждое утверждение сверено с текущими файлами; номера строк указаны по HEAD.
- **Статус:** ревью, файлы не изменялись.

## Summary

Архитектура в целом аккуратная: Ed25519 `verify_strict`, fail-closed проверки, защита от zip-slip, `env_clear` для пакетных процессов, атомарное хранилище токенов. Основные риски сосредоточены в трёх местах: (1) обновление/откат пакета уничтожает историю чатов и артефакты; (2) контур доверия (install-mode, dev-anchor, секреты, `external_writes`) можно обойти или расширить; (3) блокирующие Tauri-команды и утечки потоков/сокетов в движке дают зависания UI и рост ресурсов. Отдельно есть расхождения между платформенными верификаторами и утилитами `eco-pack`/`signer`.

## Issues Found

| Severity | File:Line | Issue |
|---|---|---|
| CRITICAL | src-tauri/src/registry.rs:899 | Обновление/откат удаляет `engine/` и `sessions/` вместе с историей чатов и артефактами |
| CRITICAL | src-tauri/windows/hooks.nsh:27 | Апгрейд установщика безусловно перезаписывает `install-mode.json`, по умолчанию в `local` |
| CRITICAL | src-tauri/src/proxy.rs:523 | Пакетный инструмент может запросить любой DPAPI-секрет (`llm_api_key`, `mcp.<id>` с токеном) |
| WARNING | src-tauri/src/manifest.rs:285 | Верификатор не отклоняет файлы вне `hashes`; в corporate-режиме добавленный skill загружается движком |
| WARNING | src-tauri/src/registry.rs:424 | При несовпадении seal пакет принимается по подписи и тут же пере-запечатывается |
| WARNING | src-tauri/src/config.rs:93 | Путь install-mode берётся из env `ProgramData`; ошибка чтения файла снимает блокировку режима |
| WARNING | src-tauri/src/registry.rs:74 | `ECO_PLATFORM_DEV=1` в release включает пользовательский anchor без прав администратора |
| WARNING | src-tauri/src/proxy.rs:604 | Отзыв allowlist/CRL не проверяется для уже запущенного движка |
| WARNING | src-tauri/src/manifest.rs:312 | `external_writes` — глобальный флаг: одно подключение открывает запись для всех `http-mcp` |
| WARNING | src-tauri/src/registry.rs:873 | Нечитаемый установленный манифест обходит проверку downgrade и архив, старая версия удаляется |
| WARNING | src-tauri/src/commands.rs:148 | `install_app` не останавливает движок приложения и выполняется на UI-потоке |
| WARNING | src-tauri/src/config.rs:296 | `config.json` пишется без temp+rename и без блокировки; ошибка парсинга молча заменяется дефолтами |
| WARNING | src-tauri/src/artifacts.rs:318 | Сессии артефактов удаляются через 24 ч (`orphan_hours`), хотя TTL по умолчанию 168 ч |
| WARNING | src-tauri/src/engine.rs:1093 | Сбой после `spawn` оставляет осиротевший `opencode.exe` без pid-файла |
| WARNING | src-tauri/src/commands.rs:628 | Два параллельных `chat_open` запускают два движка и убивают активный |
| WARNING | src-tauri/src/commands.rs:834 | Синхронные chat-команды блокируют UI-поток и держат мьютекс на HTTP с таймаутом 300 с |
| WARNING | src-tauri/src/engine.rs:777 | Бюджет готовности 25 с обходится 300-секундным таймаутом проб `GET /config` |
| WARNING | src-tauri/src/engine.rs:1424 | `consume_events` блокируется в `read_line` без таймаута; поток+сокет утекает на каждый ход |
| WARNING | src-tauri/src/proxy.rs:1236 | `read_file` читает файл целиком в память и только потом обрезает до 64 КБ |
| WARNING | src-tauri/src/lib.rs:22 | `startup_sweep` (полный хэш всех пакетов + `taskkill`) выполняется до создания окна |
| WARNING | src-tauri/src/commands.rs:112 | `list_apps`/`open_app` синхронно пере-хэшируют все файлы пакетов на UI-потоке |
| SUGGESTION | src-tauri/src/lib.rs:236 | CLI `--verify` не проверяет seal, расходясь с `registry::verify_for` |
| SUGGESTION | src-tauri/src/commands.rs:503 | URL MCP-сервера принимает `http://`, Bearer-токен уходит в открытом виде |
| SUGGESTION | src-tauri/src/registry.rs:950 | `extract_zip` не ограничивает число записей и распакованный размер (zip-bomb) |
| SUGGESTION | src-tauri/src/pack.rs:619 | `#[tauri::command]` на приватном `tool_info` не зарегистрирован — мёртвый wrapper |
| SUGGESTION | tools/pack/src/core.rs:66 | Второй валидатор semver расходится с `manifest::parse_version` |
| SUGGESTION | src/app.js:2069 | Переключение вкладки настроек каждый раз делает полный рекурсивный обход артефактов |
| SUGGESTION | src/app.js:788 | `chatItemKey` строит отпечаток по всему тексту истории на каждое SSE-событие |

## Detailed Findings

### 1. [CRITICAL] `src-tauri/src/registry.rs:899`: обновление и откат уничтожают чаты и артефакты

**Confidence:** 97%

**Problem:** Данные движка лежат внутри пакета: `engine_home = home/apps/<app_id>/engine` (`engine.rs:1036`), артефакты — `home/apps/<app_id>/sessions` (`artifacts.rs:76`). `archive_current`/`copy_package_dir` намеренно исключают `engine`, `sessions`, `tmp` (`registry.rs:532`), после чего `install` делает `fs::remove_dir_all(&dest)` (строка 899), а `rollback` — то же на строке 720. Итог: каждая установка новой версии и каждый откат безвозвратно удаляют историю чатов (`opencode.db`) и дампы артефактов; в архиве их нет, восстановить нельзя. Если движок ещё держит БД, `remove_dir_all` падает посередине, оставляя `dest` без манифеста — после этого `rollback` отказывает (`manifest::load(&dest)?`, строка 646).

**Suggestion:** Держать runtime-состояние (`engine/`, `sessions/`) вне `apps/<app_id>` либо переносить его в сторону перед заменой и возвращать после; замену делать атомарно через `rename(dest -> old)`, `rename(staging -> dest)` с удалением `old` только после успеха.

### 2. [CRITICAL] `src-tauri/windows/hooks.nsh:27`: апгрейд сбрасывает режим в local

**Confidence:** 93%

**Problem:** `NSIS_HOOK_PREINSTALL` выполняется при каждой установке, включая обновление, и безусловно открывает `install-mode.json` на запись. Значение по умолчанию — `local` (строка 12), а при интерактивном апгрейде пользователю предлагается «локальный режим» с `IDYES`. Корпоративная машина после тихого апгрейда без `/MODE=corporate` превращается в local: trust anchor и allowlist перестают применяться.

**Suggestion:** Писать `install-mode.json` только если файла ещё нет; при наличии сохранять текущий режим, а для silent-апгрейда без явного `/MODE=` не выбирать `local` по умолчанию (fail-closed в `corporate`).

### 3. [CRITICAL] `src-tauri/src/proxy.rs:523`: глобальные секреты доступны любому пакету

**Confidence:** 95%

**Problem:** `tools[].secrets` — произвольное отображение env → имя секрета, проверяется только по символам (`secret_name_ok`), а хранилище плоское: `home/secrets/*.bin` (`secrets.rs:25`). Пакетный инструмент может объявить `{"TOKEN": "llm_api_key"}` или `{"TOKEN": "mcp.<id>"}`; второй — это весь `ServerDef` JSON с URL и Bearer-токеном другого подключения (`mcp.rs:20`). Это нарушает заявленную изоляцию приложений и позволяет подписанному пакету украсть ключ шлюза и токены MCP.

**Suggestion:** Запретить зарезервированные имена (`llm_api_key`, `mcp.*`) для `tools[].secrets` и/или сделать пространство имён на приложение (`apps/<id>/secrets`), выдавая доступ только к секретам, объявленным и одобренным для этого пакета.

### 4. [WARNING] `src-tauri/src/manifest.rs:285`: лишние файлы вне `hashes` не отклоняются

**Confidence:** 93%

**Problem:** `verify_impl` идёт только по `manifest.hashes` и не проверяет, что в каталоге нет файлов сверх манифеста. `copy_dir` при установке копирует всё дерево, а `engine::skill_texts` (`engine.rs:790`) и `proxy::has_skills` читают все каталоги `skills/*/SKILL.md`, не сверяясь с манифестом. В corporate-режиме seal не пишется (`registry::install` запечатывает только local), поэтому атакующий с доступом к каналу доставки `zip` может добавить `skills/evil/SKILL.md` при неизменных `manifest.json`/`manifest.sig`: подпись и все хэши из манифеста останутся валидными, а лишний skill попадёт в системный контекст модели. Запуск произвольного exe так не получить (манифест подписан, путь `bin` менять нельзя), но внедрение неаутентифицированных инструкций и ресурсов, которые читает пакетный инструмент, возможно.

Примечание: пакеты, собранные `eco-pack build`, покрывают все используемые файлы — `targets_with_skills` (`tools/pack/src/core.rs:156-179`) включает промпт, все skills и все `tools[].bin`. Проблема не в покрытии корректного пакета, а в том, что платформа не отклоняет лишние файлы и не требует полного покрытия на установке.

**Suggestion:** При верификации строить множество файлов пакета и отклонять (или хотя бы помечать) всё, что не входит в `hashes` — тем самым закрыть пакет. Как минимум включить в `hashes` все файлы, а не только `targets_with_skills`, и требовать это на установке, а не только в `eco-pack validate` (сейчас там это лишь warning, `core.rs:625`).

### 5. [WARNING] `src-tauri/src/registry.rs:424`: seal-несоответствие молча «легализуется»

**Confidence:** 93%

**Problem:** В local-режиме при ошибке `seal::verify` код, если есть любой доверенный ключ, откатывается к проверке подписи и при успехе сразу вызывает `seal::write` по текущей (изменённой) папке. Пакет с добавленным `skills/<new>/SKILL.md` ломает только seal, подпись при этом валидна — добавленный skill принимается и запечатывается как легитимный.

**Suggestion:** Использовать fallback только если seal-файл отсутствует; при существующем, но несовпадающем seal — падать fail-closed, не вызывая `seal::write`.

### 6. [WARNING] `src-tauri/src/config.rs:93`: блокировка режима обходится через env и fail-open

**Confidence:** 92%

**Problem:** `install_mode_path()` берёт `ProgramData` из переменных процесса, тогда как allowlist специально читает HKLM, чтобы это исключить. Подмена `ProgramData` на пустую папку даёт `read_install_mode() == None` и `set_mode(Local)` проходит. Кроме того, `read_install_mode` глушит ошибки через `.ok()`: повреждённый или нечитаемый `install-mode.json` молча снимает блокировку.

**Suggestion:** Читать путь из HKLM (как `allowlist::program_data_from_registry`) и различать «файла нет» и «файл есть, но не читается/не парсится» — во втором случае fail-closed.

### 7. [WARNING] `src-tauri/src/registry.rs:74`: dev-anchor включается в release через env

**Confidence:** 90%

**Problem:** `dev_mode()` возвращает true в release при `ECO_PLATFORM_DEV=1`, а `resolve_anchor` тогда читает `<home>/trusted.pub`, и `install_trust_anchor` пишет туда без elevation. На машине без HKLM-anchor (корпоративный контур ещё не раскатан) пользователь может выставить переменную, сам себе подписать пакет и allowlist и обойти политику. В документации это описано как dev-режим, но в release он управляется только переменной окружения.

**Suggestion:** Компилировать dev-anchor только при `debug_assertions` или требовать отдельный маркер, установленный администратором; не позволять одной env-переменной разрешать выдачу доверия.

### 8. [WARNING] `src-tauri/src/proxy.rs:604`: отзыв allowlist не действует на запущенный движок

**Confidence:** 92%

**Problem:** Долгоживущий gateway на каждый `tools/call` проверяет только raw-token (`identity::validate`), а решение allowlist/CRL принимается один раз в `Proxy::new`. Пакет, добавленный в `revoked`, продолжает исполнять инструменты до истечения токена (12 ч, `DEFAULT_TTL_SECS`) или перезапуска движка.

**Suggestion:** Перепроверять `allowlist::guard(...).decision(&app_id, &version)` (минимум — `revoked`) на каждый `tools/call` либо связывать валидацию токена с текущим allowlist.

### 9. [WARNING] `src-tauri/src/manifest.rs:312`: `external_writes` открывает запись всем подключениям

**Confidence:** 94%

**Problem:** `external_writes_allowed` — это просто `!external_writes.is_empty()`, и результат применяется ко всем `http-mcp` подключениям (`proxy.rs:316`, `lib.rs:519`). Документация же определяет поле как список id подключений с правом записи. Достаточно указать одно подключение, чтобы write-инструменты стали доступны и на остальных.

**Suggestion:** Сравнивать с конкретным id: `external_writes.iter().any(|id| id == &connection.id)` в маршрутизации и в `mcp_call_for_app`.

### 10. [WARNING] `src-tauri/src/registry.rs:873`: повреждённый манифест обходит downgrade и архив

**Confidence:** 92%

**Problem:** Проверка понижения версии выполняется только внутри `if let Ok(existing) = manifest::load(&dest)`. Если установленный манифест нечитаем, `archive_current` тоже возвращает `Ok(None)` (строка 606), после чего старый каталог удаляется (строка 899). В результате можно откатиться на более старую версию и потерять точку отката.

**Suggestion:** Считать «каталог есть, манифест не читается» жёсткой ошибкой, требующей восстановления администратором; пропускать проверки только при полном отсутствии установленного каталога.

### 11. [WARNING] `src-tauri/src/commands.rs:148`: установка при работающем движке ломает каталог

**Confidence:** 90%

**Problem:** `install_app` синхронная, на UI-потоке, и не удаляет движок приложения из `state.engines` (в отличие от `rollback_app`, строка 187). Если приложение открыто, `opencode.exe` держит `apps/<id>/engine/data/opencode.db`, `fs::remove_dir_all(&dest)` падает с sharing violation уже после начала удаления — каталог остаётся частично удалённым.

**Suggestion:** Перед install закрывать движок (remove из map + дождаться kill), выполнять установку в `spawn_blocking` и делать замену через staging + атомарный rename.

### 12. [WARNING] `src-tauri/src/config.rs:296`: неатомарная запись конфигурации

**Confidence:** 91%

**Problem:** `save` пишет `config.json` на месте без temp-файла, fsync и блокировки, а `set_workspace`/`save_config`/`set_mode`/`set_role` делают read-modify-write без синхронизации. Падение или два параллельных сохранения могут обнулить или усечь файл; `load` при ошибке парсинга молча подставляет `UserConfig::default()`, теряя endpoint, модель, capabilities, рабочие папки и квоты.

**Suggestion:** Сериализовать доступ и писать через temp + `sync_all` + атомарный rename; ошибку декодирования показывать пользователю, а не заменять файл дефолтами.

### 13. [WARNING] `src-tauri/src/artifacts.rs:318`: фактическое хранение артефактов — 24 часа

**Confidence:** 93%

**Problem:** `Policy::default()` задаёт `ttl_hours = 168`, но `orphan_hours = 24` (строка 65), а `config::policy` переопределяет только TTL. `close_session` снимает `.active`, после чего `sweep` удаляет любую неактивную сессию старше 24 ч, независимо от настроенного TTL. Заявленного 7-дневного хранения нет — дампы и результаты grep исчезают через сутки.

**Suggestion:** Считать срок только по `ttl_hours` (или выставлять `orphan_hours >= ttl_hours`) и оставить orphan-логику для сессий с устаревшим `.active`, а не для всех закрытых.

### 14. [WARNING] `src-tauri/src/engine.rs:1093`: осиротевший движок при сбое старта

**Confidence:** 92%

**Problem:** После `spawn` идут `wait_ready(...)?`, `existing_session/create_session` и только затем запись pid-файла (строка 1100). Любой ранний `?` выходит из функции, `std::process::Child` при drop не убивает процесс, а pid-файла ещё нет — `kill_leftover` его не найдёт. Следующий запуск поднимет второй движок на том же `engine_home`/`opencode.db`, что код явно запрещает (строки 1041-1042).

**Suggestion:** Писать pid/runtime сразу после `spawn` и оборачивать `Child` в guard, который делает `kill()+wait()` на всех error-путях.

### 15. [WARNING] `src-tauri/src/commands.rs:628`: гонка двух `chat_open`

**Confidence:** 90%

**Problem:** `chat_open_blocking` проверяет map, отпускает мьютекс и только потом вызывает `Engine::start`, который внутри делает `kill_leftover` и спавнит новый процесс. Два одновременных вызова (двойной клик, установка → открытие) запускают два движка: второй `kill_leftover` убивает первый, а map сохраняет мёртвый `Engine`; либо оба работают с одним `engine_home`.

**Suggestion:** Резервировать слот под мьютексом (маркер «starting» или per-app guard) до спавна и освобождать его после вставки `Engine`.

### 16. [WARNING] `src-tauri/src/commands.rs:834`: синхронные chat-команды вешают UI

**Confidence:** 91%

**Problem:** `chat_list`, `chat_history`, `chat_new`, `chat_delete`, `chat_reset` не async, поэтому Tauri выполняет их на главном потоке. Они делают HTTP к движку под `engines_lock`, причём агент имеет таймаут 300 с (`engine.rs:530`); зависший или занятый sidecar замораживает окно и блокирует остальные chat-команды.

**Suggestion:** Сделать команды `async` + `spawn_blocking` и копировать `base/auth/session_id` из map до сетевого вызова, не держа мьютекс во время I/O.

### 17. [WARNING] `src-tauri/src/engine.rs:777`: таймаут готовности не работает

**Confidence:** 91%

**Problem:** `wait_ready` проверяет `started.elapsed() > timeout` только после каждой пробы, а проба использует общий `agent()` с `timeout(300s)` (`engine.rs:530`). Один зависший `GET /config` блокирует старт до 5 минут вместо заявленных 25 секунд.

**Suggestion:** Использовать отдельный агент с коротким таймаутом (1-2 с) для readiness-проб и абсолютный дедлайн на весь цикл.

### 18. [WARNING] `src-tauri/src/engine.rs:1424`: утечка потока и сокета на каждый ход

**Confidence:** 92%

**Problem:** `consume_events` проверяет `stop` только в начале цикла, затем блокируется в `reader.read_line`. Ответ создан `stream_agent` без read-таймаута (`engine.rs:539`). После завершения или отмены хода, если в шине больше нет строк, поток навсегда остаётся на сокете — по одному на каждый `chat_send` (`commands.rs:725`).

**Suggestion:** Задать read-таймаут (или закрывать сокет при установке `stop`) и выходить из цикла по таймауту/ошибке; проверять `stop` через `poll`/неблокирующее чтение.

### 19. [WARNING] `src-tauri/src/proxy.rs:1236`: чтение файла целиком до обрезки

**Confidence:** 93%

**Problem:** `read_file` делает `fs::read(&path)` и только потом срезает до `MAX_READ_BYTES` (64 КиБ). Файл в сотни мегабайт в рабочей папке полностью аллоцируется в памяти прокси; то же в `skill_call` (`proxy.rs:1175`).

**Suggestion:** Открывать файл и читать не более `MAX_READ_BYTES + 1` (например, `File::take`), а `metadata().len()` использовать для `bytes`/`truncated`.

### 20. [WARNING] `src-tauri/src/lib.rs:22`: тяжёлый `startup_sweep` до создания окна

**Confidence:** 90%

**Problem:** До `tauri::Builder` вызывается `startup_sweep`: `registry::list` (пере-хэширование всех файлов всех пакетов), `kill_leftover` на каждое приложение (спавн и ожидание `taskkill`) и обход артефактов. Окно не появляется, пока всё это не закончится. Там же `remove_dir_all(home/staging)` целиком, что может удалить staging параллельно работающего CLI `--install`.

**Suggestion:** Показывать окно сразу, а sweep выполнять в фоне (`spawn_blocking`) после старта Builder; удалять только устаревшие staging-каталоги под межпроцессной блокировкой.

### 21. [WARNING] `src-tauri/src/commands.rs:112`: хэширование пакетов на UI-потоке

**Confidence:** 91%

**Problem:** `list_apps` вызывает `registry::list`, который для каждого приложения делает `verify_for` → `verify_package` → чтение и SHA-256 каждого объявленного файла. `open_app` (строка 292) повторяет полную проверку при каждом открытии. Обе команды синхронные, поэтому задержка UI растёт с суммарным размером пакетов.

**Suggestion:** Перевести команды в `async` + `spawn_blocking` и кэшировать результат проверки по mtime/хэшу манифеста до изменения пакета.

### 22. [SUGGESTION] `src-tauri/src/lib.rs:236`: CLI-проверка расходится с запуском

**Confidence:** 88%

**Problem:** `--verify`/`--check` идут через `verify_by_policy`, которая в local-режиме делает только `verify_integrity` без seal, тогда как `registry::verify_for` (используется `open_app`/`list`) проверяет seal. Пакет, отвергаемый при запуске, может успешно пройти CLI-проверку, и наоборот.

**Suggestion:** Направить CLI-проверку через `registry::verify_for` или общий верификатор, чтобы семантика seal не расходилась.

### 23. [SUGGESTION] `src-tauri/src/commands.rs:503`: MCP URL без HTTPS

**Confidence:** 90%

**Problem:** `mcp_server_save` проверяет только непустой URL, а `McpSession` отправляет `Authorization: Bearer <token>` на сохранённый адрес. `http://`-endpoint передаёт токен и payload открытым текстом.

**Suggestion:** Требовать `https://` (исключение — loopback) при сохранении или перед прикреплением токена.

### 24. [SUGGESTION] `src-tauri/src/registry.rs:950`: zip-bomb при установке

**Confidence:** 88%

**Problem:** `extract_zip` распаковывает все записи без лимита на число и суммарный размер, причём до любой проверки подписи/хэшей. Недоверенный `.zip` может занять диск и CPU на этапе установки.

**Suggestion:** Ограничить количество записей и суммарный распакованный размер, прерывая извлечение при превышении.

### 25. [SUGGESTION] `src-tauri/src/pack.rs:619`: мёртвый `#[tauri::command]`

**Confidence:** 92%

**Problem:** Атрибут `#[tauri::command]` стоит на приватном `tool_info(home: &Path)`, который не зарегистрирован в `invoke_handler` (регистрируется только `pack::pack_tool_info`, `lib.rs:51`) и вызывается лишь как обычная функция. Сгенерированный wrapper нигде не используется.

**Suggestion:** Убрать атрибут, оставив `tool_info` обычным helper.

### 26. [SUGGESTION] `tools/pack/src/core.rs:66`: второй валидатор версии

**Confidence:** 88%

**Problem:** `semver_ok` допускает произвольную длину числовых частей и не тримит пробелы, а `manifest::parse_version` (`manifest.rs:151`) тримит и парсит в `u64`. Входы вроде `"1.2.3 "` или переполняющий компонент могут проходить один валидатор и падать на другом (`eco-pack validate` vs `--install`).

**Suggestion:** Вынести один парсер/валидатор semver и использовать его в платформе и в `eco-pack`.

### 27. [SUGGESTION] `src/app.js:2069`: полный обход артефактов на каждое открытие настроек

**Confidence:** 90%

**Problem:** Клик по вкладке настроек вызывает `refreshArtifacts`, которая дергает `artifacts_usage` и `artifact_dumps`; `artifacts::usage` рекурсивно обходит и делает `metadata()` по каждому файлу всех сессий. Стоимость растёт со всем объёмом артефактов, а не с изменениями; то же происходит при инициализации (`app.js:2424`).

**Suggestion:** Кэшировать usage/список дампов и инвалидировать по событиям tool-call/dump; полный пересчёт — только по явной кнопке обновления.

### 28. [SUGGESTION] `src/app.js:788`: отпечаток по всему тексту на каждое событие

**Confidence:** 89%

**Problem:** `chatItemKey` конкатенирует весь текст каждой записи истории; `renderChat` вызывает его для всех `ctx.chatItems` и стриминговых элементов до ~12 раз в секунду (события `chat://stream` каждые 80 мс). Плюс `ctx.chatItems`/DOM хранят всю историю без ограничения, поэтому длинные сессии деградируют по CPU и памяти.

**Suggestion:** Ключевать существующие элементы стабильным id/индексом вместо полного текста и ограничить/виртуализировать отрисованную историю (например, последние N сообщений с «загрузить раньше»).

## Примечание к ревизии отчёта

Первоначальная формулировка пункта 4 утверждала, что файлы вне `hashes` не аутентифицированы и что это критично для проверяемого пакета. Это было скорректировано: `eco-pack build` хэширует промпт, все skills и все `tools[].bin`, поэтому у корректно собранного пакета покрыто всё используемое. Остаётся платформенный пробел: верификатор не отклоняет лишние файлы и не требует полного покрытия, из-за чего в corporate-режиме добавленный skill загружается без аутентификации. Уровень понижен с CRITICAL до WARNING.


## Статус исправлений (2026-11-15)

Исправления внесены в рабочее дерево поверх `cb32dcb`; коммит не создавался.

| Пункт | Статус |
|---|---|
| 1. Потеря чатов/артефактов при install/rollback | Исправлено: `park_runtime`/`restore_runtime` переносят `engine/` и `sessions/` через замену пакета |
| 2. Апгрейд сбрасывал install-mode | Исправлено: NSIS пишет файл только при первом install |
| 3. Глобальные DPAPI-секреты | Исправлено: `llm_api_key` и `mcp.*` запрещены для `tools[].secrets` (проверка в verify и в proxy) |
| 4. Файлы вне `hashes` | Частично: обязательное покрытие промпта, всех `SKILL.md` и всех `tools[].bin`; прочие файлы по-прежнему разрешены |
| 5. Re-seal при несовпадении seal | Исправлено: fallback только когда seal отсутствует |
| 6. Path install-mode из env и fail-open | Исправлено: путь из HKLM (через `allowlist::program_data`), битый файл → corporate |
| 7. dev-anchor через `ECO_PLATFORM_DEV=1` | Не менялось: документированный dev-режим; на машине с machine-anchor он не действует |
| 8. Отзыв allowlist для запущенного движка | Исправлено: allowlist перепроверяется на каждый `tools/call` |
| 9. Глобальный `external_writes` | Исправлено: проверка по id конкретного подключения |
| 10. Corrupt manifest обходил downgrade | Исправлено: нечитаемый установленный манифест — жёсткая ошибка |
| 11. Install при работающем движке | Исправлено: движок приложения останавливается, install в `spawn_blocking` |
| 12. Неатомарная запись config | Исправлено: temp + `sync_all` + rename, внутрипроцессная блокировка, предупреждение при битом JSON |
| 13. Фактический TTL артефактов 24 ч | Исправлено: `orphan_hours` равен `ttl_hours` |
| 14. Осиротевший движок при сбое старта | Исправлено: `ChildGuard` + pid пишется сразу после spawn |
| 15. Гонка двух `chat_open` | Исправлено: набор `starting` в `AppState` |
| 16. Синхронные chat-команды | Исправлено: `chat_list/new/delete/history/reset/stop` → `spawn_blocking` |
| 17. Таймаут readiness | Исправлено: отдельный агент с таймаутом 2 с |
| 18. Утечка SSE-потока | Исправлено: `timeout_read(120 с)` у stream-агента |
| 19. Полное чтение файла | Исправлено: `read_capped_file` читает не больше лимита |
| 20. Тяжёлый startup sweep | Исправлено: выполняется после создания окна; staging чистится только старше 6 ч |
| 21. Хэширование пакетов на UI-потоке | Исправлено: `list_apps`/`open_app` → `spawn_blocking` |
| 22. CLI verify без seal | Исправлено: для установленного каталога используется `registry::verify_for` |
| 23. MCP URL без HTTPS | Исправлено: `https://` или loopback `http://` |
| 24. Zip-bomb | Исправлено: лимиты на число записей и распакованный размер |
| 25. Мёртвый `#[tauri::command]` | Исправлено: атрибут удалён |
| 26. Расхождение semver | Исправлено: `semver_ok` тримит и парсит `u64` |
| 27. Полный обход артефактов на вкладке | Исправлено: авто-refresh при переключении вкладки убран |
| 28. Отпечаток по всему тексту истории | Исправлено: ключ по длине текста, последний элемент перестраивается всегда; виртуализация истории не добавлена |

Проверки:

- `cargo test --manifest-path src-tauri/Cargo.toml` — 70 passed (добавлены регрессионные тесты на сохранение runtime-состояния, покрытие hashes, reserved secrets и scope `external_writes`);
- `cargo test --manifest-path tools/pack/Cargo.toml` — 11 passed;
- `cargo check` — без предупреждений;
- `npm test` в `tools/ui-smoke` — `ui smoke ok`.

Не запускалось: `tools/e2e-docx.ps1` (нужна сборка PyInstaller exe) и полная сборка NSIS-установщика.

Остаточные риски: dev-anchor по env (п. 7) оставлен как документированный режим; строгая «закрытость» пакета (запрет любых лишних файлов, п. 4) не введена; межпроцессная блокировка `config.json` не реализована.


## Статус исправлений второго ревью (2026-11-15)

| Пункт второго ревью | Статус |
|---|---|
| Обход фильтра секретов через регистр | Исправлено: `manifest::is_reserved_secret` сравнивает имя в нижнем регистре; используется и в verify, и в proxy |
| `restore_runtime` удалял запаркованные данные | Исправлено: `restore_runtime` возвращает `Result`, пересоздаёт `dest`, сохраняет `keep-*` при неудаче; добавлен `recover_runtime` при старте |
| Частичный `park_runtime` | Исправлено: при ошибке второго rename уже перемещённые каталоги возвращаются назад |
| Фоновый sweep убивал новый движок | Исправлено: `kill_leftover` вынесен в синхронный `startup_reap` до создания окна; sweep остался только для hashing/артефактов |
| Обход loopback через userinfo | Исправлено: `@` в authority отклоняется, поддержан bracketed IPv6, проверка повторяется в `McpSession::connect` |
| Старые `http://` MCP-эндпоинты | Исправлено: валидация при подключении (fail closed с понятной ошибкой) |
| Утечка `starting` при панике | Исправлено: RAII `StartingGuard` |
| Остановка движка по непроверенному манифесту | Исправлено: `registry::install_with` вызывает хук после проверки подписи/allowlist/downgrade; `package_app_id` удалён |
| `Path::exists()` fail-open | Исправлено: `fs::metadata`, ошибки кроме `NotFound` → corporate |
| NSIS: env ProgramData и «вечный» local | Исправлено: детерминированный путь `$WINDIR\..\ProgramData`, явный `/MODE=` перезаписывает pin, диалог не показывается на апгрейде |
| Покрытие хэшами ломало старые установки | Исправлено: требование покрытия только при install (`verify_*_install`); `open_app` использует обычную проверку |
| Zip-лимит после записи | Исправлено: чтение entry ограничено `take(remaining + 1)` |
| Мьютекс `engines` на HTTP | Исправлено: chat-команды берут `ChatSession`-снимок и выполняют HTTP вне блокировки |
| Дорогая allowlist-проверка на вызов | Исправлено: кэш на 5 секунд с привязкой к версии приложения |
| Отравленный `CONFIG_LOCK` | Исправлено: восстановление через `unwrap_or_else(|e| e.into_inner())` |
| Мёртвый `Option` у `install_mode_path` | Исправлено: возвращает `PathBuf` |
| Отпечаток чата по длине | Исправлено: длина + первые/последние 32 символа; панель артефактов обновляется не чаще раза в 30 с |
| `--check` мог писать seal | Исправлено: `verify_for_readonly` без записи; безопасное сравнение canonicalize |

Проверки после второго круга:

- `cargo test --manifest-path src-tauri/Cargo.toml` — 73 passed (добавлены тесты на case-insensitive reserved secrets, восстановление `keep-*`, TLS/loopback для MCP);
- `cargo test --manifest-path tools/pack/Cargo.toml` — 11 passed;
- `cargo check` — без предупреждений;
- `npm test` в `tools/ui-smoke` — `ui smoke ok`.

Остаточный риск: тихий апгрейд без `/MODE=` сохраняет существующий `install-mode.json` (это и есть защита от сброса), поэтому для корпоративного парка апгрейд следует запускать с `/MODE=corporate`; предварительно созданный пользователем файл в ProgramData по-прежнему не отличается от установленного (для полного закрытия нужен хранение режима в HKLM рядом с anchor).

## Recommendation

**NEEDS CHANGES** — есть критические проблемы целостности пакетов, потери данных при обновлении/откате и обхода контура доверия; их нужно исправить до того, как считать сборку пригодной к раскатке.