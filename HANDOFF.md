# Handoff: ECO AIOps Platform

Дата: 2026-11-13. Состояние: рабочий прототип, собран и проверен на этой машине.
Назначение документа: передать сессию другому агенту без потери контекста.

Связанные документы:
- `PLAN.md` — целевая архитектура, фазы, риски, открытые вопросы.
- `dev-apps/doc-writer`, `dev-apps/doc-lookup` — примеры подписанных пакетов.

## 1. Что это

Desktop-платформа для мини-приложений на базе ИИ-агентов. Пользователь ставит один
инсталлятор и загружает подписанные пакеты: у каждого свой системный промпт, скилы,
MCP-подключения и набор инструментов. Агентский движок — opencode sidecar.

Текущий статус: платформа, реестр пакетов, MCP-клиент и stdio-шлюз, чат с движком,
история чатов, уровни reasoning, стриминг и статистика ответов. Демо-приложение
`doc-writer` работает в выбранной пользователем рабочей папке: агент читает и пишет
файлы, конвертирует markdown в Word через `docx-export.exe` из самого пакета.

## 2. Запуск (dev)

```powershell
$env:ECO_PLATFORM_HOME="D:\git\rust-ai-app\.data"   # профиль разработки
cargo run --manifest-path src-tauri/Cargo.toml
```

Собранный бинарь: `src-tauri\target\debug\eco-platform.exe`.

Если профиль не задан, данные идут в `%LOCALAPPDATA%\EcoAIOps`. Движок ищется в
`ECO_OPENCODE_BIN`, затем `<home>\engine\opencode.exe`, затем
`tools\oc\node_modules\opencode-ai\bin\opencode.exe` (dev-путь).

Установка пакета: UI «Приложения» → путь к папке или `.zip` → «Установить», либо
`eco-platform.exe --install <путь>`.

Перед первой установкой нужен trust anchor:

```powershell
eco-platform.exe --trust dev\keys\trusted.pub   # debug: dev-anchor в профиле
eco-platform.exe --anchor                       # проверить source/fingerprint/anchors
eco-platform.exe --allowlist                    # статус подписанного allowlist (если есть)
```

Allowlist (prod): `%ProgramData%\EcoAIOps\allowlist.json` + `allowlist.sig`,
подпись — от активного anchor; пример — `dev/allowlist.example.json`.

## 3. Карта репозитория

| Путь | Что там |
|---|---|
| `src-tauri/src/manifest.rs` | манифест пакета, Ed25519-подпись, sha256, schema |
| `src-tauri/src/allowlist.rs` | подписанный allowlist/CRL одобренных приложений и версий |
| `src-tauri/src/registry.rs` | установка/удаление, fail-closed, список, trust anchor |
| `src-tauri/src/artifacts.rs` | сессии, NDJSON-дампы, grep, квоты, TTL, sweep |
| `src-tauri/src/identity.rs` | короткоживущие токены приложений, отзыв, псевдоним пользователя |
| `src-tauri/src/secrets.rs` | DPAPI (CryptProtectData) |
| `src-tauri/src/mcp.rs` | MCP-клиент (streamable HTTP + SSE), лимит 64 КиБ |
| `src-tauri/src/proxy.rs` | stdio-MCP шлюз: allow-list, file tools в рабочей папке, пакетные exe, аудит |
| `src-tauri/src/engine.rs` | opencode sidecar: конфиг, запуск, сессии, чат, варианты |
| `src-tauri/src/commands.rs` | Tauri-команды платформы и чата |
| `src-tauri/src/lib.rs` | сборка приложения, startup sweep, CLI |
| `src/app.js`, `src/index.html`, `src/styles.css` | интерфейс, без npm и сборки |
| `tools/signer` | genkey / sign / verify для пакетов |
| `tools/package.ps1` | одна команда: сборка конвертера → hashes → подпись → `--verify` → `--install` |
| `tools/vendor.ps1` | вендоринг движка в `vendor/engine` (opencode + models.json + хеши) |
| `tools/build-installer.ps1` | `cargo tauri build` + dev-подпись (self-signed или `-Thumbprint`) |
| `tools/pack` | `eco-pack`: офлайн-утилита автора мини-приложения (init/validate/build/sign/dev-install) |
| `tools/build-pack.ps1` | сборка `eco-pack.exe` и Authenticode-подпись в `vendor/pack` |
| `tools/docx-export` | Python-скрипт markdown → docx, PyInstaller-сборка, README |
| `tools/stub-llm/server.js` | локальная заглушка OpenAI-совместимого API (порт 4098) |
| `tools/e2e-docx.ps1` | e2e: агент создаёт markdown и конвертирует в `.docx` |
| `tools/ui-smoke` | Playwright-прогон UI на встроенном browser-mock |
| `dev-apps/*` | подписанные пакеты-примеры: `doc-writer`, `doc-lookup` |
| `docs/APP_LIFECYCLE.md` | процесс: разработка, тесты, подпись ИБ, тиражирование, разграничение ролей |
| `.data/` (gitignored) | рабочий профиль: пакеты, секреты, аудит, конфиг |

## 4. Что реализовано

### Платформа и пакеты
- Tauri v2 + WebView2, статический фронтенд (`withGlobalTauri`), npm не нужен.
- Манифест `schema_version: 2`: `app_id`, `profile.system_prompt`, `tools[]`,
  `connections[]`, `capabilities`, `docs[]`, `artifacts`, `hashes`.
- Проверка при установке и при каждом запуске: подпись manifest.json, sha256 файлов,
  версия схемы. Любая проблема — отказ (fail-closed).
- Установка из папки и zip (защита от zip-slip), удаление, компактный переключатель
  приложений в шапке.
- Trust anchor: Machine-scope в `HKLM\SOFTWARE\EcoAIOps\TrustedPub` (пишет только
  администратор), dev-fallback — `<home>\trusted.pub` только в debug-сборке или при
  `ECO_PLATFORM_DEV=1`. В UI виден отпечаток и источник (`machine`/`dev`).
- `min_shell_version` проверяется при установке и при каждом запуске (формат `x.y.z`,
  префикс `-suffix` игнорируется). Понижение версии запрещено; установка той же версии
  разрешена как обновление контента.
- Реестр версий: перед заменой текущая версия архивируется в
  `<home>/versions/<app>/<version>__<ts>` (без `engine`/`sessions`, до 5 версий).
  Откат — явные `--rollback`/кнопка «↩ vX», с проверкой архива и записью в аудит;
  `--check <package>` показывает `new`/`update`/`same`/`downgrade`.
- Мульти-anchor (ротация): `HKLM\SOFTWARE\EcoAIOps\TrustedPub` и dev-`trusted.pub`
  хранят до 4 активных ключей (hex через запятую/перевод строки). Подпись пакета
  принимается любым из них; `--trust` добавляет ключ, `--untrust <fingerprint|pub|all>`
  убирает (последний ключ — только с `--force`); `--anchor` показывает все отпечатки.
  В UI виден `+N` при нескольких ключах.
