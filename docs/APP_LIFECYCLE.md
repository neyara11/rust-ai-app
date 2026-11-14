# Жизненный цикл мини-приложений

Документ описывает, как разрабатывать, тестировать, подписывать и тиражировать
пакеты-мини-приложения ECO AIOps, и как разграничить эти роли. Разделы помечены
**сейчас** (работает в коде) и **целевое** (нужно доделать).

## 1. Роли

| Роль | Что делает | Ключи |
|---|---|---|
| Разработчик | пишет пакет (`manifest.json`, prompt, skills, инструменты), тестирует у себя | свой dev-ключ Ed25519 |
| ИБ / подписант | ревью пакета, пересчёт хешей, подпись prod-ключом, ведение allowlist | prod-ключ в HSM/офлайн |
| Администратор | ставит платформу, раскатывает trust anchor и allowlist, публикует пакеты | ключей подписи нет |
| Пользователь | устанавливает и запускает одобренные пакеты | нет |

Правило: **приватный prod-ключ не покидает ИБ/HSM**. Разработчик не может
подписать пакет, который примет прод-платформа.

## 2. Контуры

| Контур | Кто работает | Ключ подписи | Где доверенный ключ (anchor) |
|---|---|---|---|
| dev | разработчик | личный dev-ключ | только в dev-профиле пользователя |
| test/staging | разработчик + ИБ | тестовый ключ ИБ | на тестовых машинах (GPO/вручную) |
| prod | ИБ + администратор | prod-ключ ИБ (HSM) | Machine-scope на всех машинах |

Тестовый и прод-ключи ИБ — разные. Тестовый ключ можно использовать для приёмки,
но он не должен быть доверенным на прод-машинах.

## 3. Анатомия пакета

```
dev-apps/<app_id>/
  manifest.json          # подписывается целиком
  manifest.sig           # Ed25519-подпись байтов manifest.json (hex)
  prompt/system.md       # системный промпт агента
  skills/<skill>/SKILL.md
  bin/<tool>.exe         # если манифест объявляет tools[]
```

`manifest.json` (`schema_version: 2`) задаёт:

- `app_id`, `name`, `version`, `min_shell_version`, `publisher`;
- `profile.system_prompt`, `profile.model_hint`, `profile.max_steps`;
- `tools[]` — исполняемые инструменты внутри пакета (`bin`, `stream`);
- `connections[]` — какие внешние подключения нужны;
- `capabilities` — `file_scopes`, `external_writes`, `index_allowlist`;
- `hashes` — `sha256:` для каждого файла содержимого.

Что проверяет платформа: подпись `manifest.json`, соответствие хешей файлов,
`schema_version`, `min_shell_version`, запрет downgrade. Любая несостыковка —
отказ (fail-closed). Пакет не может расширить свои права: только манифест,
подписанный ИБ.

## 4. Разработка

### 4.0. Два разных разработчика

- **Разработчик платформы** собирает саму платформу, движок и утилиты. Ему нужны
  Rust/MSVC, Node (opencode в `tools/oc`), Python/PyInstaller для конвертера,
  доступ к crates.io/npm и большой диск. Это отдельная роль и отдельное рабочее
  место.
- **Автор мини-приложения** пишет `manifest.json`, prompt и skills, упаковывает и
  тестирует пакет. Ему **не нужны** Rust, Node, Python, интернет и большой диск.

Дальше в этом разделе речь только про автора мини-приложения; сборка платформы
описана в §8.

### 4.1. Рабочее место автора мини-приложения

- Windows 10/11 x64, текстовый редактор.
- Установленная платформа ECO AIOps (WebView2 ставится её установщиком) — для
  локального теста.
- Портативная утилита `eco-pack.exe` (без внешних зависимостей), версия которой
  совместима с установленной платформой.
- Опционально git-клиент, если пакеты ведутся в репозитории.

Не требуются: Rust, MSVC, Node/npm, Python, доступ в интернет, десятки гигабайт
диска. Репозиторий платформы для разработки пакета не нужен.

### 4.2. Утилита `eco-pack` (требование к инструменту)

