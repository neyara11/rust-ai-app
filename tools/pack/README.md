# eco-pack

Офлайн-утилита для авторов мини-приложений ECO AIOps. Один exe, без Rust/Node/Python
и без сети на машине автора. Собирается командой платформы.

## Сборка

```powershell
cargo test --manifest-path tools\pack\Cargo.toml
powershell -File tools\build-pack.ps1     # -> vendor\pack\eco-pack.exe (+ dev Authenticode)
```

## Команды

```text
eco-pack init <dir> --app-id <id> [--name <name>] [--template minimal|files]
eco-pack validate <dir>
eco-pack build <dir> [-o <package.zip>]
eco-pack keygen --out <dir>
eco-pack sign <dir> --key <dev.key>
eco-pack verify <dir> --pub <trusted.pub>
eco-pack trust --profile <path> --pub <trusted.pub>
eco-pack dev-install <dir> --key <dev.key> --profile <path> [--pub <trusted.pub>] [--platform <exe>]
```

- `build` пересчитывает `hashes` по фактическим файлам и сохраняет остальной текст
  манифеста; `validate` до первого `build` ожидаемо ругается на непокрытые hashes.
- `dev-install` подписывает пакет dev-ключом, кладёт anchor в указанный профиль и
  вызывает установленную платформу с `ECO_PLATFORM_DEV=1`. Machine-scope anchor
  prod-машины имеет приоритет, поэтому там dev-install не сработает.
- `eco-pack` не содержит prod-ключей, не собирает платформу и exe-инструменты.
  Dev-ключ — только для локальных тестов.