- Подписанный allowlist/CRL: `%ProgramData%\EcoAIOps\allowlist.json` +
  `allowlist.sig` (в dev — `<home>\allowlist.json`; `ECO_ALLOWLIST_PATH` — только
  debug-сборка и никогда не перекрывает machine-документ). Формат:
  `apps: [{app_id, versions[]}]` (пустой `versions` — любая версия),
  `revoked: [app_id]`, опциональные `issued_at`/`expires_at`. Подпись Ed25519 от
  любого активного anchor; допускается несколько строк подписей для ротации.
  Проверяется при установке, запуске и откате (включая запись отказа в аудит);
  отсутствие файла не ограничивает (действует только подпись пакета), а
  битый/просроченный/неподписанный документ блокирует всё (fail-closed).
  Отдельный статус `incomplete` — доставлен `allowlist.json` без `.sig`.
  `--allowlist`, `--allowlist-verify <file>`, `--allowlist-check <app> [version]`.

### Рабочая папка и файлы
- У каждого приложения своя рабочая папка: `config.json` → `workspaces.<app_id>`.
  Если не задана — `<home>\workspaces\<app_id>`. В шапке UI есть путь и кнопка «Выбрать».
- Агент получает `list_files`, `read_file`, `write_file`, `append_file` только если
  манифест объявил `capabilities.file_scopes: ["workspace"]`.
- Все пути относительные; абсолютные, `..` и выход за папку (включая symlink/junction)
  отклоняются после канонизации. Чтение ≤ 64 КиБ, запись ≤ 1 МиБ, листинг ≤ 200 записей.
- Квоты/TTL артефактов сессии сохраняются; рабочая папка пользователя не удаляется.
- Каждый запуск движка открывает артефакт-сессию; tool-результаты ≥ 8 КиБ (и все
  усечённые) пишутся в `sessions/<id>/dumps.ndjson` с лимитом сессии. Дамп не
  ломает tool-вызов при превышении квоты. Поиск/список: `--artifacts <app>`,
  `--grep-artifacts <app> <query> [session]` и Tauri-команда `artifact_grep`.
- Типизированная docs-подсистема удалена (P6): нет `docs.rs`, doc-команд, поля `docs`
  в манифесте/реестре и UI документов. `doc-lookup` переведён на файловый сценарий.

### Пакетные инструменты и Word
- `tools[]` манифеста может указывать `bin` — исполняемый файл внутри подписанного
  пакета. Он проверяется по хешу вместе с остальными файлами.
- Шлюз вызывает `<package>\<bin>` с фиксированными аргументами; модель задаёт только
  относительные `input`/`output`.
- У `doc-writer` это `bin/docx-export.exe` (Python-скрипт, собранный PyInstaller) и
  инструмент `docx_export`: markdown из рабочей папки → `.docx` там же.
- Пакетный exe запускается с очищенным env (без `ECO_LLM_KEY`/`ECO_AGENT_TOKEN`),
  фиксированным cwd, таймаутом 60 с (override `ECO_PACKAGE_TOOL_TIMEOUT_SECS`),
  лимитом 64 КиБ на stdout/stderr и kill дерева процессов; причина ошибки
  пишется в аудит (`problem`).
- Конвертер является частью мини-приложения, а не платформы. Платформа не ищет его
  по глобальным путям и не запускает свои экспорты.
- Пересборка и подпись пакета — `tools/package.ps1` (см. §7).

### Дистрибутив и закрытый контур
- `tools/vendor.ps1` собирает `vendor/engine` (`opencode.exe` + `models.json`),
  Tauri-бандл кладёт его в `engine/` рядом с `eco-platform.exe`.
- `bundle.active: true`: NSIS `perMachine`, WebView2 `offlineInstaller`.
- Движок запускается с `OPENCODE_DISABLE_MODELS_FETCH`, `..._DEFAULT_PLUGINS`,
  `..._LSP_DOWNLOAD`; провайдер встроен в бинарь, npm в рантайме не нужен.
- Установщик подписывается dev self-signed сертификатом (`tools/build-installer.ps1`);
  для поставки нужен сертификат УЦ/ИБ.

### Секреты
- DPAPI (per-user) в `<home>\secrets\<name>.bin`, в конфиг не попадают.
- UI: «Ключ модели (DPAPI)» + произвольные секреты; CLI `--secret-add`, `--import-auth`.

### Идентичность агента и аудит LLM
- При старте движка платформа выпускает короткоживущий токен приложения
  (12 ч, `identity/tokens.json` хранит только sha256). Сырой токен не пишется
  на диск: он передаётся через env движка и подставляется в конфиг как
  `{env:ECO_AGENT_TOKEN}` (и в provider headers, и в environment MCP-сервера).
- Запись хранилища атомарная (temp + rename) и под межпроцессным lock-файлом;
  повреждённый `tokens.json` — ошибка (fail-closed), а не пустой store.
- Каждый вызов LLM маркируется `X-Agent-Id`, `X-Agent-Version`, `X-User`
  (стабильный псевдоним `u_<hex>`), `X-Request-Id`, `X-Agent-Token-Id`.
  Сам токен на внешний LLM-gateway не отправляется.
- Шлюз проверяет токен при старте и на каждом вызове инструмента; отозванный или
  истёкший токен — отказ. Remote MCP-вызовы получают `X-Agent-Id`, `X-User`,
  свежий `X-Request-Id` и `X-Agent-Token-Id`.
- Аудит: `action: llm` на каждый ход (app/version, token_id, request_id, модель,
  tokens in/out/reasoning, ms, ok) и `action: tool`/`tool_denied`; содержимое
  сообщений не пишется. `token_issue`, `token_revoke`, `app_revoke`,
  `app_unrevoke` — в `audit/<app>.jsonl`.
- `--revoke <token-id>` отзывает один токен; `--revoke <app-id>` ставит durable
  маркер: перезапуск приложения не выпустит новый токен до `--unrevoke <app-id>`.
  Обе формы останавливают движок; `--tokens` показывает токены и отозванные app.
- `engine.pid` хранит JSON `{pid,image,created}`; kill выполняется только если
  процесс совпадает по образу/времени создания, а `taskkill` подтвердил успех.
  `Engine::Drop` удаляет pid-файл только если тот всё ещё принадлежит ему.

### MCP
- Клиент: `initialize` → `tools/list` → `tools/call`, JSON или SSE, `Mcp-Session-Id`,
  Bearer-токен, лимит ответа 64 КиБ, таймауты.
- Определения серверов (URL может содержать секретный путь) лежат в DPAPI.
- Ручной вызов из UI: сервер + инструмент + JSON-аргументы, с проверкой allow-list
  по подписанному манифесту и записью в `audit/<app>.jsonl`.