Один статический exe, работает офлайн, шаблоны и проверки внутри:

```text
eco-pack init <dir> --app-id <id> [--name <name>]   # каркас пакета
eco-pack validate <dir>                             # схема, политика, пути, размеры
eco-pack build <dir> [-o <app.zip>]                 # пересчёт hashes + zip (без ключей)
eco-pack keygen --out <dir>                         # личный dev-ключ
eco-pack sign <dir> --key <dev.key>                 # dev-подпись для локального теста
eco-pack verify <dir> --pub <trusted.pub>           # проверка подписи и хешей
eco-pack trust --profile <path> --pub <trusted.pub> # dev-anchor в профиль
eco-pack dev-install <dir> --key <dev.key> --pub <trusted.pub> --profile <path>
                                                    # build + sign + install в dev-профиль
```

Обязательные свойства:

- не компилирует платформу и не собирает exe-инструменты — только валидирует,
  хеширует, упаковывает, подписывает dev-ключом;
- не содержит и не запрашивает prod-ключи; prod-подпись делает только ИБ;
- офлайн: без обращений к crates.io/npm/registry и без установки зависимостей;
- шаблоны: `minimal` (prompt + один skill) и `files` (то же с `file_scopes:
  ["workspace"]`);
- `validate` проверяет: `schema_version`, `app_id`, semver, `min_shell_version`,
  отсутствие абсолютных путей и `..`, наличие всех файлов из `hashes`/`tools[].bin`,
  разумные лимиты, эвристики на секреты;
- `build` пишет `hashes` по фактическим файлам (не доверяет старым значениям);
- `dev-install` использует установленную платформу и отдельный dev-профиль
  (`ECO_PLATFORM_DEV=1`, dev-anchor); Machine-scope anchor prod-машины имеет
  приоритет и блокирует такой запуск;
- версия утилиты печатается в `eco-pack --version` и совместима с платформой;
- распространяется вместе с платформой из доверенного хранилища и подписана
  Authenticode ИБ.

**Сейчас:** `eco-pack` реализован в `tools/pack` и собирается командой платформы
(`powershell -File tools\build-pack.ps1` → `vendor/pack/eco-pack.exe`, подпись
Authenticode dev-сертификатом). Автору мини-приложения не нужны Rust/Node/Python и
репозиторий платформы; `tools\package.ps1` + `tools\signer` остаются инструментами
команды платформы для сборки/подписи prod-пакетов.

### 4.3. Правила пакета

- Запрашивать минимальные `capabilities`: файлы — только `file_scopes: ["workspace"]`,
  внешние записи — только с обоснованием.
- Никаких абсолютных путей и выхода за рабочую папку: платформа отклоняет `..`,
  абсолютные пути, symlink/junction.
- Секреты не кладутся в пакет: только в DPAPI-хранилище профиля.
- `version` — semver `x.y.z`; обновление контента той же версии допустимо,
  понижение версии при обычной установке запрещено.
- Конвертер/бинарь принадлежит пакету (`bin/`), а не платформе.

### 4.4. Пакеты с exe-инструментами

Это отдельный трек, не типовой сценарий автора мини-приложения:

- exe собирается в контролируемом окружении/CI с вендоренными зависимостями
  (wheelhouse для PyInstaller, vendor для Rust/Go), фиксируется коммит;
- в заявку на подпись прикладывается SBOM/список зависимостей;
- готовый бинарь передаётся автору пакета либо собирается им в том же окружении;
- `eco-pack` такой бинарь не компилирует — только упаковывает, хеширует и
  подписывает dev-ключом для теста.

### 4.5. Локальный цикл

```powershell
eco-pack init D:\apps\my-app --app-id my-app
# правим prompt/system.md, skills/**, manifest.json
eco-pack keygen --out D:\keys\me            # один раз
eco-pack dev-install D:\apps\my-app --key D:\keys\me\dev.key `
  --pub D:\keys\me\trusted.pub --profile D:\work\eco-dev-me
