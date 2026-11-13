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
eco-platform.exe --trust dev\keys\trusted.pub
```

## 3. Карта репозитория

| Путь | Что там |
|---|---|
| `src-tauri/src/manifest.rs` | манифест пакета, Ed25519-подпись, sha256, schema |
| `src-tauri/src/registry.rs` | установка/удаление, fail-closed, список, trust anchor |
| `src-tauri/src/docs.rs` | md-документы: ревизии, якоря, append/insert/replace/finalize |
| `src-tauri/src/artifacts.rs` | сессии, квоты, TTL, sweep осиротевших |
| `src-tauri/src/secrets.rs` | DPAPI (CryptProtectData) |
| `src-tauri/src/mcp.rs` | MCP-клиент (streamable HTTP + SSE), лимит 64 КиБ |
| `src-tauri/src/proxy.rs` | stdio-MCP шлюз: allow-list, file tools в рабочей папке, пакетные exe, аудит |
| `src-tauri/src/engine.rs` | opencode sidecar: конфиг, запуск, сессии, чат, варианты |
| `src-tauri/src/commands.rs` | 30+ Tauri-команд |
| `src-tauri/src/lib.rs` | сборка приложения, startup sweep, CLI |
| `src/app.js`, `src/index.html`, `src/styles.css` | интерфейс, без npm и сборки |
| `tools/signer` | genkey / sign / verify для пакетов |
| `tools/docx-export` | Python-скрипт markdown → docx, PyInstaller-сборка, README |
| `tools/stub-llm/server.js` | локальная заглушка OpenAI-совместимого API (порт 4098) |
| `dev-apps/*` | подписанные пакеты-примеры: `doc-writer`, `doc-lookup` |
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
- Trust anchor отдельным файлом `trusted.pub`; в UI показывается его отпечаток.
- `min_shell_version` проверяется при установке и при каждом запуске (формат `x.y.z`,
  префикс `-suffix` игнорируется). Понижение версии запрещено; установка той же версии
  разрешена как обновление контента.

### Рабочая папка и файлы
- У каждого приложения своя рабочая папка: `config.json` → `workspaces.<app_id>`.
  Если не задана — `<home>\workspaces\<app_id>`. В шапке UI есть путь и кнопка «Выбрать».
- Агент получает `list_files`, `read_file`, `write_file`, `append_file` только если
  манифест объявил `capabilities.file_scopes: ["workspace"]`.
- Все пути относительные; абсолютные, `..` и выход за папку (включая symlink/junction)
  отклоняются после канонизации. Чтение ≤ 64 КиБ, запись ≤ 1 МиБ, листинг ≤ 200 записей.
- Старая подсистема типизированных документов (`docs.rs`) осталась в коде, но её UI и
  инструменты агенту больше не выдаются: `doc-writer` работает с обычными файлами.
- Квоты/TTL артефактов сессии сохраняются; рабочая папка пользователя не удаляется.

### Пакетные инструменты и Word
- `tools[]` манифеста может указывать `bin` — исполняемый файл внутри подписанного
  пакета. Он проверяется по хешу вместе с остальными файлами.
- Шлюз вызывает `<package>\<bin>` с фиксированными аргументами; модель задаёт только
  относительные `input`/`output`.
- У `doc-writer` это `bin/docx-export.exe` (Python-скрипт, собранный PyInstaller) и
  инструмент `docx_export`: markdown из рабочей папки → `.docx` там же.
- Конвертер является частью мини-приложения, а не платформы. Платформа не ищет его
  по глобальным путям и не запускает свои экспорты.

### Секреты
- DPAPI (per-user) в `<home>\secrets\<name>.bin`, в конфиг не попадают.
- UI: «Ключ модели (DPAPI)» + произвольные секреты; CLI `--secret-add`, `--import-auth`.

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
- Запуск движка — в `spawn_blocking`, UI не блокируется. Если приложение упало жёстко,
  следующий запуск убивает старый движок по `<engine>\engine.pid`.
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
`--verify`, `--install`, `--list`, `--trust`, `--secret-add`, `--import-auth`,
`--mcp-probe`, `--mcp-call`, `--mcp-add`, `--mcp-call-app`,
`--chat <app> <text> [low|medium|high]`, `--chat-async`, `--chat-sessions`.

### Тесты
`cargo test --manifest-path src-tauri/Cargo.toml` — 19 тестов: подпись/хеши/схема,
ревизии и якоря документов, path traversal, sweep артефактов, разбор MCP (JSON/SSE),
лимит ответа, разбор endpoint, allow-list, file tools и path escape шлюза.

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

## 6. Что НЕ реализовано (важно)

### Блокеры для «настоящего» продукта
1. **Нет сборки инсталлятора.** `bundle.active: false`, нет Authenticode-подписи,
   установки в `Program Files`, WebView2-бандла, CI, SBOM, SAST/SCA.
2. **opencode не вендорен в дистрибутив**: бинарь 211 МБ лежит в `tools/oc`
   (gitignored), провайдер `@ai-sdk/openai-compatible` opencode ставит сам через npm
   при первом старте. Для закрытого контура это недопустимо — нужен vendor.

### Функциональные пробелы
3. Нет полной матрицы совместимости платформа × пакет, реестра версий и отката.
   Проверяются только `min_shell_version` и запрет downgrade.
4. Нет артефактов-дампов, grep/query по NDJSON, квот для них; `artifacts.rs` сейчас
   считает пустые сессии.
5. Нет идентичности агента: маркировки операций, короткоживущих токенов, отзыва.
   Ключ — пользовательский, из DPAPI.
6. Нет аудита вызовов LLM (MCP-вызовы агента уже пишутся в `audit/` со `source: agent`),
   нет отправки в Observability. **Отправка отложена** — настраивать пока не на чем.
7. Нет песочницы: движок и инструменты работают с правами пользователя. Встроенные
   tools выключены, но изоляции процесса/ФС нет.
8. Нет маскирования ПДн/псевдонимизации. **Не требуется** на текущем этапе.
9. При смене приложения движок закрывается — незавершённый ход теряется.
10. `tools[].argv` не используется: пакетные инструменты запускаются по `tools[].bin`
    с фиксированными аргументами, остальное — задел.
11. Заголовки старых сессий — имя приложения; новые берутся из первого сообщения.
12. Нет обновления контента, отката, ротации ключей подписи, защиты от downgrade.

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
- crates.io и registry.npmjs.org доступны **на этой машине**. В целевом контуре —
  нет, нужен vendor/mirror.
- **rustls не проходит** к `mcp.context7.com` (таймаут 60 с), Schannel проходит.
  Поэтому `ureq` собран с `native-tls`.
- opencode требует `@ai-sdk/openai-compatible`; при старте ставит его в
  `<engine>\config\opencode\node_modules` (нужен npm).
- Шлюз инструментов — тот же `eco-platform.exe` с `--mcp-proxy <app> <workspace>`; у
  движка подменены `HOME`/`XDG_*`, поэтому рабочая папка и home передаются абсолютными
  (`canonicalize`, на Windows с префиксом `\\?\`).
- Файловые инструменты работают только в рабочей папке приложения; путь проверяется
  канонизацией. Пакетный exe запускается из каталога установленного пакета.
- Стриминг идёт через `/global/event` (не `/event`): per-instance поток не видит
  события сессии, если её каталог отличается от `cwd` движка. В `/global/event`
  событие лежит в `payload`, сверху — `directory`/`project`; синк-события (`sync`)
  игнорируются.
- Для пересборки `docx-export.exe` нужен Python + PyInstaller; офлайн — wheelhouse
  (см. `tools/docx-export/README.md`), затем скопировать exe в
  `dev-apps/doc-writer/bin`, обновить хеш и переподписать манифест.
- PowerShell 5.1: не использовать `Get-Content`/`Set-Content` на UTF-8 файлах без
  явной кодировки — уже приводило к mojibake в `app.js`.

## 8. Данные в dev-профиле (`.data`, gitignored)

- `trusted.pub` — anchor, отпечаток `48efcf9d72861a68`.
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

1. Инсталлятор: per-machine, Authenticode, WebView2 offline, вендоринг opencode и
   npm-пакета провайдера.
2. Реестр версий и откат (проверка `min_shell_version` и запрет downgrade уже есть).
3. Идентичность агента; отправка аудита в Observability — отложена (см. §6).

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
