---
description: Шаблоны JSON-адаптеров и tools[] для скриптов с подкомандами
---

# Адаптеры инструментов

## 0. Два режима, не путай

| Что это | `io` | Как вызывается |
|---|---|---|
| Конвертер/обработчик файлов (`--input`, `--output`, «вход → выход») | не указывать (`files`) | `<bin> --input <путь> --output <путь>` |
| Сервисный скрипт/API-клиент (JSON в stdin, ответ в stdout) | `"json"` | без флагов, JSON в stdin |

Конвертер с `io: "json"` упадёт с `usage: ... --input --output`. Если не уверен,
запусти exe с `--help` и определи режим по аргументам.

MCP объявляется на двух уровнях: **сервер** — в `connections`
(`{"id","type":"http-mcp"}`), а **каждый его инструмент** — в `tools[]` с
`tool`/`stream` и без `bin` (платформа выставляет его как `<id>_<tool>`). Сам
сервер в `tools[]` не добавляй. Право записи — `capabilities.external_writes`.

## 1. Контракт платформы

- Инструмент объявляется в `manifest.json` → `tools[]`.
- `io: "json"`: один JSON-объект приходит в **stdin**, ответ — **stdout**
  (JSON или текст). `argv` пустой; `{input}`/`{output}` запрещены.
- Файловый режим (`io` нет или `"files"`): модель передаёт `input`/`output`,
  платформа подставляет их в `--input/--output` (или в `argv` с
  `{input}`/`{output}`), `stdin` пустой.
- Скрипт запускается с `cwd` = рабочая папка приложения, окружение очищено
  (кроме `TEMP`, `TMP`, `SystemRoot`, `windir`) + секреты из `tools[].secrets`.
- Модель видит инструмент как `platform_<tool>`, например `platform_agg`.
- `parameters` — JSON Schema аргументов; подкоманды — поле `action` с `enum`.
- Таймаут по умолчанию 300 с; для долгих операций задай `timeout_secs`
  (максимум 7200) и всё равно держи операции узкими (`--max`, окно).

## 2. Адаптер

```python
import json
import os
import sys


def run(req: dict) -> dict:
    action = req.get("action")
    if action == "verify":
        # вызови существующую функцию/CLI, не копируй бизнес-логику
        return {"ok": True, "action": action}
    raise ValueError(f"unknown action: {action}")


def main() -> int:
    try:
        req = json.load(sys.stdin)
    except Exception as e:
        print(json.dumps({"ok": False, "error": f"bad json: {e}"}))
        return 2
    try:
        result = run(req)
        print(json.dumps(result, ensure_ascii=False))
        return 0
    except Exception as e:
        print(json.dumps({"ok": False, "error": str(e)}, ensure_ascii=False))
        return 1


if __name__ == "__main__":
    sys.exit(main())
```

Правила:

1. Не читай `.env`; секреты — только `os.environ["NAME"]`.
2. Не меняй cwd и не пиши в папку пакета: результаты — в cwd (workspace).
3. Пути из аргументов (`export`, `out`) считай относительными workspace.
4. PII-массивы — только в файл (`export`), в stdout — число и имя файла.
5. Ошибки — JSON с `ok: false` и понятным текстом, код возврата ≠ 0.
6. `stdout` — только результат; диагностика — `stderr`.

## 3. Пример `tools[]`

```json
{
  "tool": "agg",
  "bin": "bin/agg_tool.exe",
  "io": "json",
  "parameters": {
    "type": "object",
    "properties": {
      "action": { "type": "string", "enum": ["analyze", "verify", "hist", "top", "stats", "users", "trace", "impact", "dump", "weblog"] },
      "text": { "type": "string" },
      "phrase": { "type": "string" },
      "at": { "type": "string" },
      "last": { "type": "string" },
      "bucket": { "type": "string" },
      "field": { "type": "string" },
      "regex": { "type": "string" },
      "group": { "type": "integer" },
      "export": { "type": "string" },
      "out": { "type": "string" },
      "max": { "type": "integer" }
    },
    "required": ["action"],
    "additionalProperties": false
  },
  "secrets": {
    "OS_URL": "os_url",
    "OS_USER": "os_user",
    "OS_PASSWORD": "os_password"
  },
  "stream": "read"
}
```

Если у подкоманд совсем разные аргументы и схема становится нечитаемой,
раздели на несколько инструментов (`agg_dump`, `weblog`, `sentry`) — это
нормально; главное, чтобы имена не пересекались.

## 4. Сборка

`out/build.ps1`:

```powershell
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$app = Join-Path $root "<app_id>"

py -m PyInstaller --onefile --clean --name agg_tool `
  --distpath (Join-Path $app "bin") `
  (Join-Path $root "scripts\agg_adapter.py")

# затем:
# eco-pack validate $app
# eco-pack build $app
# eco-pack validate $app
# eco-pack sign $app --key <dev.key>   # только для локального теста
```

- Если PyInstaller недоступен — оставь исходник адаптера и запиши блокер:
  пакет не пройдёт `validate` без `bin`.
- Не подменяй exe заглушкой.
- Не добавляй в пакет `.env`, ключи, `scripts/` и `build.ps1`.

## 5. Частые ошибки

| Симптом | Причина |
|---|---|
| `usage: ... --input --output` | конвертер объявлен с `io: "json"` вместо файлового режима |
| `package tool ... must not use {input}/{output}` | `io: json` и файловые плейсхолдеры вместе |
| MCP не виден в приложении | нет записи инструмента в `tools[]` (нужны `tool`/`stream` без `bin`) или нет сервера в `connections` |
| Инструмент не появился | нет `bin`, `tool` продублирован или пакет не собран |
| `unknown action` | модель передала подкоманду вне `enum` |
| Пустой ответ | адаптер пишет в `stderr` или не печатает JSON в `stdout` |
| Секрет пустой | имя в `secrets` не совпадает с именем в хранилище DPAPI |