- **TLS: используется native-tls (Schannel), не rustls.** В этом контуре rustls
  упирается в таймаут на `mcp.context7.com`, Schannel работает (см. раздел 7).

### Движок и чат
- opencode 1.19.5 как sidecar, один инстанс на открытое приложение.
- Конфиг генерируется платформой: провайдер `gw` (`@ai-sdk/openai-compatible`),
  единственный local MCP-сервер `platform` (`eco-platform.exe --mcp-proxy <app> <session>`),
  `permission: {"*":"deny"}` + точечные allow на `platform_*`, все встроенные tools
  выключены, автообновление и sharing off.
- Подменённые `HOME`/`USERPROFILE`/`XDG_*`, случайный пароль сервера + Basic auth.
- Секреты передаются через env: в конфиге `{env:ECO_LLM_KEY}`, значение в DPAPI
  (проверено по заголовку, который видит заглушка).
- Запуск движка — в `spawn_blocking`, UI не блокируется. Осиротевший движок после
  жёсткого падения прибирается при старте платформы по `engine.pid` (pid/образ/время
  создания), а также перед повторным запуском того же приложения.
- Переключение приложения не останавливает движок: незавершённый ход продолжается и
  при возврате подтягивается финальная история + busy. Возврат в каталог
  (`close_all_apps`) останавливает все движки; смена рабочей папки — только движок
  этого приложения.
- Модель и endpoint — из настроек; ключ — из DPAPI. Поддержан вариант модели с
  `reasoningEffort: low|medium|high` (передаётся как `variant`). Проверено на шлюзе:
  длина reasoning меняется.
- Чаты: список, создание, переключение, удаление; заголовок из первого сообщения;
  сессии сохраняются между запусками (проверено 11 → 12 при создании нового).
- Прогресс: SSE `/global/event`, события `chat://stream|update|busy|error` с id сессии.
  Стрим показывает reasoning и текст по мере генерации (троттлинг 80 мс), финальная
  история приходит из `GET /session/{id}/message` после завершения хода.
  «Занят» — на уровне сессии, можно работать в другом чате параллельно.
- Стоп: `/session/{id}/abort`; таймер «думает… N с».
- Рендер: markdown (заголовки, списки, код, жирный), блок «Размышления» (свёрнут),
  статистика (вход/выход/reasoning, tok/s, время, модель), кнопка «Копировать».
- Аудит ручных MCP-вызовов: `audit/<app>.jsonl`.

### Шлюз инструментов (MCP-прокси)
- Тот же бинарь в режиме `--mcp-proxy <app_id> <workspace>`; открывается по stdio.
- Движок видит один сервер `platform`:
  - файловые инструменты, если у пакета `file_scopes: ["workspace"]`;
  - пакетные инструменты из `tools[].bin` (например, `docx_export`);
  - remote MCP-инструменты объявленных `http-mcp` подключений (`<connection>_<tool>`).
- Write-инструменты remote MCP не выдаются без `capabilities.external_writes`; файловые
  и пакетные инструменты управляются `file_scopes` и подписью пакета.
- Каждый вызов: проверка манифеста (fail-closed), allow-list, лимит 64 КиБ, запись в
  `audit/<app>.jsonl` с `source: "agent"`, `failed`, `took_ms`.
- Файловые пути канонизируются и обязаны остаться внутри рабочей папки; `..`, абсолютные
  пути и symlink-выходы отклоняются.
- `docx_export` запускает `<package>\bin\docx-export.exe --input <abs> --output <abs>`;
  вход и выход — относительные пути рабочей папки.
- URL и токены MCP-серверов не попадают в конфиг движка: прокси берёт их из DPAPI.
- Проверено: `opencode mcp list` → `platform connected`; модель вызвала
  `platform_write_file` и `platform_docx_export`; файлы появились в рабочей папке.

### CLI
`--verify`, `--install`, `--list`, `--trust`, `--untrust`, `--anchor`,
`--allowlist`, `--allowlist-verify`, `--allowlist-check`, `--artifacts`,
`--grep-artifacts`, `--secret-add`,
`--import-auth`, `--mcp-probe`, `--mcp-call`, `--mcp-add`, `--mcp-call-app`,
`--chat <app> <text> [low|medium|high]`, `--chat-async`, `--chat-sessions`,
`--tokens [app]`, `--issue-token <app> [ttl-hours]`, `--revoke <token-id|app-id>`,
`--unrevoke <app-id>`.

### Тесты
`cargo test --manifest-path src-tauri/Cargo.toml` — 36 тестов: подпись/хеши/схема,
path traversal, sweep артефактов, разбор MCP (JSON/SSE),
лимит ответа, разбор endpoint, allow-list, file tools и path escape шлюза,
allowlist (версии/revoked/TTL/две подписи) и ротация anchor.

## 5. Что проверено вживую

- Установка из папки и zip; подмена файла в пакете → `hash mismatch`, отказ.
- Пакет без `manifest.sig` → отказ.
- Версии: установка 0.1.0 → повторная 0.0.9 отклонена (`downgrade rejected`);
  пакет с `min_shell_version: 99.0.0` отклонён fail-closed.
- Реальный шлюз `https://routerai.ru/api/v1`, модель `~deepseek/deepseek-v4-flash-latest`:
  чат отвечает за 4–10 с.
- Вызов context7 через платформу: `resolve-library-id` 1929 байт, запись в аудит.
- Шлюз: `opencode mcp list` → `platform connected`; модель вызвала
  `platform_write_file` и `platform_docx_export`; файлы появились в рабочей папке.
- Рабочая папка: `read_file("../secret.txt")` отклонён шлюзом
  (`path must stay inside the workspace`); write/read/append работают.
- Разные уровни reasoning дают разное поведение модели.
- Стриминг: `--chat-async` печатает растущий reasoning/текст до завершения хода;
  UI-события `chat://stream` проверены через Playwright с моками Tauri (частичный
  текст, замена, финальный update, игнор позднего стрима).
- UI: список чатов, переключение, удаление, скролл списка, markdown, статистика,
  копирование — проверено через Playwright с моками Tauri.
- doc-writer: агент создал `note.md` в выбранной папке и сконвертировал в `note.docx`;
  обратное чтение Word совпало с markdown. `--chat-async` использует ту же папку.
- Отсутствие осиротевших движков после перезапуска.
- Офлайн-движок: vendored `opencode.exe` + `models.json`, мёртвый прокси, чат через
  `--chat-async` завершился (`prompt finished`), внешних TCP-соединений у движка нет,
  npm/node_modules в профиле не создаются.
- Инсталляторы собраны `cargo tauri build`: MSI 285.5 МБ и NSIS 267 МБ.
  Административное извлечение MSI (`msiexec /a`) дало раскладку
  `eco-platform.exe` + `engine\opencode.exe` + `engine\models.json`; чат из этой
  раскладки без `ECO_OPENCODE_BIN` и с мёртвым прокси работает.
