---
description: Пошаговый плейбук переупаковки стороннего агента в пакет ECO AIOps
---

# Плейбук переупаковки

## 1. Инвентаризация (сначала факты)

Собери и занеси в таблицу `out/REPORT.md`:

| Что | Где | Статус |
|---|---|---|
| AGENTS.md / prompt | источник | есть/нет |
| Скилы | `skills/<имя>/SKILL.md` | список |
| MCP | `.mcp.json`, `mcp.json`, `.vscode/mcp.json`, `opencode.json(c)` | id и тип |
| Скрипты | `*.py`, `*.ps1`, `*.sh`, `*.js` | что запускают, нужна ли сеть |
| Бинарники | `bin/*.exe`, `dist/*.exe` | есть/нет |
| Секреты | `.env*`, упоминания переменных | только имена |

Не открывай `.env*` и файлы ключей. Значения не попадают ни в отчёт, ни в пакет.

## 2. Целевая раскладка

```text
out/<app_id>/
  manifest.json
  prompt/system.md
  skills/<имя>/SKILL.md      # один уровень, вложенных нет
  scripts/<tool>_adapter.py  # исходник адаптера (не обязателен в пакете)
  bin/<tool>.exe             # только если уже собран
out/SECRETS_REQUIRED.md
out/REPORT.md
out/build.ps1
```

`REPORT.md`, `SECRETS_REQUIRED.md` и `build.ps1` — рядом с пакетом, не внутри:
они не должны попадать в zip и хеши.

## 3. Переписывание промпта

| В источнике | В пакете |
|---|---|
| `вызови skill name` | `platform_skill {"action":"read","name":"..."}` |
| `platform_skill list` | то же, инструмент уже доступен |
| `py agg_tool.py analyze ...` | `platform_agg {"action":"analyze",...}` |
| `py sentry_issue.py summary <id>` | `platform_sentry {"action":"summary","issue":"..."}` |
| `C:\Users\...\.roo\skills\...` | удалить; скилы грузятся по имени |
| «рабочая директория = папка скила» | удалить; cwd = рабочая папка приложения |
| `--export "C:\...\affected.txt"` | `--export "affected.txt"` (относительно workspace) |
| `.env` | имена переменных + `tools[].secrets` |

Сохрани обязательные шаги и запреты источника (первый скил, порядок, 5 почему,
PII). Сокращай дубли: если правило живёт в скиле, в промпте — только ссылка
«работай по скилу X».

## 4. Манифест (шаблон)

```json
{
  "schema_version": 2,
  "app_id": "<slug>",
  "name": "<Название>",
  "version": "0.1.0",
  "min_shell_version": "0.1.0",
  "publisher": "",
  "profile": {
    "system_prompt": "prompt/system.md",
    "model_hint": "gateway-default",
    "max_steps": 60
  },
  "tools": [
    {
      "tool": "agg",
      "bin": "bin/agg_tool.exe",
      "io": "json",
      "parameters": {
        "type": "object",
        "properties": {
          "action": { "type": "string", "enum": ["analyze", "verify", "hist", "top", "stats", "users", "trace", "impact", "dump"] },
          "text": { "type": "string" },
          "phrase": { "type": "string" },
          "at": { "type": "string" },
          "last": { "type": "string" },
          "field": { "type": "string" },
          "regex": { "type": "string" },
          "export": { "type": "string" }
        },
        "required": ["action"]
      },
      "secrets": { "OS_URL": "os_url", "OS_USER": "os_user", "OS_PASSWORD": "os_password" },
      "stream": "read"
    }
  ],
  "connections": [
    { "id": "llm", "type": "llm", "required": true }
  ],
  "capabilities": {
    "index_allowlist": [],
    "file_scopes": ["workspace"],
    "external_writes": []
  },
  "artifacts": { "quota_mb": 128, "ttl_hours": 168 },
  "hashes": {}
}
```

Правила:

- `hashes` всегда пустой: их считает `eco-pack build`.
- `tools[].tool` — короткое имя (`agg`, `sentry`, `weblog`); модель увидит его как
  `platform_<tool>`.
- `io: "json"` — без `{input}`/`{output}` в `argv`.
- `stream: "read"` для чтения, `"write"` только для меняющих операций.
- `connections`: `llm` обязателен; remote MCP — по id, без URL/токенов.
- `external_writes` — только id MCP, которым разрешена запись.
- `file_scopes: ["workspace"]` — если агенту нужны файлы рабочей папки.

## 5. Скилы

1. Не меняй смысл `SKILL.md`; меняй только пути/вызовы инструментов.
2. Один уровень: `skills/<имя>/SKILL.md`. Вложенные папки разложи плоско;
   если это невозможно — блокер.
3. Корневой `SKILL.md` → `skills/main/SKILL.md`.
4. Во frontmatter добавь `description: ...` — по нему строится индекс.
5. Если скилов несколько, платформа отдаёт `platform_skill`; модель грузит их по
   имени. Опиши в промпте, какой скил и когда грузить.
6. Ресурсы скила (шаблоны, словари) в пакете хешируются, но `platform_skill`
   отдаёт только `SKILL.md`. Всё, что нужно модели, переноси в текст скила или в
   инструмент.

## 6. Секреты

- В источнике: `.env` + `os.getenv`/`dotenv`.
- В пакете: `tools[].secrets` (env → имя секрета DPAPI); скрипт читает
  `os.environ["..."]`.
- Имена env — `A-Z`, `0-9`, `_`; имена секретов — латиница, цифры, `-`, `_`, `.`.
- `SECRETS_REQUIRED.md` — таблица «секрет | зачем | env», без значений.
- Если секрет нужен нескольким инструментам — одно имя, один ввод.

## 7. MCP

- Поддерживаются только remote (`url`/`serverUrl`/`type: http`).
- stdio (`command`/`args`) — блокер: запиши id в `REPORT.md`, в манифест не
  добавляй.
- id в манифесте должен совпадать с id в настройках платформы.
- Инструменты видны как `<connection>_<tool>`.

## 8. Сеть и ИБ

Пакетные exe запускаются с очищенным окружением и по политике не имеют сети:
только LLM-gateway и объявленные MCP. Если `agg_tool`/`sentry_issue`/CRM-скрипт
ходят в сеть:

- зафиксируй это в `REPORT.md` как требование: `http-mcp`/сервис-прокси или
  исключение ИБ;
- не добавляй в пакет креды, прокси-переменные и сертификаты;
- не обещай, что «просто заработает».

## 9. Отчёт и следующий шаг

`out/REPORT.md` — кратко:

1. Что найдено (скилы, MCP, скрипты, бинарники).
2. Что перенесено и переписано (таблица).
3. Какие инструменты объявлены, какие exe нужно собрать (PyInstaller).
4. Какие секреты завести.
5. Блокеры: stdio MCP, сеть, вложенные скилы, отсутствующие бинарники.
6. Команды для пользователя:
   `eco-pack validate out/<app_id>` → `build` → `validate` → `sign` (dev) →
   проверка в локальном режиме.

Не называй пакет готовым, пока `eco-pack validate` не пройден пользователем.
