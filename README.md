# ECO AIOps Platform

Desktop-платформа для мини-приложений на базе ИИ-агентов. Пользователь ставит одно
Windows-приложение и устанавливает в него подписанные пакеты: у каждого свой системный
промпт, скилы, подключения и инструменты. Агентский движок — [opencode](https://opencode.ai)
sidecar, интерфейс — Tauri v2 + WebView2.

> Статус: рабочий прототип, не production. Инсталляторы и dev-подпись есть; нет
> production-сертификата Authenticode, проверки на чистой машине и песочницы
> процессов. Полный список — в `HANDOFF.md` §6 и §11.

![Превью](preview-dark.png)

Каталог подписанных пакетов:

![Каталог пакетов](preview-dark-cards.png)

## Возможности

- **Подписанные пакеты.** Ed25519-подпись `manifest.json`, sha256 файлов, проверка при
  установке и при каждом запуске. Любая проблема — отказ (fail-closed).
- **Реестр приложений.** Установка из папки и `.zip`, удаление, переключатель приложений,
  отображение версии и подписанта.
- **Рабочая папка.** У каждого приложения своя папка, которую выбирает пользователь.
  Агент читает и пишет только внутри неё.
- **Шлюз инструментов.** opencode видит один MCP-сервер `platform`: файловые инструменты,
  exe-инструменты из пакета и remote MCP. Allow-list, лимит ответа 64 КиБ, аудит.
- **Демо `doc-writer`.** Агент создаёт markdown в рабочей папке и конвертирует его в
  Word через `docx-export.exe` из самого пакета.
- **MCP.** Streamable HTTP + SSE, токены в DPAPI, ручной вызов из UI.
- **Секреты.** DPAPI (per-user), в конфиг и логи не попадают.
- **Чат.** История, переключение и удаление чатов; live-стриминг reasoning/текста через
  SSE; уровни reasoning; статистика ответов.
- **Версии.** Проверка `min_shell_version`, запрет downgrade, архив предыдущих версий,
  управляемый откат (`--rollback`/кнопка в UI) и сравнение пакета с установленной
  версией (`--check`).

Полный процесс разработки, тестирования, подписи ИБ и тиражирования —
`docs/APP_LIFECYCLE.md`.

## Архитектура

```
┌────────────────────────────────────────────────────────────┐
│ Tauri v2 shell (Rust) + WebView2                           │
│  ├─ реестр подписанных пакетов                             │
│  ├─ UI: каталог, чат, рабочая папка, настройки             │
│  └─ stdio MCP gateway (--mcp-proxy app workspace)          │
└───────────────┬───────────────────────────────┬────────────┘
                │ stdio                         │ HTTP
                ▼                               ▼
        opencode sidecar                 LLM gateway
        ├─ file tools / package exe      (OpenAI-compatible)
        └─ remote MCP (опционально)
```

Инварианты:

- агент не общается с инструментами напрямую — только через шлюз;
- приложения изолированы по пакетам, рабочим папкам и подключениям;
- пакет не может расширить свои права — только разработчик при выпуске новой версии.

## Быстрый старт (dev)

Требования: Windows 10/11, Rust/MSVC, Node.js (чтобы поставить opencode в
`tools/oc`), WebView2. Python нужен только для пересборки `docx-export.exe`.
Собранному дистрибутиву Node/npm/Python не нужны.

```powershell
$env:ECO_PLATFORM_HOME="D:\git\rust-ai-app\.data"   # профиль разработки
cargo run --manifest-path src-tauri/Cargo.toml
```

Если профиль не задан, данные идут в `%LOCALAPPDATA%\EcoAIOps`.

Установка trust anchor и демо-пакетов:

```powershell
$exe = "src-tauri\target\debug\eco-platform.exe"
& $exe --trust dev\keys\trusted.pub   # debug/dev: пишет в профиль
& $exe --install dev-apps\doc-writer
& $exe --install dev-apps\doc-lookup
& $exe --anchor                       # источник anchor и отпечаток
```

Release-сборка принимает anchor только machine-wide:
`HKLM\SOFTWARE\EcoAIOps\TrustedPub` (ставить из-под администратора; GPO/установщик).
Пользовательский anchor действует только при `ECO_PLATFORM_DEV=1` или в debug-сборке.

Движок ищется в `ECO_OPENCODE_BIN`, затем в `<home>\engine\opencode.exe`, затем в
`tools\oc\node_modules\opencode-ai\bin\opencode.exe` (dev-путь).

## Мини-приложение `doc-writer`

1. Откройте приложение и выберите рабочую папку в шапке (кнопка «Выбрать»).
2. Напишите агенту, например: «Создай `report.md` с планом проверки и сконвертируй в
   `report.docx`».
3. Агент вызовет `write_file` и `docx_export`; оба файла появятся в рабочей папке.

Конвертер — часть подписанного пакета:

```
dev-apps/doc-writer/
  bin/docx-export.exe      # Python-скрипт, собранный PyInstaller
  manifest.json            # tools[].bin + sha256 exe
  manifest.sig
  prompt/system.md
  skills/doc-workflow/SKILL.md
```

Исходник и сборка конвертера:

```powershell
cd tools\docx-export
.\build.ps1                 # соберёт и скопирует exe в dev-apps/doc-writer/bin
```

Пересборка, обновление `hashes`, подпись и установка — одной командой:

```powershell
powershell -File tools\package.ps1              # exe -> hashes -> sign -> --verify -> --install
powershell -File tools\package.ps1 -SkipConverter   # только hashes/sign/verify/install
```

Ключ `-Wheelhouse <dir>` передаёт офлайн-зависимости в сборку конвертера, `-NoInstall`
оставляет только сборку и проверку пакета. Для закрытого контура см.
`tools/docx-export/README.md`.

## Сборка дистрибутива

```powershell
powershell -File tools\vendor.ps1     # vendor/engine: opencode.exe + models.json + хеши
powershell -File tools\build-installer.ps1 -Trust   # сборка + dev-подпись (self-signed)
cargo tauri build                     # NSIS per-machine, WebView2 offlineInstaller
```

Для `cargo tauri build` нужен Tauri CLI 2 (`cargo install tauri-cli --version "^2"`).

- `bundle.active: true`; `vendor/engine` попадает в `engine/` рядом с
  `eco-platform.exe` (ресурсы Tauri).
- Движок не ходит в сеть и не требует npm: включены
  `OPENCODE_DISABLE_MODELS_FETCH`/`..._DEFAULT_PLUGINS`/`..._LSP_DOWNLOAD`, а
  провайдер `@ai-sdk/openai-compatible` встроен в бинарь opencode.
- Порядок поиска движка: `ECO_OPENCODE_BIN` → `<home>/engine/opencode.exe` →
  `engine/opencode.exe` рядом с платформой → `tools/oc` (dev).
- Для полностью офлайн-сборки нужны кеши Tauri
  `%LOCALAPPDATA%\tauri\{NSIS,WixTools314,x64}` (в `x64` — bootstrapper WebView2);
  вместо `offlineInstaller` можно указать `fixedRuntime` с папкой рантайма.
- Результат: MSI ~286 МБ и NSIS ~267 МБ в `src-tauri/target/release/bundle/`;
  административное извлечение MSI подтверждает `engine\opencode.exe` рядом с
  `eco-platform.exe`.
- Подпись Authenticode для dev-сборки:
  `powershell -File tools\build-installer.ps1 -Trust` создаёт self-signed
  code-signing сертификат `CN=ECO AIOps Dev` в `Cert:\CurrentUser\My`, подписывает
  `eco-platform.exe`, MSI и NSIS (SHA-256). `-Trust` добавляет сертификат в
  `CurrentUser\Root` и `TrustedPublisher`, иначе статус будет `UntrustedRoot`.
  Для поставки подписывать сертификатом УЦ: `-Thumbprint <sha1>` (или свой
  `signCommand` для HSM/токена). Self-signed сертификат — только для dev.

## Файловые инструменты

Доступны пакету с `capabilities.file_scopes: ["workspace"]`:

| Инструмент | Назначение |
|---|---|
| `list_files` | список файлов и папок в рабочей папке |
| `read_file` | чтение UTF-8 файла (≤ 64 КиБ) |
| `write_file` | создание/перезапись (≤ 1 МиБ) |
| `append_file` | дописать в конец |
| `docx_export` | exe из пакета: markdown → `.docx` |

Пути только относительные. Абсолютные пути, `..` и выход за папку (включая symlink)
отклоняются после канонизации.

## CLI

```text
--verify <package>          проверить подпись и хеши
--install <package>         установить пакет (downgrade отклоняется)
--check <package>           сравнить версию пакета с установленной
--history <app>             сохранённые версии для отката
--rollback <app> [version]  явный откат к сохранённой версии (в аудит)
--list                      список установленных пакетов
--trust <trusted.pub>       установить trust anchor (release: только с админом)
--anchor                    источник, отпечаток и dev/elevated
--secret-add <name> <v>     сохранить секрет в DPAPI
--import-auth <name> <json> <provider>
--mcp-probe <url>
--mcp-call <url> <tool> <json>
--mcp-add <id> <url> [token]
--mcp-call-app <app> <server> <tool> <json>
--chat <app> <text> [low|medium|high]
--chat-async <app> <text>
--chat-sessions <app>
```

Пример сквозной проверки:

```powershell
$env:ECO_PLATFORM_HOME="D:\git\rust-ai-app\.data"
.\src-tauri\target\debug\eco-platform.exe --chat-async doc-writer `
  "Создай note.md и сконвертируй в note.docx"
```

## Тесты

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

Покрыты подпись/хеши/схема, версии и downgrade, path traversal, file tools, разбор
MCP (JSON/SSE), лимит ответа, endpoint.

## Карта репозитория

| Путь | Что там |
|---|---|
| `src-tauri/src/` | платформа: реестр, шлюз, движок, секреты, аудит, команды |
| `src/` | статический UI без npm-сборки |
| `dev-apps/` | подписанные пакеты-примеры |
| `tools/signer/` | genkey / sign / verify пакетов |
| `tools/package.ps1` | сборка/подпись/установка пакета одной командой |
| `tools/vendor.ps1` | вендоринг движка в `vendor/engine` |
| `tools/docx-export/` | исходник конвертера и PyInstaller-сборка |
| `tools/stub-llm/` | локальная заглушка OpenAI-совместимого API |
| `HANDOFF.md` | состояние, ограничения и backlog для следующего агента |
| `docs/APP_LIFECYCLE.md` | как разрабатывать, тестировать, подписывать и тиражировать мини-приложения |
| `PLAN.md` | целевая архитектура и фазы |
| `.data/` | рабочий профиль (gitignored) |
| `vendor/` | артефакты дистрибутива (gitignored) |

## Ограничения

- Authenticode — только dev self-signed сертификат; нет production-сертификата УЦ и
  проверки установки на чистой машине; вендоринг движка — скриптом
  `tools/vendor.ps1`, артефакты не в git.
- Нет песочницы процессов; пакетный exe работает с правами пользователя.
- Нет идентичности агента и аудита вызовов LLM; Observability отложена.
- Docs-подсистема в backend осталась мёртвым кодом после перехода на файловый сценарий.

Подробнее — `HANDOFF.md` §6 и §11.

## Лицензия

Apache License 2.0, см. `LICENSE`.
