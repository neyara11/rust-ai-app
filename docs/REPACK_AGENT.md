# Переупаковка агента: MCP, скрипты и `.env`

Инструкция для агента-исполнителя. Прочитай её целиком, затем упакуй текущего
агента в отдельную папку по правилам ниже. Ничего не подписывай prod-ключом и не
публикуй.

## 0. Задача

На входе есть папка агента: `AGENTS.md`, несколько `skills/**`, Python-скрипты,
`.env` и конфиги MCP. Нужно получить валидный пакет ECO AIOps:

```text
out/
├─ <app_id>/                 # сам пакет
│  ├─ manifest.json
│  ├─ prompt/system.md
│  ├─ skills/<имя>/SKILL.md
│  └─ bin/<скрипт>.exe       # по одному на исполняемый скрипт
├─ SECRETS_REQUIRED.md       # какие секреты ввести в платформе (имена, без значений)
└─ REPORT.md                 # что сделано, что осталось вручную
```

Исходники не изменяй на месте: работай с копией.

### Как поставить задачу агенту

```text
Прочитай docs/REPACK_AGENT.md. Переупакуй текущего агента из <папка-исходников>
в <out>. Соблюдай жёсткие правила: .env и секреты не копировать, MCP — только id,
скрипты перевести на переменные окружения и объявить tools[].secrets.
В конце заполни SECRETS_REQUIRED.md и REPORT.md и прогони eco-pack validate/build.
```


## 1. Инвентаризация (до любых изменений)

Собери и зафиксируй в `REPORT.md`:

- `AGENTS.md` (или `prompt/system.md`) — есть/нет.
- Скилы: `skills/<имя>/SKILL.md`, а также корневой `SKILL.md`.
- MCP-конфиги: `.mcp.json`, `mcp.json`, `.vscode/mcp.json`, `opencode.json(c)`.
- Скрипты: `*.py`, `*.ps1`, `*.sh`, `*.js`, которые вызывают внешние сервисы.
- `.env` и производные: `.env.*`, `*.pem`, `*.key`, `*.p12`, `*.pfx`,
  `id_rsa*`, `credentials*`.
- Что нужно скриптам: аргументы, файлы, сеть, запись в внешнюю систему.

Если чего-то не хватает (нет `AGENTS.md`, нет скилов, скрипт требует
интерактивного ввода), остановись и перечисли блокеры в `REPORT.md`.

## 2. Жёсткие правила

1. **Никогда не копируй `.env` и секреты в пакет.** Значения логинов, паролей,
   токенов не должны попасть ни в пакет, ни в `REPORT.md`, ни в `SECRETS_REQUIRED.md`.
2. Не копируй `.git`, `node_modules`, `.venv`, `__pycache__`, `target`, `dist`,
   `build`, `sessions`, `engine` и логи.
3. В пакете допускаются только относительные пути; никаких `C:\...` и `..`.
4. Не меняй бизнес-логику скриптов. Разрешено только: заменить чтение `.env` на
   переменные окружения, добавить/уточнить CLI/JSON-интерфейс.
5. Prod-ключ и подпись — только у ИБ. Локально допустим dev-ключ.
6. Все секреты вводятся пользователем в платформе (DPAPI), в пакете остаются
   только **имена** секретов.

## 3. MCP

Для каждого найденного конфига:

1. Извлеки **только id серверов** (ключи в `mcpServers` / `servers` / `mcp`).
2. Определи тип:
   - remote (`url`, `serverUrl`, `type: "http"`/`"remote"`) — поддерживается как
     `http-mcp`;
   - stdio (`command`, `args`) — **не поддерживается** платформой; запиши в
     `REPORT.md` как ручной блокер, в манифест не добавляй.
3. URL, заголовки и токены не копируй в пакет. В `SECRETS_REQUIRED.md` для каждого
   remote-сервера укажи: id, что нужно ввести (endpoint URL, Bearer-токен), где:
   «Настройки → Реестр MCP-серверов».
4. В манифест добавь подключения:

```json
"connections": [
  { "id": "llm", "type": "llm", "required": true },
  { "id": "<mcp-id>", "type": "http-mcp", "required": false }
]
```

5. Права записи для MCP по умолчанию выключены. Включай id в
   `capabilities.external_writes` только если пользователь/владелец агента явно
   подтвердил, что инструменты этого сервера должны менять данные.
6. Id в манифесте должен совпадать с id в настройках платформы.

## 4. Скрипты и `.env`

Для каждого скрипта, который ходит во внешнюю систему:

1. Определи, какие переменные он читает из `.env` (имена, не значения).
2. Перепиши чтение на окружение, например:

```python
import os
login = os.environ["CRM_LOGIN"]
password = os.environ["CRM_PASSWORD"]
base_url = os.environ.get("CRM_URL", "https://crm.internal/api")
```

3. Убери `python-dotenv`, `load_dotenv()` и путь к `.env`. Скрипт не должен сам
   искать `.env`.
4. Собери exe (если есть Python и PyInstaller):

```powershell
pyinstaller --onefile --clean --name crm_sync crm_sync.py
# готовый файл: dist\crm_sync.exe -> <app_id>\bin\crm_sync.exe
```

Если PyInstaller недоступен, оставь скрипт и запиши в `REPORT.md`: «bin не
собран, пакет не пройдёт validate». Не подменяй exe заглушкой.