- Версии: установка 0.1.0 → 0.2.0 архивирует 0.1.0; повторная установка 0.1.0
  отклонена; `--rollback` вернул 0.1.0, `--history` показывает 0.2.0/0.1.0,
  в `audit/doc-writer.jsonl` появилось событие `rollback` с `source: admin`.
- Authenticode: self-signed `CN=ECO AIOps Dev`; `Get-AuthenticodeSignature` даёт
  `Valid` для `eco-platform.exe`, MSI и NSIS (сертификат добавлен в
  `CurrentUser\Root`/`TrustedPublisher`). В MSI-раскладке exe тоже подписан.
- Доверие: release-сборка игнорирует `<home>\trusted.pub` (`--anchor` → `ok:false`),
  `--trust` без прав администратора отклоняется, `ECO_PLATFORM_DEV=1` включает
  dev-anchor; установка и отказ по хешам пишутся в `audit/<app>.jsonl`.
- P10: два активных anchor (dev-key + второй ключ), `--trust` добавил второй
  (`added:1`), пакет прошёл проверку; allowlist, подписанный **вторым** ключом,
  дал `status:valid`, `--allowlist-check` — `allowed`; запрет чужого приложения и
  версии `0.2.0` отклонён, установка отклонена с `reason:allowlist`; revoked
  заблокировал `--chat-async` до старта движка (`action:open`, `reason:allowlist`);
  подмена `allowlist.json` без переподписи дала `status:invalid`; `--untrust`
  по отпечатку оставил один anchor.
- P7: `tools\e2e-docx.ps1` на реальном LLM создал `note.md` и `note.docx`
  (37 КБ, ZIP-заголовок), аудит `platform_write_file` + `platform_docx_export`;
  `tools/ui-smoke` (`npm test`) прошёл: каталог, allowlist-бейдж, disabled у
  недоверенного пакета, markdown/статистика чата, переключение приложения.
- P11: `read_file` на 14 КБ через `--mcp-proxy` создал
  `sessions/s-test-artifacts/dumps.ndjson` (14486 байт); `--artifacts` и
  `--grep-artifacts doc-writer ARTIFACTNEEDLE` нашли запись, токены/тестовые
  файлы после проверки удалены.
- `cargo test` — 36 тестов, включая detection/derive старых заголовков и схему
  пакетного инструмента с `input`/`output`.
- `eco-pack` (release, Authenticode `Valid`): `init` → `validate` (до build падает на
  непокрытых hashes) → `build` (zip) → `keygen` → `sign` → `verify` → `dev-install`
  в отдельный профиль через debug-платформу; пакет виден в `--list` как trusted,
  установка попала в аудит.
- Идентичность (P4): `--issue-token` → `--tokens` → `--revoke <app>` (durable
  маркер: `--issue-token` и `--chat-async` отказывают до `--unrevoke`);
  `--mcp-proxy` с отозванным токеном завершается `agent token ... is revoked`,
  с валидным — `initialize` отвечает; токен не пишется на диск (tool-вызов
  работает только через env-подстановку); `--revoke` с чужим pid отказывается
  убивать процесс; `agent.token` больше нет, `engine.pid` удаляется после
  остановки; в аудите `action: tool` с `token_id`/`request_id`/`agent_id`,
  затем `action: llm` с токенами и `request_id`.
- P5: `docx_export` в живом ходе сконвертировал markdown в `.docx` при очищенном
  env; unit-тест `package_runner_enforces_timeout_and_captures_output` прерывает
  зависший процесс по таймауту.

## 6. Что НЕ реализовано (важно)

### Блокеры для «настоящего» продукта
1. **Подпись есть, но dev-сертификатом.** `bundle.active: true`, NSIS `perMachine`,
   WebView2 `offlineInstaller`; MSI/NSIS собираются. `tools/build-installer.ps1`
   создаёт self-signed `CN=ECO AIOps Dev` и подписывает exe/MSI/NSIS (SHA-256).
   Для поставки нужен сертификат УЦ/ИБ (thumbprint или `signCommand` для HSM),
   CI, SBOM, SAST/SCA; установка на чистой машине не проверялась. Для офлайн-сборки
   нужны кеши Tauri: `%LOCALAPPDATA%\tauri\{NSIS,WixTools314,x64}` (там bootstrapper
   WebView2) или `fixedRuntime` с локальной папкой рантайма.
2. **opencode вендорен скриптом, а не артефактом в git.** `tools/vendor.ps1` кладёт
   `opencode.exe` и `models.json` в `vendor/engine` (gitignored); платформа ищет бинарь
   в `<home>/engine`, рядом с exe (`engine/opencode.exe`) и `tools/oc`. С флагами
   `OPENCODE_DISABLE_MODELS_FETCH`/`DEFAULT_PLUGINS`/`LSP_DOWNLOAD` движок не ходит
   в сеть и не требует npm: провайдер `@ai-sdk/openai-compatible` встроен в бинарь.

### Функциональные пробелы
3. Нет полной матрицы совместимости платформа × пакет. Реестр версий, архив и
   `--rollback` сделаны (P3); проверяются `min_shell_version` и запрет downgrade.
4. ~~Нет артефактов-дампов, grep/query по NDJSON, квот~~ — сделано (P11):
   движок открывает артефакт-сессию, крупные tool-результаты пишутся в
   `dumps.ndjson`, есть квоты/TTL, `--artifacts`, `--grep-artifacts`, `artifact_grep`.
5. ~~Нет идентичности агента~~ — сделано (P4): короткоживущие токены, отзыв,
   маркировка `X-Agent-Id`/`X-User`/`X-Request-Id`; токен локальный, не от
   централизованной системы ключей.
6. Нет аудита вызовов LLM в Observability: локальный `action: llm` уже пишется,
   отправка отложена — настраивать пока не на чем.
7. Нет OS-песочницы: движок и инструменты работают с правами пользователя.
   Встроенные tools выключены. Пакетные exe получают очищенный env, таймаут,
   лимит вывода и kill дерева, но это не изоляция ФС/сети.
8. Нет маскирования ПДн/псевдонимизации. **Не требуется** на текущем этапе.
9. ~~При смене приложения движок закрывается~~ — сделано (P7): движки живут до
   возврата в каталог, ход продолжается в фоне.
10. ~~`tools[].argv` не используется~~ — сделано (P7): плейсхолдеры `{input}`/`{output}`.
11. ~~Заголовки старых сессий~~ — сделано (P7): фоновое переименование из первого
    сообщения.
12. Нет обновления контента и защиты от downgrade. Откат, ротация ключей и
    подписанный allowlist/CRL сделаны (P3/P10).

### Решения по объёму (2026-11-13)