```

`dev-install` сам пересчитывает `hashes`, подписывает dev-ключом, кладёт anchor в
профиль и ставит пакет. `eco-pack validate` полезен до/после правок: до первого
`build`/`dev-install` он ругается на непокрытые `hashes` — это ожидаемо.

Далее — тесты в GUI и `--chat-async`; после правок повторяем `validate` и
`dev-install`. Перед передачей ИБ: `eco-pack build` и отправка архива **без**
prod-подписи.

## 5. Тестирование

### 5.1. Обязательный минимум

```powershell
# подпись/хеши/схема
src-tauri\target\debug\eco-platform.exe --verify dev-apps\<app_id>

# список и версия
src-tauri\target\debug\eco-platform.exe --list

# прогон агента без GUI (поднимает движок, стримит события)
src-tauri\target\debug\eco-platform.exe --chat-async <app_id> "проверочный сценарий"

# сравнение с установленной версией
src-tauri\target\debug\eco-platform.exe --check dev-apps\<app_id>
```

`ECO_PLATFORM_HOME` должен указывать на dev-профиль.

### 5.2. Негативные проверки

- Изменить файл в установленном пакете → установка/запуск отказывает
  (`hash mismatch`).
- Изменить `manifest.json` без переподписи → `signature mismatch`.
- Поставить версию ниже установленной → `downgrade rejected`.
- `read_file("../secret")` → `path must stay inside the workspace`.
- Убрать `bin/<tool>.exe` → инструмента нет в списке или вызов возвращает ошибку.
- Мёртвый внешний endpoint → внятная ошибка, стрим завершается.

### 5.3. UI и офлайн

- UI-прогоны: Playwright с моками Tauri (частичный текст, замена, финальный update).
- E2E `doc-writer`: агент создаёт `.md`, конвертирует в `.docx`, файлы появляются в
  рабочей папке.
- Офлайн: запуск с мёртвым прокси и `vendor/engine` — чат должен работать,
  внешних соединений и npm-установок быть не должно.

### 5.4. Чек-лист перед передачей ИБ

- [ ] `--verify` возвращает `ok: true`.
- [ ] Пройдены негативные проверки из 5.2.
- [ ] `version` поднята, `min_shell_version` не выше текущей платформы.
- [ ] `capabilities` минимальны, каждое расширение обосновано.
- [ ] prompt/skills вычитаны, без секретов и запрещённых инструкций.
- [ ] Для exe: исходники, тег/коммит, SBOM, способ сборки.
- [ ] Проверен сценарий отката (для обновления существующего приложения).

## 6. Подпись ИБ

### 6.1. Что подаётся

- архив пакета (`.zip` или папка) **без** `manifest.sig` или с dev-подписью;
- заявка: `app_id`, `version`, коммит, SBOM, описание изменений, какие
  `capabilities` добавлены, кто проверил.

### 6.2. Что проверяет ИБ

1. Состав пакета: нет лишних/необъявленных файлов, все `tools[].bin` на месте.
2. Манифест: `schema_version`, `app_id`, semver, `min_shell_version`, `publisher`,
   `capabilities`, `external_writes`, `connections`.
3. **Пересчёт хешей по фактическим файлам** — нельзя доверять присланным `hashes`.
4. Ревью prompt/skills (это исполняемое поведение агента).
5. Для exe — происхождение: исходники/CI/тег, SBOM; при необходимости —
   дополнительная Authenticode-подпись самого exe.
6. Политика версий: обычный downgrade запрещён; откат — отдельное решение.

### 6.3. Процедура (сейчас)

```powershell
# 1. Распаковать в чистый каталог на станции подписи
# 2. Пересчитать хеши, подписать prod-ключом, проверить
powershell -File tools\package.ps1 `
  -PackageDir C:\sign\<app_id>-<version> `
  -KeyPath C:\keys\prod.key `
  -TrustedPath C:\keys\prod.pub `
  -SkipConverter -SkipBuild -NoInstall
