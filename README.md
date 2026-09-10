# BKPI

BKPI — локальный runtime для анализа KPI через Codex, Claude Code и другие coding agents.

**KPI + план + данные Bitrix → оценка агента → точный расчёт в Rust.**

BKPI не зашивает в код конкретные KPI, грейды или размеры премий. Агент интерпретирует ваши документы и evidence, а runtime отвечает за получение данных, локальное состояние и детерминированный расчёт.

## Быстрый старт

Требуется Node.js 22+. Rust конечному пользователю не нужен.

```sh
npx bkpi
```

Installer:

- скачает подходящий native runtime из GitHub Release;
- проверит SHA256;
- поможет подключить Bitrix24;
- настроит Codex и/или Claude Code;
- установит общий BKPI skill.

Поддерживаемые платформы:

- macOS Apple Silicon;
- macOS Intel;
- Linux x64;
- Linux ARM64;
- Windows x64.

Полезные команды installer:

```sh
npx bkpi doctor
npx bkpi targets
npx bkpi@latest update
npx bkpi uninstall
```

## Создание workspace

Создайте каталог для KPI и запустите инициализацию:

```sh
mkdir -p ~/work/kpi
cd ~/work/kpi
bkpi init
```

BKPI предложит выбрать integration, сотрудника и KPI-документ.

Для одного человека workspace выглядит примерно так:

```text
project/
  KPI.md
  .bkpi/
    workspace.toml
    people/
      <person-id>/
        state.toml
```

После этого откройте новую сессию Codex или Claude Code в каталоге workspace и спросите, например:

> Посмотри, как у меня идёт KPI за сентябрь.

Агент сам использует BKPI tools, данные Bitrix и доступные ему документы.

## Несколько сотрудников

В `bkpi init` можно выбрать нескольких сотрудников. Добавлять и удалять людей можно и позже:

```sh
bkpi person add
bkpi person list
bkpi person remove <person-id>
bkpi workspace show
```

Примеры запросов агенту:

- «Разбери подробно KPI Ивана»;
- «Как у всей команды?»;
- «По каким KPI не хватает evidence?»;
- «Какие цели сейчас под риском?».

## Интеграции Bitrix24

```sh
bkpi integration add
bkpi integration list
bkpi integration doctor
bkpi integration users <integration-id> --search Иван
bkpi integration remove <integration-id>
```

BKPI хранит в workspace только ID integration и Bitrix user ID. Credential хранится отдельно от workspace.

По умолчанию используется локальный file backend. Также доступны keychain и env backend:

```sh
bkpi integration credential show <integration-id>
bkpi integration credential move <integration-id> --to file
bkpi integration credential move <integration-id> --to keychain
```

Webhook не передаётся через argv и не сохраняется в workspace.

## KPI-документы

Для KPI можно использовать локальный Markdown/plain-text файл или передать документ агенту непосредственно в текущем чате.

PDF и DOCX интерпретирует сам агент; Rust runtime не пытается самостоятельно разбирать такие документы.

При оценке BKPI различает подтверждённые факты, вероятные соответствия, неизвестные данные и отрицательные evidence. Название задачи само по себе не считается доказательством выполнения KPI.

## План и scoring

К workspace можно привязать план на период и scoring-конфигурацию.

Пример plan reference:

```toml
[[plans]]
id = "2026-09"
label = "План разработки — сентябрь 2026"
start_date = "2026-09-01"
end_date = "2026-09-30"
source = "google_drive"
reference = "https://docs.google.com/document/d/DOCUMENT_ID/edit"
scope = "workspace"
```

BKPI хранит ссылку, а содержимое документа читает connector, доступный агенту.

Команды:

```sh
bkpi plan list
bkpi plan show 2026-09
bkpi scoring show
bkpi calculate --input-json calculation.json
```

Денежный расчёт выполняется Rust runtime и не зависит от свободной интерпретации модели.

Подробная схема: [Plan and scoring](specs/plan-and-scoring.md).

Примеры конфигураций:

- [scoring-senior.toml](examples/scoring-senior.toml)
- [calculation-senior.json](examples/calculation-senior.json)

Это примеры, а не встроенные defaults.

## CLI и MCP

BKPI можно использовать напрямую из shell:

```sh
bkpi tasks --person eduard --month 2026-09 --json
bkpi tasks --person eduard --open
bkpi tasks --person eduard --late
bkpi agent snapshot --person eduard --month 2026-09 --json
bkpi agent team-snapshot --person-ids eduard,ivan --json
```

Для coding agents runtime предоставляет MCP stdio server:

```sh
bkpi mcp
```

Можно жёстко ограничить MCP одним workspace:

```sh
bkpi --workspace /absolute/path/to/project mcp
```

Основные MCP tools покрывают workspace, people, Bitrix tasks, KPI documents, evidence, snapshots, plans и расчёт KPI.

## Codex и Claude Code

Targets можно настроить в installer:

```sh
npx bkpi targets
```

Installer использует официальные CLI для регистрации MCP и устанавливает общий BKPI skill.

После изменения target или обновления BKPI откройте новую сессию агента.

## Безопасность

Credentials хранятся вне workspace и не должны попадать в Git.

На Unix BKPI использует приватные права доступа для credential directories/files и умеет их проверять:

```sh
bkpi config fix-permissions
```

BKPI редактирует локальное состояние атомарно и не считает отсутствующие данные автоматически выполненным или проваленным KPI.

## Разработка

Проверка Rust runtime:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Проверка installer:

```sh
cd installer
npm ci
npm run typecheck
npm run lint
npm run build
npm test
npm pack --dry-run
```

Release workflow собирает native binaries для всех поддерживаемых платформ и публикует их с SHA256 checksums.

## License

MIT