- **Разбор логов удалён.** `eco-logs` и его инструменты убраны из репозитория и профиля.
- **`doc-writer` — файловый агент.** Пользователь выбирает рабочую папку; агент получает
  `list_files`/`read_file`/`write_file`/`append_file` и не может выйти за папку.
- **Конвертер принадлежит мини-приложению.** `docx-export.exe` лежит в
  `dev-apps/doc-writer/bin`, включён в `hashes` и подпись пакета. `tools/docx-export`
  — исходник и сборка PyInstaller; платформа не ищет конвертер по глобальным путям.
- **Отправка в Observability отложена** — пока не на чем настраивать; остаётся локальный
  аудит `audit/<app>.jsonl`.
- **Маскирование ПДн/псевдонимизация не требуются** на данном этапе.

## 7. Окружение и особенности

- Windows 10/11, rustc/cargo 1.97.1, MSVC Build Tools 2022, WebView2 157,
  Node 22 (npm работает только через `npm.cmd`; `npm.ps1` блокируется
  ExecutionPolicy), Python не требуется.
- Tauri CLI 2 установлен (`cargo install tauri-cli --version "^2"`); `cargo tauri build`
  в `src-tauri` собирает MSI и NSIS. NSIS/WiX/WebView2-bootstrapper скачиваются при
  первой сборке и кешируются в `%LOCALAPPDATA%\tauri`.
- `signtool.exe` есть в Windows Kits 10.0.26100 (Tauri находит сам).
  Dev-подпись: `CN=ECO AIOps Dev`, self-signed, `Cert:\CurrentUser\My`,
  thumbprint `FE1D3554E6A62D645A94E77D9E7D437827B45F05` (до 2029-11-14, ключ
  non-exportable). `tools/build-installer.ps1 -Trust` дополнительно кладёт его в
  `CurrentUser\Root`/`TrustedPublisher` — только для локальной проверки.
  Tauri после бандлинга оставляет `target\release\eco-platform.exe` без подписи,
  поэтому скрипт подписывает его повторно; подписанные копии внутри MSI/NSIS.
- crates.io и registry.npmjs.org доступны **на этой машине**. В целевом контуре —
  нет, нужен vendor/mirror.
- **rustls не проходит** к `mcp.context7.com` (таймаут 60 с), Schannel проходит.
  Поэтому `ureq` собран с `native-tls`.
- opencode **не требует npm на старте**, если заданы офлайн-флаги: провайдер
  `@ai-sdk/openai-compatible` и его зависимости встроены в бинарь. Проверено: чат через
  vendored `opencode.exe` при мёртвом прокси и нулевых внешних соединениях.
- Дистрибутив движка: `tools/vendor.ps1` → `vendor/engine` (`opencode.exe`,
  `models.json`, `VERSION.txt`, `sha256.json`). Платформа передаёт
  `OPENCODE_MODELS_PATH`, если `models.json` лежит рядом с бинарём; иначе модели
  берутся встроенные, без похода на models.dev.
