# Тестирование в закрытом контуре без готовых ключей

Ключи в комплект не входят специально. Для проверки можно создать **временный
dev-ключ прямо на тестовой машине** (`eco-pack` работает офлайн) и включить
dev-режим платформы. Prod-ключи и сертификаты для этого не нужны.

## Вариант A. Временный dev-ключ (рекомендуется)

1. Сгенерируйте ключ на целевой машине:

```powershell
mkdir D:\eco-test\keys
eco-pack\eco-pack.exe keygen --out D:\eco-test\keys
# D:\eco-test\keys\dev.key       — приватный, никому не передавать
# D:\eco-test\keys\trusted.pub   — публичный
```

2. Соберите и подпишите пакет этим ключом:

```powershell
eco-pack\eco-pack.exe build D:\eco-test\app
eco-pack\eco-pack.exe sign D:\eco-test\app --key D:\eco-test\keys\dev.key
```

Если пакет уже подписан другим dev-ключом (например, `examples\skill-only`),
достаточно переподписать его этим ключом — манифест и hashes не меняются:

```powershell
eco-pack\eco-pack.exe sign D:\eco-test\skill-only --key D:\eco-test\keys\dev.key
```

3. Включите dev-доверие только для теста:

```cmd
set ECO_PLATFORM_DEV=1
start "" "C:\Program Files\ECO AIOps\eco-platform.exe"
```

и положите публичный ключ в профиль платформы:

```powershell
# профиль по умолчанию (release)
copy D:\eco-test\keys\trusted.pub "$env:LOCALAPPDATA\EcoAIOps\trusted.pub"
```

`trusted.pub` — публичный файл, его можно передавать. `dev.key` — нет.

4. В платформе пакет должен показывать «подпись ок», источник anchor `dev`.
   Установите пакет и откройте приложение.

5. Изолированный профиль (не трогает рабочие данные): `dev-install` сам
   пересчитывает hashes, подписывает, кладёт anchor и ставит пакет:

```powershell
eco-pack\eco-pack.exe dev-install D:\eco-test\app `
  --key D:\eco-test\keys\dev.key `
  --pub D:\eco-test\keys\trusted.pub `
  --profile D:\eco-test\profile `
  --platform "C:\Program Files\ECO AIOps\eco-platform.exe"

set ECO_PLATFORM_HOME=D:\eco-test\profile
set ECO_PLATFORM_DEV=1
start "" "C:\Program Files\ECO AIOps\eco-platform.exe"
```

6. После теста уберите dev-доверие:

```powershell
del "$env:LOCALAPPDATA\EcoAIOps\trusted.pub"
set ECO_PLATFORM_DEV=
```

## Вариант B. Машинный anchor (ближе к prod)

Если есть права администратора, anchor можно поставить в HKLM — тогда
`ECO_PLATFORM_DEV=1` не нужен:

```powershell
# из-под администратора
& "C:\Program Files\ECO AIOps\eco-platform.exe" --trust D:\eco-test\keys\trusted.pub
& "C:\Program Files\ECO AIOps\eco-platform.exe" --anchor
```

Для удаления:

```powershell
& "C:\Program Files\ECO AIOps\eco-platform.exe" --untrust <fingerprint>
```

В prod этот ключ заменяется ключом ИБ; временный dev-anchor нужно снять.

## LLM в закрытом контуре

- Укажите внутренний OpenAI-совместимый gateway в «Настройки → LLM Gateway»,
  модель и ключ (DPAPI). Это единственный обязательный сетевой ресурс.
- Если LLM пока нет, можно проверить установку, подпись/hashes, каталог, мастер
  пакета, настройки и MCP-реестр; чат работать не будет.
- Внешние MCP-серверы настраиваются в «Реестр MCP-серверов»; без них remote-tools
  не появятся.

## Allowlist

Отсутствие `%ProgramData%\EcoAIOps\allowlist.json` не ограничивает установку —
проверяется только подпись пакета. Если allowlist в контуре уже есть, он
подписывается тем же ключом, что и anchor, иначе платформа откажет (fail-closed).

## Ограничения

- Dev-ключ и `ECO_PLATFORM_DEV=1` — только для теста. Не использовать в prod.
- Установщики подписаны dev self-signed сертификатом; для поставки нужен
  сертификат УЦ/ИБ.
- `examples/skill-only` в комплекте подписан dev-ключом сборки; на целевой машине
  проще переподписать его локальным ключом (см. шаг 2).