# 3. Убедиться, что вывод заканчивается verify: ok
```

Для шага `--verify` на станции подписи должен быть установлен prod-anchor:
`eco-platform.exe --trust C:\keys\prod.pub` (или передайте `-DataHome` с профилем,
где он уже установлен).

Если хеши уже верны и менять манифест не нужно, достаточно:

```powershell
tools\signer\target\debug\signer.exe sign manifest.json --key C:\keys\prod.key
src-tauri\target\debug\eco-platform.exe --verify <package-dir>
```

**Разделение шагов:** `eco-pack build <dir>` пересчитывает `hashes` без ключей,
затем `eco-pack sign <dir> --key prod.key` подписывает готовый манифест. ИБ может
использовать эту пару вместо `package.ps1`; хеши при этом пересчитываются по
фактическим файлам. `eco-pack verify <dir> --pub prod.pub` проверяет результат.

### 6.4. Ключи

- Подпись пакета — Ed25519, ключ в HSM/офлайн-хранилище; `trusted.pub` —
  публичный anchor.
- Приватный ключ никогда не попадает в репозиторий, CI-логи, dev-машины.
- Ротация: в платформе предусмотреть 1–2 активных anchor и `key_id`, чтобы смена
  ключа не требовала переустановки всего парка.
- Authenticode (подпись exe/MSI/NSIS) — отдельный сертификат, см. §8.

### 6.5. Аудит подписи

Для каждого подписанного пакета фиксировать: `app_id`, `version`, sha256
`manifest.json`, отпечаток ключа, коммит, кто и когда подписал, решение/заявку.
Хранить подписанный архив и запись в журнале ИБ.

## 7. Тиражирование

1. Подписанный архив и `manifest.sig` публикуются в доверенном хранилище
   (внутренний файловый сервер/репозиторий), публикуется sha256 архива.
2. На машинах один раз ставится платформа (NSIS per-machine, WebView2 offline).
3. Trust anchor ИБ раскатывается **в Machine-scope** (HKLM
   `SOFTWARE\EcoAIOps\TrustedPub`, GPO/установщик), не в пользовательский профиль.
   Пользовательский anchor действует только в dev-режиме (`debug`-сборка или
   `ECO_PLATFORM_DEV=1`); release-сборка его игнорирует.
4. Пользователь ставит пакет из хранилища; платформа проверяет подпись и хеши.
5. Обновление: новая подписанная версия; платформа архивирует предыдущую,
   `--rollback` — только явным действием администратора (пишется в аудит).
6. Отзыв: убрать `app_id`/версию из allowlist или выпустить исправленную версию.
   **Целевое:** подписанный отзыв (CRL) с TTL, чтобы отзыв доезжал без обновления
   пакета.

## 8. Платформа и поставка

- `tools/vendor.ps1` → `vendor/engine` (opencode + models.json), попадает в
  `engine/` рядом с `eco-platform.exe`.
- `bundle.active: true`, NSIS `perMachine`, WebView2 `offlineInstaller`.
- Подпись установщика: `tools/build-installer.ps1`. Для dev — self-signed
  (`-Trust`), для поставки — сертификат УЦ/ИБ (`-Thumbprint`), для HSM —
  `signCommand`.
- Офлайн-сборка: кеши Tauri (`%LOCALAPPDATA%\tauri\{NSIS,WixTools314,x64}`) или
  `fixedRuntime` WebView2.

## 9. Как не дать разрабатывать/подписывать не-разработчикам

Технически нельзя запретить человеку создать файлы пакета — можно запретить
**подпись и запуск**. Поэтому контроль двухслойный.

### 9.1. Организационно

- Доступ к репозиторию и CI — по AD-группам; branch protection, ревью.
- Заявка на подпись — через ИБ; prod-ключ использует только уполномоченный
  сотрудник (4 глаза для критичных приложений).
- Тестовый контур отделён от прода; тестовый ключ не доверен на прод-машинах.
- Список разработчиков и права на сборку ведутся ИБ/владельцем платформы.

### 9.2. Технически

- Prod-ключ только в HSM/офлайн; в дистрибутив и на машины пользователей
  `signer` и приватные ключи не попадают.
- Release-сборка доверяет **только Machine-scope anchor** (`HKLM\SOFTWARE\EcoAIOps\
  TrustedPub`) и отклоняет `--trust` без elevation. Пользовательский anchor
  (`<home>\trusted.pub`) используется только в dev-режиме. Проверка:
  `eco-platform --anchor` показывает `source` и `fingerprint`.
- dev-режим — `debug`-сборка или явный `ECO_PLATFORM_DEV=1`; в UI источник anchor
  виден (`machine`/`dev`), dev-anchor не действует в prod.
- Опционально allowlist: подписанный ИБ файл в `%ProgramData%\EcoAIOps\`;
  платформа проверяет его подпись и разрешает только перечисленные `app_id`/
  версии. Обновление allowlist не требует обновления платформы.
- ACL: `Program Files`, `%ProgramData%\EcoAIOps` — запись только администраторы.
- Аудит установок/отказов/откатов; отпечаток anchor виден в UI.

### 9.3. Кто что может (целевое)

| Действие | Разработчик | ИБ | Админ | Пользователь |
|---|---|---|---|---|
| писать пакет | да | — | — | нет доступа к репо |
| тестовая установка (dev-ключ) | да | да | — | нет |
| подпись prod | нет | да | нет | нет |
| установка подписанного пакета | да | да | да | да, если разрешён allowlist |
| allowlist / anchor | нет | подписывает | раскатывает | нет |
| откат версии | нет | решает | выполняет | нет |

## 10. Текущее состояние и пробелы

Работает:

- подпись/проверка Ed25519, хеши, fail-closed, проверка при каждом запуске;
- `eco-pack` (`init/validate/build/keygen/sign/verify/trust/dev-install`) — офлайн,
  без Rust/Node/Python на машине автора, с Authenticode-подписью dev-сертификатом;
- Machine-scope anchor в `HKLM\SOFTWARE\EcoAIOps\TrustedPub`; release-сборка
  игнорирует пользовательский anchor, `--trust` без прав администратора отклоняется,
  `--anchor` показывает источник и отпечаток;
- `tools/package.ps1` (hashes → sign → verify → install) с параметрами ключей;
- dev-ключи, отдельные профили, `--verify`, `--list`, `--check`, `--history`,
  `--rollback`, `--chat-async`;
- аудит установок и отказов по подписи в `audit/<app>.jsonl`;
- вендоринг движка и офлайн-запуск;
- подпись установщиков self-signed сертификатом (dev).

Не хватает для целевой модели:

- allowlist и отзыв (CRL) как подписанные файлы;
- провенанс/CI/SBOM и подпись артефакта из CI;
- ротация ключей и несколько активных anchor;
- production-сертификат Authenticode (сейчас dev self-signed).

## 11. Шпаргалка

Разработчик мини-приложения:

```powershell
eco-pack init D:\apps\<app_id> --app-id <app_id>
eco-pack keygen --out D:\keys\<user>
eco-pack dev-install D:\apps\<app_id> --key D:\keys\<user>\dev.key `
  --pub D:\keys\<user>\trusted.pub --profile D:\work\eco-dev-<user>
```

Разработчик платформы (текущий способ в репозитории):

```powershell
powershell -File tools\package.ps1 `
  -App <app_id> `
  -KeyPath dev\keys\<user>\dev.key `
  -TrustedPath dev\keys\<user>\trusted.pub `
  -DataHome D:\work\eco-dev-<user>
```

ИБ:

```powershell
powershell -File tools\package.ps1 -PackageDir C:\sign\<app> `
  -KeyPath C:\keys\prod.key -TrustedPath C:\keys\prod.pub `
  -SkipConverter -SkipBuild -NoInstall
```

Администратор:

```powershell
# установка платформы (per-machine) и anchor — через установщик/GPO
# anchor: HKLM\SOFTWARE\EcoAIOps\TrustedPub (или `--trust` из-под администратора)
eco-platform.exe --trust C:\keys\prod.pub   # elevated
eco-platform.exe --anchor                   # source=machine, fingerprint
# пакеты — из доверенного хранилища; проверка:
eco-platform.exe --verify <package-dir>
eco-platform.exe --install <package-dir>
eco-platform.exe --check <package-dir>
eco-platform.exe --history <app_id>
eco-platform.exe --rollback <app_id> [version]
```