- Шлюз инструментов — тот же `eco-platform.exe` с `--mcp-proxy <app> <workspace>`; у
  движка подменены `HOME`/`XDG_*`, поэтому рабочая папка и home передаются абсолютными
  (`canonicalize`, на Windows с префиксом `\\?\`).
- Файловые инструменты работают только в рабочей папке приложения; путь проверяется
  канонизацией. Пакетный exe запускается из каталога установленного пакета.
- Стриминг идёт через `/global/event` (не `/event`): per-instance поток не видит
  события сессии, если её каталог отличается от `cwd` движка. В `/global/event`
  событие лежит в `payload`, сверху — `directory`/`project`; синк-события (`sync`)
  игнорируются.
- Пакет собирается и подписывается одной командой:
  `powershell -File tools\package.ps1` (сборка `docx-export.exe` через PyInstaller,
  пересчёт `hashes` для prompt/skills/bin, подпись `dev/keys/dev.key`, `--verify`,
  `--install` в `ECO_PLATFORM_HOME` или `.data`). Ключи: `-SkipConverter` — не
  пересобирать exe; `-Wheelhouse <dir>` — офлайн-зависимости PyInstaller;
  `-NoInstall` — только собрать и проверить; `-PackageDir` — другой пакет.
  Офлайн-сборка конвертера: wheelhouse (см. `tools/docx-export/README.md`).
- PowerShell 5.1: не использовать `Get-Content`/`Set-Content` на UTF-8 файлах без
  явной кодировки — уже приводило к mojibake в `app.js`.
- `dev-apps/**/manifest.json` и `manifest.sig` помечены `-text` в `.gitattributes`:
  подпись считается по байтам, и CRLF-конверсия Git делает её невалидной. Не убирать
  эти правила и не пересохранять манифест с другим переводом строк.

## 8. Данные в dev-профиле (`.data`, gitignored)

- `trusted.pub` — dev-anchor, отпечаток `48efcf9d72861a68`; действует только в
  debug-сборке или при `ECO_PLATFORM_DEV=1`. Release-сборка его игнорирует.
- `config.json` — endpoint `https://routerai.ru/api/v1`, модель
  `~deepseek/deepseek-v4-flash-latest`, `workspaces` (рабочие папки приложений).
- `secrets/llm_api_key.bin` — ключ шлюза, импортирован из `auth.json` Kilo,
  значение в репозиторий и логи не попадало.
- `apps/doc-writer`, `apps/doc-lookup` — установленные пакеты; у каждого свой
  `engine/` (конфиг, сессии, pid). `apps/doc-writer/bin/docx-export.exe` — конвертер
  из подписанного пакета.
- `audit/doc-lookup.jsonl`, `audit/doc-writer.jsonl`.
- `dev/keys/dev.key` + `dev/keys/trusted.pub` — dev-ключ подписи (gitignored).
- `workspaces/doc-writer` — рабочая папка по умолчанию (если пользователь не выбрал свою).
- `tools/docx-export/dist/docx-export.exe` — локальная сборка конвертера (gitignored).

## 9. Рекомендуемые следующие шаги

Подробный backlog с критериями приёмки — §11. Краткий порядок:

1. ~~Скрипт сборки/подписи пакета~~ — сделано: `tools/package.ps1` (P1).
2. Инсталлятор и вендоринг opencode/npm/WebView2 для закрытого контура (P2) —
   сделано: бандл, офлайн-движок и dev-подпись; для поставки нужен сертификат УЦ и
   проверка на чистой машине.
3. Реестр версий и управляемый откат (P3) — сделано: `--history`, `--rollback`,
   `--check`, архив `versions/`, аудит отката.
4. ~~Идентичность агента и аудит вызовов LLM~~ — сделано (P4): токены, отзыв,
   маркировка вызовов, локальный аудит LLM; Observability по-прежнему отложена.
5. ~~Таймауты/лимиты пакетных инструментов и изоляция процессов~~ — сделано (P5):
   таймаут 60 с, лимит 64 КиБ на поток, kill дерева, очищенный env, причина в
   аудите. Полной OS-песочницы по-прежнему нет.
6. ~~Чистка мёртвой docs-подсистемы, `doc-lookup` и вкладки ручных MCP-вызовов~~ —
   сделано (P6): docs-модуль/команды/поле удалены, `doc-lookup` переведён на файлы,
   вкладка MCP скрывается без `http-mcp`.
7. ~~Standalone `eco-pack` для авторов мини-приложений~~ — сделано: `tools/pack`,
   сборка/подпись `tools/build-pack.ps1` (P8).
8. Ужесточение доверия (P9) — сделано: Machine-scope anchor, release игнорирует
   пользовательский anchor, `--trust` только с elevation, аудит установок.

## 10. Известные баги/шероховатости

- `stub-llm` оставлен в репозитории как тестовый инструмент; им легко случайно
  воспользоваться, если указать его endpoint в настройках. UI предупреждает бейджем
  «локальная заглушка».
- `--chat` переиспользует последнюю сессию приложения, поэтому повторные запуски
  дописывают историю в тот же чат.
- Часть текстов в БД движка и в старых сессиях — на английском/служебная (`chat`).
- Приложение показывает каталог приложений только когда ничего не открыто; это
  ожидаемое поведение, но легко принять за пропажу списка.
- Рабочая папка по умолчанию — `<home>\workspaces\<app_id>`; её можно сменить кнопкой
  «Выбрать» в шапке (нативный диалог), после чего приложение переоткрывается.
- `docx_export` работает только если в пакете есть `bin/docx-export.exe`; иначе
  инструмента нет в списке или вызов вернёт ошибку. Платформа сама ничего не конвертирует.
- `--install`/`tools/package.ps1` падают с `os error 32`, если запущено приложение или
  остался осиротевший движок opencode: он держит `apps/<app>/engine/data/opencode.db`.
  Скрипт в этом случае подсказывает закрыть приложение; при необходимости остановить
  процесс `opencode.exe` из `tools\oc` вручную.

## 11. Backlog для следующего агента

### Контекст и инварианты

- Платформа Tauri v2, движок opencode sidecar; агент видит один MCP-сервер `platform`.
- Процесс разработки/тестов/подписи/тиражирования — `docs/APP_LIFECYCLE.md`;
  prod-подпись только у ИБ, у пользователя и разработчика prod-ключа быть не должно.
- Рабочая папка задаётся на приложение (`config.json` → `workspaces`), файловые
  инструменты не выходят за неё. Не возвращай файловый доступ без scope-проверки.
- `docx-export.exe` — часть подписанного пакета `dev-apps/doc-writer/bin`, а не
  платформы. Платформа не ищет конвертер по глобальным путям.
- UI документов и платформенный экспорт удалены вместе с `docs.rs` (P6); не возвращать
  их без явной задачи. `doc-lookup` теперь файловый пакет (`file_scopes: ["workspace"]`)
  и сохраняет выжимку в `findings.md`.
- Контур закрытый: нельзя рассчитывать на crates.io, npm и PyPI. Всё внешнее нужно
  вендорить; сборка конвертера офлайн — через wheelhouse.
- Перед изменениями: `cargo test --manifest-path src-tauri/Cargo.toml`, затем
  `cargo build`. Не коммитить `.data`, `dev/keys`, `.venv`, `dist`, `target`.
- E2E-проверка агента: `--chat-async doc-writer "создай note.md и сконвертируй в note.docx"`,
  затем проверить файлы в рабочей папке. Проверка escape: `read_file("../x")` должен
  вернуть `path must stay inside the workspace`.

### P1. Пайплайн пакета — сделано (2026-11-14)

- `tools/package.ps1`: сборка `docx-export.exe` → пересчёт `hashes` (prompt,
  `skills/**`, `tools[].bin`) → подпись `dev.key` → `--verify` → `--install`.
- Хеши считаются по фактическим файлам, новые файлы скилов добавляются сами;
  форматирование и порядок ключей манифеста сохраняются (минимальный diff).
- Проверено: полный прогон пересобрал exe, обновил хеш и `manifest.sig`, `--verify`
  вернул `ok: true`, установка прошла; повторный прогон без изменений не меняет
  манифест; подмена хеша в копии пакета исправляется прогоном.

### P2. Инсталлятор и вендоринг — сделано (2026-11-14, dev-подпись)

- `bundle.active: true`: NSIS `perMachine`, WebView2 `offlineInstaller`, ресурс
  `vendor/engine` → `engine/` рядом с `eco-platform.exe`.
- `tools/vendor.ps1` собирает вендоренный движок (`opencode.exe`, `models.json`,
  `VERSION.txt`, `sha256.json`); `vendor/` gitignored.
- Рантайм без сети и npm: флаги `OPENCODE_DISABLE_MODELS_FETCH`,
  `..._DEFAULT_PLUGINS`, `..._LSP_DOWNLOAD`, `OPENCODE_MODELS_PATH`. Провайдер
  `@ai-sdk/openai-compatible` встроен в бинарь opencode.
- Проверено: `--chat-async` с vendored бинарём и мёртвым прокси завершился успешно,
  внешних соединений 0, `node_modules`/`package.json` в профиле не создаются;
  MSI/NSIS собраны, MSI-раскладка содержит `engine\opencode.exe` и работает офлайн.
- Осталось: для поставки заменить self-signed сертификат на сертификат УЦ/ИБ
  (`-Thumbprint` или `signCommand` для HSM); проверить установку на чистой машине;
  для офлайн-сборки — кеши Tauri (NSIS/WiX/WebView2) или `fixedRuntime`; CI, SBOM,
  SAST/SCA.

### P3. Реестр версий и откат — сделано (2026-11-14)

- Перед заменой установленной версии пакет копируется в
  `<home>/versions/<app>/<version>__<ts>` без `engine`/`sessions`; хранится до 5
  версий. `--history <app>` и поле `history` в `--list` показывают архив.
- Обычный `--install` по-прежнему отклоняет downgrade. Откат — только явный
  `--rollback <app> [version]` (или кнопка «↩ vX» в UI): архив проверяется
  fail-closed, текущая версия архивируется, событие пишется в
  `audit/<app>.jsonl` (`source: admin`, `action: rollback`, from/to).
- `--check <package>` показывает `new`/`update`/`same`/`downgrade` относительно
  установленной версии.
- Проверено вживую: 0.1.0 → 0.2.0 архивирует 0.1.0; повторная установка 0.1.0
  отклонена; `--rollback` вернул 0.1.0 и записал аудит; `--check` дал `same`.
- Матрица совместимости платформа × пакет пока не формализована.

### P4. Идентичность агента и аудит LLM — сделано (2026-11-14)

- `identity.rs`: токен на приложение/версию, TTL 12 ч (CLI может задать часы),
  в `identity/tokens.json` только sha256; атомарная запись под lock-файлом;
  повреждённый store — fail-closed. Выпуск/отзыв пишутся в аудит.
- Сырой токен не персистится: env движка → `{env:ECO_AGENT_TOKEN}` в provider
  headers и в environment MCP-сервера. На внешний LLM-gateway уходит только
  `X-Agent-Token-Id`, не сам токен.
- Шлюз проверяет токен при старте и на каждом вызове; remote MCP-вызовы
  маркируются `X-Agent-Id`/`X-User`/`X-Request-Id`/`X-Agent-Token-Id`.
- Аудит LLM: `action: llm` на каждый ход; текст сообщений не пишется.
  Observability-отправка отложена.
- CLI: `--tokens [app]`, `--issue-token <app> [ttl-hours]`,
  `--revoke <token-id|app-id>`, `--unrevoke <app-id>`. App-отзыв durable:
  перезапуск не выдаёт новый токен. Некорректный TTL и неизвестная цель дают
  ошибку.
- `engine.pid` — JSON с pid/image/created; kill только при совпадении процесса и
  успешном `taskkill`; `--revoke` не рапортует успех, если движок не остановлен.
- **Приёмка:** выполнена — отозванный токен отклоняется шлюзом, app-отзыв
  переживает перезапуск, аудит содержит app/version и операцию, raw-токен не
  попадает ни в конфиг, ни в аудит.
- Осталось: токен локальный, не от централизованной системы ключей; в целевой
  модели нужен выпуск/отзыв на стороне ИБ-контура.

### P5. Изоляция и лимиты — сделано (2026-11-14)

- `run_with_limits` в шлюзе: таймаут 60 с (`ECO_PACKAGE_TOOL_TIMEOUT_SECS`),
  лимит 64 КиБ на stdout/stderr с постоянным дренажом труб, kill дерева
  процессов (`taskkill /T /F`), понятная ошибка и `problem` в
  `audit/<app>.jsonl`.
- Пакетный exe запускается с `env_clear` (не видит `ECO_LLM_KEY`/`ECO_AGENT_TOKEN`),
  фиксированным cwd рабочей папки и `CREATE_NO_WINDOW`; `TEMP/TMP/SystemRoot`
  пробрасываются для PyInstaller.
- Проверено: `docx_export` конвертирует `p5-check.md` → `.docx` (аудит:
  `took_ms`, `problem:null`); unit-тест прерывает зависший процесс по 200 мс
  таймауту и захватывает вывод.
- **Приёмка:** выполнена — зависший/бесконечно пишущий exe прерывается,
  платформа не блокируется, причина видна в аудите.
- Осталось: нет OS-песочницы (job object/AppContainer), пакетный exe всё ещё с
  правами пользователя и доступом к ФС вне рабочей папки.

### P6. Чистка после смены курса — сделано (2026-11-14)

- Удалены `docs.rs`, все doc-команды Tauri, `DocStore`, `Session`/`state.sessions`,
  поле `docs` из манифеста и `InstalledApp`, создание документов в `open_app`.
  `cargo test` — 23 теста (минус 5 тестов docs).
- `doc-lookup` переведён на файловый сценарий: версия `0.1.1`,
  `file_scopes: ["workspace"]`, skill пишет `findings.md` через `write_file`,
  манифест пересчитан и подписан dev-ключом. Вживую: context7 → `findings.md`.
- Вкладка «MCP-инструменты» показывается только пакетам с `http-mcp`; у
  `doc-writer` она скрыта, у `doc-lookup` доступна.
- `dev-apps/doc-writer/bin/docx-export.exe` оставлен в git как dev-артефакт для
  воспроизводимого локального теста (13.4 МБ); для поставки exe собирается в CI
  и входит в подписанный пакет. LFS не вводим.

### P7. Мелочи — сделано (2026-11-14)

- При старте платформа прибирает осиротевшие `opencode.exe` по
  `apps/<app>/engine/engine.pid` (kill только при совпадении pid/образа/времени),
  до запуска UI и новых движков.
- Переключение приложения больше не закрывает движок: ход продолжается в фоне,
  а при возврате история и busy-статус подтягиваются из сессии. Возврат в каталог
  (`close_all_apps`) останавливает все движки; смена рабочей папки — только движок
  этого приложения.
- Старые чаты с заголовком по имени приложения/`chat`/`Диалог` переименовываются
  в фоне из первой строки первого сообщения при старте движка.
- `tools[].argv` задействован: аргументы пакетного exe задаются в манифесте с
  плейсхолдерами `{input}`/`{output}` (по умолчанию — `--input/--output`); при
  отсутствии плейсхолдеров вызов отклоняется. Схема пакетного инструмента всегда
  объявляет `input`/`output`.
- Добавлены `tools/e2e-docx.ps1` (реальный поток «файл → docx» через
  `--chat-async`, проверка PK-заголовка) и `tools/ui-smoke` (Playwright + встроенный
  browser-mock: каталог, allowlist-бейдж, запрет недоверенного пакета, рендер чата,
  переключение приложения).
- Проверено: e2e создал `note.md` → `note.docx` (37 КБ, real LLM); `npm test` в
  `tools/ui-smoke` — `ui smoke ok`; `cargo test` — 36 тестов.

### P8. Standalone-утилита `eco-pack` — сделано (2026-11-14)

- Крейт `tools/pack` (бинарник `eco-pack`): `init` (шаблоны `minimal|files`),
  `validate`, `build` (пересчёт `hashes` + zip), `keygen`, `sign` (dev),
  `verify`, `trust`, `dev-install`. Офлайн, без внешних зависимостей; платформу и
  exe-инструменты не собирает, prod-ключей не содержит.
- `validate` проверяет схему, `app_id`, semver, пути/`..`, покрытие `hashes`,
  наличие `tools[].bin`, размеры и эвристики на секреты; `build` пересчитывает
  `hashes` по фактическим файлам и сохраняет форматирование манифеста.
- `dev-install` пересчитывает hashes, подписывает dev-ключом, кладёт anchor в
  указанный профиль и вызывает установленную платформу с `ECO_PLATFORM_DEV=1`;
  Machine-scope anchor prod-машины имеет приоритет и блокирует такой запуск.
- Сборка/подпись: `powershell -File tools\build-pack.ps1` → `vendor/pack/eco-pack.exe`
  (Authenticode dev-сертификатом, статус `Valid`).
- Проверено: init → validate (до build падает на непокрытых hashes) → build →
  validate ok → keygen/sign/verify → dev-install в отдельный профиль; пакет виден
  в `--list` как `trusted`, запись установки в аудите; `cargo test` — 9/9.
- Осталось: положить подписанный `eco-pack.exe` в доверенное хранилище и проверить
  на машине без Rust/Node/Python (сейчас проверено на dev-машине).

### P9. Ужесточение доверия — сделано (2026-11-14)

- Anchor в Machine-scope: `HKLM\SOFTWARE\EcoAIOps\TrustedPub` (hex ключа);
  пользовательский `<home>\trusted.pub` — только dev-fallback.
- Release-сборка игнорирует пользовательский anchor; `--trust` без elevation
  возвращает ошибку и ничего не пишет.
- dev-режим: debug-сборка или явный `ECO_PLATFORM_DEV=1`; `--anchor` показывает
  `source` (`machine`/`dev`), отпечаток, `elevated`, `dev_mode`; UI показывает
  источник anchor (dev — жёлтый бейдж).
- Аудит: `action: install` (`ok:true/false`, `reason: verification|downgrade|allowlist`)
  и `action: open` при отказе в проверке пакета или allowlist.

### P10. Подписанный allowlist/CRL и ротация ключей — сделано (2026-11-14)

- `allowlist.rs`: `%ProgramData%\EcoAIOps\allowlist.json` + `allowlist.sig`
  (Machine scope, доставка GPO/Intune/SCCM; путь ProgramData читается из HKLM, а не
  из пользовательской env); dev-fallback `<home>\allowlist.json`, а
  `ECO_ALLOWLIST_PATH` работает только в debug-сборке и не перекрывает machine.
- Формат: `schema_version:1`, `apps:[{app_id, versions[]}]` (пустой список — любая
  версия), `revoked:[app_id]`, `issued_at`/`expires_at`. Документ подписывается
  Ed25519-ключом ИБ; `allowlist.sig` может содержать несколько подписей (по строке),
  чтобы ротация ключей не блокировала тиражирование.
- Enforcement: одна загрузка/проверка (`AllowlistGuard`) на установку, запуск
  (`verified_manifest`), откат и `list()`; нет файла — ограничения нет;
  битый/просроченный/неподписанный/частично доставленный документ — fail-closed.
  Отказы: `reason: allowlist` в `audit/<app>.jsonl`, включая rollback.
- CLI: `--allowlist` (статус/source/fingerprint), `--allowlist-verify <file>`,
  `--allowlist-check <app> [version]`; `--check` показывает `allowlist_ok`.
  Отзыв (`--revoke`) не зависит от allowlist: использует установленный пакет.
- Ротация anchor: `TrustedPub`/`trusted.pub` хранят до 4 ключей; `--trust` добавляет
  (`added`), `--untrust <fingerprint|pub|all> [--force]` убирает (без `--force`
  последний ключ не удаляется), `--anchor` выдаёт `anchors[]`/`fingerprints[]`.
  Значение HKLM пишется в legacy-совместимом ANSI-hex (старые сборки читают),
  читатель принимает ANSI/UTF-16/REG_MULTI_SZ; усечения набора больше нет.
- Проверено вживую в отдельном профиле: 2 anchor, подпись пакета и allowlist вторым
  ключом, версия/revoked/подмена документа, отказ установки и запуска, `--untrust`.
- После ревью добавлены регрессионные тесты: `incomplete` allowlist, запрет
  удаления последнего anchor, отказ >4 ключей, аудит отказа rollback.
- Попутно исправлено: `dev-apps/doc-writer/manifest.sig` был рассинхронизирован после
  P6 (поле `docs` удалили без переподписи); переподписан dev-ключом, `signer verify`
  для `doc-writer` и `doc-lookup` проходит.
- Осталось: доставка allowlist по GPO/HTTPS и его аудит на стороне ИБ; подпись
  allowlist сейчас через `tools/signer sign <file> --key ... --out allowlist.sig`.

### P11. Артефакты, NDJSON-дампы и grep — сделано (2026-11-14)

- `artifacts.rs`: `append_dump` (NDJSON, лимит `session_quota_bytes`, отказ не
  ломает tool-вызов), `list_dumps`, `grep` (case-insensitive, лимит 100/500,
  фильтр по сессии); квоты/TTL/sweep переиспользуются.
- Движок при старте открывает артефакт-сессию (`ArtifactGuard` закрывает маркер
  при ошибке старта, `Drop` — при остановке) и передаёт её путь шлюзу.
- Шлюз (`--mcp-proxy <app> <workspace> <artifact-dir>`) пишет результат в
  `dumps.ndjson`, если он ≥ 8 КиБ или усечён; квота сессии не влияет на ответ
  модели и аудит.
- CLI: `--artifacts <app>`, `--grep-artifacts <app> <query> [session]`;
  Tauri-команда `artifact_grep`; UI `artifacts_usage`/`cleanup_artifacts` работают
  с теми же сессиями.
- Проверено: `read_file` 14 КБ через прокси → `dumps.ndjson` 14486 байт, grep
  нашёл уникальную строку; unit-тест покрывает квоту и поиск.
- Осталось: UI-поиск по артефактам (сейчас CLI/команда) и экспорт дампов.

### С чего начать

1. Прочитать §1–§8, запустить приложение по §2.
2. Проверить `--list`, `--verify dev-apps/doc-writer`, `--chat-async doc-writer ...`.
3. Пересобрать пакет через `powershell -File tools\package.ps1` (P1 сделано).
4. Собрать дистрибутив: `powershell -File tools\vendor.ps1`, затем
   `powershell -File tools\build-installer.ps1 -Trust` (P2 сделано с dev-подписью;
   для поставки — сертификат УЦ, см. §6 и §7).
5. Доверие: `eco-platform.exe --anchor`; для prod — установить anchor из-под
   администратора (HKLM), для dev — debug-сборка или `ECO_PLATFORM_DEV=1`.
   Ротация: `--trust <new.pub>` → обновить парк → `--untrust <old-fingerprint>`.
   Allowlist: подписать `dev/allowlist.example.json` (`tools/signer sign ... --out
   allowlist.sig`) и положить в `%ProgramData%\EcoAIOps\`; проверить `--allowlist`.
6. Для разработки мини-приложений внешними авторами: `tools\build-pack.ps1` →
   `vendor/pack/eco-pack.exe` (P8 сделано); проверить на машине без Rust/Node/Python.
7. P7 и P11 сделаны. Следующие функциональные пробелы: матрица совместимости
   платформа × пакет и OS-песочница. P2/P10-эксплуатация (сертификат УЦ, GPO/HTTPS)
   отложены как нереализуемые в текущем контуре.
8. Проверить идентичность: `--issue-token doc-writer 1` → `--tokens` →
   `--revoke <token_id>`; для app-отзыва: `--revoke doc-writer` →
   `--issue-token doc-writer` (должен отказать) → `--unrevoke doc-writer`;
   в `audit/doc-writer.jsonl` должны быть `token_issue`, `token_revoke`,
   `app_revoke`, `action: llm` и `action: tool`.