5. Определи интерфейс инструмента:
   - `io: "json"` — аргументы модели приходят в **stdin** одним JSON-объектом,
     результат печатается в **stdout** (JSON или текст). Для сервисных скриптов
     используй этот режим.
   - `io: "files"` (или не указывать) — файловый режим `--input/--output` в
     рабочей папке (только для конвертеров).
6. Для `io: json` опиши `parameters` — JSON Schema аргументов. Если не можешь
   вывести её из кода, используй свободную схему и отметь в `REPORT.md`:

```json
{ "type": "object", "additionalProperties": true }
```

7. Сопоставь переменные с именами секретов DPAPI в `secrets`. Правила имён:
   env — `A-Z`, `0-9`, `_`; секрет — латиница, цифры, `-`, `_`, `.`.
   Рекомендуемая схема: `<сервис>_<поле>` (`crm_login`, `crm_password`).

Пример инструмента:

```json
{
  "tool": "crm_sync",
  "bin": "bin/crm_sync.exe",
  "io": "json",
  "parameters": {
    "type": "object",
    "properties": { "query": { "type": "string" } },
    "required": ["query"]
  },
  "secrets": {
    "CRM_URL": "crm_url",
    "CRM_LOGIN": "crm_login",
    "CRM_PASSWORD": "crm_password"
  },
  "stream": "write"
}
```

`stream`: `read`, если инструмент только читает; `write`, если меняет данные во
внешней системе или в рабочей папке.

## 5. Скилы

1. `AGENTS.md` → `prompt/system.md` (перенос/копирование).
2. Корневой `SKILL.md` → `skills/main/SKILL.md`.
3. Остальные `skills/<имя>/SKILL.md` оставь как есть.
4. В тексте скилов:
   - убери упоминания `.env`, паролей и токенов;
   - замени «запусти скрипт X» на «вызови инструмент `<tool>` с аргументами …»;
   - укажи MCP-инструменты как `<connection>_<tool>`, если они известны.

## 6. Манифест

Создай `<app_id>/manifest.json` со схемой 2. Шаблон, который нужно заполнить:

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
    "max_steps": 20
  },
  "tools": [],
  "connections": [
    { "id": "llm", "type": "llm", "required": true }
  ],
  "capabilities": {
    "index_allowlist": [],
    "file_scopes": ["workspace"],
    "external_writes": []
  },
  "artifacts": { "quota_mb": 50, "ttl_hours": 72 },
  "hashes": {}
}
```

Правила:

- `app_id`: латиница, цифры, `-`, `_`, не более 64 символов.
- `file_scopes: ["workspace"]` — только если агенту нужны файлы рабочей папки.
- `external_writes` — только для MCP, которые меняют внешние данные.
- `tools` — по одному на каждый exe-инструмент; секреты — в `secrets`.
- `hashes` оставь пустым: их посчитает `eco-pack build`.

## 7. Сборка и проверка

```powershell
eco-pack validate .\out\<app_id>
eco-pack build .\out\<app_id>
eco-pack validate .\out\<app_id>
```

- До `build` validate ожидаемо ругается на `hashes`.
- `build` должен пройти без ошибок; zip появится рядом с пакетом.
- Если validate ругается на `.env`, ключи, абсолютные пути, `bin` или `secrets` —
  исправь, а не игнорируй.
- Для локального теста: `eco-pack sign .\out\<app_id> --key <dev.key>`.
- Передай в ИБ: папку пакета без dev-подписи и `SECRETS_REQUIRED.md`.

## 8. `SECRETS_REQUIRED.md`

Таблица **без значений**:

```markdown
# Что ввести в платформе

## Секреты (Настройки → Хранилище секретов DPAPI)

| Имя секрета | Для чего | Env-переменная |
|---|---|---|
| crm_url | Адрес API CRM | CRM_URL |
| crm_login | Логин сервисной УЗ | CRM_LOGIN |
| crm_password | Пароль сервисной УЗ | CRM_PASSWORD |

## MCP (Настройки → Реестр MCP-серверов)

| id | Ввести |
|---|---|
| crm | endpoint URL, Bearer-токен |
```

Не вставляй в этот файл реальные URL с токенами и значения секретов.

## 9. `REPORT.md`

Кратко:

- что найдено (скилы, MCP, скрипты, `.env`);
- какие скрипты переведены на env и собранные exe;
- какие MCP объявлены, какие stdio пропущены;
- какие секреты нужно завести;
- что не удалось автоматизировать;
- результат `validate/build` (команды и итог).

## 10. Чек-лист приёмки

- [ ] В пакете нет `.env`, `.pem`, `.key`, `.p12`, `.pfx`, `id_rsa*`, `credentials*`.
- [ ] Все exe лежат в `bin/` и объявлены в `tools` с `secrets`.
- [ ] Скрипты читают только `os.environ`, без `dotenv` и зашитых логинов.
- [ ] MCP: объявлены только remote id; URL/токены — в настройках.
- [ ] `capabilities` минимальны: `file_scopes`/`external_writes` только по делу.
- [ ] `eco-pack build` и `validate` проходят без ошибок.
- [ ] `SECRETS_REQUIRED.md` и `REPORT.md` заполнены, значений секретов нет.
- [ ] Prod-подпись не выполнялась; пакет готов к проверке ИБ.
