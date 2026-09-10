# BKPI

Локальные инструменты для анализа KPI в Codex, Claude Code и shell agents.

**KPI + согласованный Plan + Bitrix → оценка агента → точный расчёт в Rust.**
BKPI не содержит встроенных критериев, профилей, универсального score или фиксированного фонда премии. Rust runtime собирает данные и рассчитывает выплату по опциональному `scoring.toml`; Codex/Claude интерпретируют документы и evidence. OpenRouter для MCP не нужен.

Планы и scoring опциональны: старые workspaces продолжают качественный анализ. Общий Google team plan хранится как ссылка; документ читает доступный connector агента. Денежный расчёт при настроенном scoring выполняется только через `kpi_calculate` / `bkpi calculate`.

Схемы, команды, правила округления и настройка существующего workspace: [Plan and scoring](specs/plan-and-scoring.md). Готовый пример 170k: [scoring-senior.toml](examples/scoring-senior.toml), [calculation-senior.json](examples/calculation-senior.json). Это примеры, а не runtime defaults.

## Установка

После публикации npm package и release assets:

```sh
npx bkpi
npx bkpi doctor
npx bkpi@latest update
npx bkpi targets
npx bkpi uninstall
```

Сейчас package **подготовлен, но не опубликован**. Данный исходный каталог не содержит Git remote, поэтому GitHub owner не выдуман. Перед публикацией укажите `installer/package.json` → `bkpi.releaseRepository` (`owner/repo`) либо задайте `BKPI_RELEASE_REPOSITORY`. Имя npm package задаётся только в `installer/package.json.name`; доступность имени `bkpi` не гарантируется. Требуется Node.js 22+, Rust конечному пользователю не нужен.

Wizard устанавливает бинарник текущей версии, проверяет SHA256 release asset, позволяет добавить несколько порталов и выбрать Codex/Claude/CLI only. Webhook вводится скрыто в wizard (или в Rust CLI при прямом вызове). Installer передаёт его runtime только через stdin; argv не содержит секретов, вывод child process при ошибке подавляется. Сначала проверяются `user.current` и доступ к задачам, затем сохраняется credential. Для подключения агентов нужны установленные соответствующие CLI.

Runtime устанавливается в `~/.local/share/bkpi/bin` (Windows: `%LOCALAPPDATA%/bkpi/bin`). Добавьте этот каталог в пользовательский PATH. Agent MCP registrations используют абсолютный путь и не зависят от PATH.

Повторная установка позволяет добавить/обновить/удалить integration, выбрать или удалить targets, обновить runtime и запустить миграцию. Uninstall сохраняет credentials и workspace; удаление интеграции выполняется отдельно. `update` ставит версию, соответствующую версии запущенного npm installer, поэтому для новой версии используйте `@latest`. После обновления `targets` обновляет skill.

## Локальный запуск installer и npx flow

```sh
cargo build --release --features standalone-ai
cd installer
npm ci
npm run typecheck
npm run lint
npm run build
npm test
BKPI_RUNTIME_SOURCE="$(pwd)/../target/release/bkpi" node dist/index.js update
node dist/index.js setup
npm pack
npx --yes --package ./bkpi-0.6.0.tgz bkpi --help
npx --yes --package ./bkpi-0.6.0.tgz bkpi setup
```

`BKPI_RUNTIME_SOURCE` — явный development override для готового локального бинарника; он проверяется по версии. Для изолированного smoke задайте `BKPI_INSTALL_DIR=/tmp/bkpi-smoke`, а для runtime config — `BKPI_CONFIG_DIR=/tmp/bkpi-config`. Эти переменные не содержат credentials. Не выбирайте реальные agent targets в изолированном smoke: пути их skills являются обычными пользовательскими путями.

## Codex и Claude Code

```sh
cd installer
node dist/index.js targets
```

Выберите один или оба targets. Installer ставит общий skill через target adapter:

- Codex: `~/.agents/skills/bkpi/SKILL.md`; `codex mcp add bkpi -- /absolute/path/bkpi mcp`.
- Claude: `~/.claude/skills/bkpi/SKILL.md`; `claude mcp add --transport stdio --scope user bkpi -- /absolute/path/bkpi mcp`.

Регистрация выполняется официальными CLI, без ручного переписывания их config. Неуправляемые существующие `bkpi` skills/MCP entries не перезаписываются. Настроенные targets можно удалить через тот же wizard. После подключения откройте новую сессию агента в проекте с workspace.

Адаптеры сверены с [Codex skills](https://learn.chatgpt.com/docs/build-skills), локальным `codex mcp add --help`, [Claude MCP](https://code.claude.com/docs/en/mcp) и [Claude skills](https://code.claude.com/docs/en/skills).

`npm run build` также генерирует альтернативные plugin bundles:
`installer/bundles/codex/bkpi/.codex-plugin/plugin.json` и
`installer/bundles/claude/bkpi/.claude-plugin/plugin.json`, с общим skill и `.mcp.json`.
Это отдельный вариант распространения, не устанавливайте его одновременно с standalone target adapter. Bundle требует `bkpi` в PATH. Codex manifest проверяется штатным plugin validator. Installer по умолчанию использует skill + MCP, без регистрации marketplace.

## Интеграции

```sh
bkpi integration add
bkpi integration add --id another-portal --name 'Другой портал'
bkpi integration list
bkpi integration doctor
bkpi integration doctor another-portal
bkpi integration users another-portal --search Иван
bkpi integration remove another-portal
```

`add` без `--id` генерирует ID из hostname (astrovolga.bitrix24.ru → astrovolga; коллизии → -2, -3). Wizard не спрашивает и не показывает ID. Название опционально. `add` с существующим explicit ID обновляет webhook; installer предлагает отдельное действие обновления с выбором названия портала. Webhook никогда не является аргументом команды. Глобальный config содержит schema version, ID, имя, HTTPS origin портала и ссылку на credential. Person хранит только integration ID и Bitrix user ID.

## Workspace с одним человеком

```sh
mkdir -p ~/work/kpi
cd ~/work/kpi
bkpi init
```

Подтвердите каталог, выберите integration по названию и `Me`. При одном integration отдельный выбор портала не нужен. Укажите существующий KPI-документ либо нажмите Enter. Копирование в workspace предлагается по умолчанию; исходник сохраняется. Можно отказаться и оставить ссылку на исходный файл. Поддерживаются `~`, относительные/абсолютные пути, кавычки Explorer и пути Finder с экранированными пробелами. Runtime читает UTF-8 текст (Markdown/plain text); PDF/DOCX передавайте агенту в чате.

```text
project/
  KPI.md
  .bkpi/
    workspace.toml
    people/
      portal-44843/
        state.toml
```

Без документа создаётся видимый placeholder `KPI.md`. Summary показывает точный абсолютный путь. Пустой локальный файл не мешает анализу KPI, переданных в текущем чате; агент сохраняет их только после явного согласия.

После этого спросите агента: «Посмотри, как у меня идёт KPI».

## Workspace с несколькими людьми

В `bkpi init` выберите `Search employees`, найдите сотрудников и отметьте нужных пробелом. Для каждого можно указать KPI или пропустить шаг. Новые документы создаются в `kpi/<person-slug>.md`. В stdin-режиме выбор IDs через запятую сохранён. Для другого портала или последующего расширения:

```sh
bkpi person add
bkpi person list
bkpi person remove ivan
bkpi workspace show
```

Для известного сотрудника можно задать параметры явно:

```sh
bkpi person add --integration astrovolga --user-id 44843 --id eduard --name 'Эдуард Шубин' --self
bkpi person add --integration another-bitrix --user-id 127 --id ivan --name 'Иван Иванов'
```

Пример конфигурации:

```toml
version = 1

[[people]]
id = "eduard"
name = "Эдуард Шубин"
integration = "astrovolga"
bitrix_user_id = "44843"
self = true
kpi = "kpi/eduard-shubin.md"

[[people]]
id = "ivan"
name = "Иван Иванов"
integration = "another-bitrix"
bitrix_user_id = "127"
kpi = "kpi/ivan-ivanov.md"
```

У каждого свой документ. Self может быть не более одного; без явного person используется единственный человек либо self. Иначе требуется выбор. `person remove` убирает запись из workspace, но оставляет файлы для восстановления. Чтобы вернуть её, восстановите запись в workspace.toml; `person add` не перезаписывает оставшиеся файлы.

Вопросы агенту: «Разбери подробно KPI Ивана», «Как у всей команды?», «У кого не хватает evidence?», «Кто рискует недобрать премию?». Руководитель анализируется столь же подробно, как любой сотрудник. Сравнение разных документов не превращается в универсальный рейтинг.

## Приоритет KPI

1. Документ/текст/PDF/DOCX, явно предоставленный пользователем в текущем разговоре.
2. Документ по ссылке `Person.kpi` выбранного человека.
3. Агент просит KPI и сообщает, что данных для оценки недостаточно.

Runtime не читает чат и не парсит PDF/DOCX; это задача агента. Markdown не требует специальной схемы. Пустой шаблон не считается настроенным KPI. Название задачи само по себе не доказывает достижение. Skill требует confirmed / likely / unknown / negative, конкретные evidence и gaps.

## JSON CLI и MCP

```sh
bkpi tasks --person eduard --month 2026-09 --json
bkpi tasks --person eduard --open
bkpi tasks --person eduard --late
bkpi agent snapshot --person eduard --month 2026-09 --json
bkpi agent team-snapshot --person-ids eduard,ivan --json
bkpi mcp
# Optional: restrict an MCP instance to one workspace
bkpi --workspace /absolute/project mcp
```

JSON stdout содержит только JSON, ошибки и прогресс идут в stderr. Вывод snapshot детерминирован для одинаковых данных; ключи коллекций и порядок людей/задач стабильны.

MCP stdio поддерживает handshake версии `2025-11-25`, ping, tools/list и tools/call. По умолчанию инструмент получает абсолютный `workspace` от агента или ищет `.bkpi` от cwd процесса. Явный `--workspace` ограничивает MCP данным workspace; попытка обратиться к другому отклоняется.

Read tools: `bkpi_workspace_get`, `bkpi_people_list`, `bkpi_person_get`, `bkpi_tasks_list`, `bkpi_task_get`, `bkpi_kpi_document_get`, `bkpi_local_state_get`, `bkpi_evidence_get`, `bkpi_snapshot_get`, `bkpi_team_snapshot_get`.

Local write tools: `bkpi_mapping_set`, `bkpi_mapping_remove`, `bkpi_evidence_set`, `bkpi_evidence_remove`. Они меняют только `people/<id>/state.toml`. Criteria — произвольные ссылки/текст из реального документа. Mapping идентифицируется task_id + criterion + period, evidence — ID. Запись защищена advisory lock и атомарной заменой файла.

Shell fallback для локальных tools:

```sh
bkpi agent call bkpi_mapping_set --args '{"person":"eduard","task_id":"123","criterion":"Ссылка на реальный критерий","period":"2026-09","note":"Основание связи"}'
```

Snapshot содержит workspace, person, kpi_document, period, tasks, task_details, checklists, task_results, local_mappings, evidence, notes и coverage. По умолчанию подробности до 100 задач; `--limit` до 500. Явно показываются omitted_task_ids, missing_mapped_task_ids и detail_errors. Team snapshot принимает список person IDs и сохраняет полноценный индивидуальный формат, включая ошибки отдельных людей.

Выбор периода включает задачи, активные в месяце, задачи с датированной активностью и локальные mappings. Это текущие данные Bitrix, **не исторический снимок состояния на конец месяца**. Назначение/доступы могли измениться. Отсутствующая привязанная задача отмечается как пробел, не считается провалом KPI.

## Безопасность и производительность

- По умолчанию `file`: приватные локальные файлы вне workspace. На Unix файл создаётся с 0600, каталоги — 0700; слишком широкие права блокируют чтение с подсказкой `bkpi config fix-permissions`. Запись выполняется через временный файл, fsync и atomic rename. На Windows используются ACL пользовательского config-каталога; Unix mode check там не применяется.
- `--backend keychain` выбирает macOS Keychain / Windows Credential Manager / Linux Secret Service. `--backend env` хранит только имя переменной. Старое написание `store = "keyring"` сохранено в config для совместимости; CLI показывает `keychain`. Старый флаг `--file-fallback` остаётся алиасом явного выбора file.
- Webhook должен быть HTTPS `/rest/user/token/`; userinfo, query и fragment запрещены. Redirects отключены, transport errors и response bodies не печатаются. Дополнительно редактируются credential echoes и случайно вставленные webhook/OpenRouter key в данных.
- Config, secrets и backups находятся глобально; workspace не содержит credentials. Не передавайте агенту старый backup — в нём сохранён исходный секрет.
- Bitrix-клиент содержит только read methods. Никаких действий с реальными задачами, сообщений и платежей. Данные передаются только соответствующему Bitrix для чтения; OpenRouter — исключительно по `ai analyze`.
- До 4 одновременных HTTP запросов на integration client, интервал начала запросов 500 ms, timeout 30 s, максимум 3 попытки при transient transport/429/5xx/Bitrix rate-limit errors. Pagination ограничена 10000 элементами с явной ошибкой, без тихой обрезки.
- Внутрипроцессный cache успешных ответов: 15 s, до 1000 entries. Никакого дискового cache рабочих данных. В команде переиспользуются clients одного integration; до 4 person snapshots собираются параллельно.
- Workspace paths не выходят за `.bkpi`, symlinks для документов/state запрещены. MCP input line ограничена 1 MiB. Локальные файлы и агент работают с правами текущего пользователя; это не граница между недоверенными пользователями ОС.

## Credential storage

Общая abstraction `CredentialStore` предоставляет `set/load/remove/exists`; старый `save` сохранён для совместимости. Bitrix получает resolved secret. OpenRouter использует те же backend'ы (`bkpi ai setup --backend file|keychain|env`, env — `--env-var BKPI_OPENROUTER_API_KEY`).

Сохранён существующий per-account формат `secrets/<account>.secret`, а не введён второй `credentials.toml`. Каталог выбирает `dirs::config_dir()`:

| ОС | Файл новой integration `astrovolga` |
|---|---|
| macOS | `~/Library/Application Support/bkpi/secrets/bitrix-astrovolga.secret` |
| Linux | `$XDG_CONFIG_HOME/bkpi/secrets/bitrix-astrovolga.secret`, иначе `~/.config/bkpi/secrets/bitrix-astrovolga.secret` |
| Windows | `%APPDATA%\bkpi\secrets\bitrix-astrovolga.secret` (Roaming AppData) |

`BKPI_CONFIG_DIR` переопределяет каталог для изолированной среды; не направляйте его в workspace. При обновлении и переносе создаётся уникальный account: неудача не перезаписывает working credential. Global `config.toml` содержит только `credential = { store = "file", account = "..." }` (или `keyring`/`env`), а не секрет.

Setup: **Add integration → Private local file (default) / OS secure store / Environment variables → masked webhook для file/keychain → display name → проверка пользователя и задач → сохранение → agent targets**. Для env запрашиваются origin и имя переменной, секрет не запрашивается. Если переменной нет, setup явно сообщает, что integration пока не usable. Повторное обновление сохраняет backend; отдельное действие **Change credential storage** запускает перенос.

```sh
bkpi integration credential show astrovolga
bkpi integration credential show astrovolga --json
bkpi integration credential move astrovolga --backend file
bkpi integration credential move astrovolga --backend keychain
# Set the variable in this process environment first, without a secret in argv/history:
bkpi integration credential move astrovolga --backend env --env-var BKPI_BITRIX_ASTROVOLGA_WEBHOOK --confirm-env
bkpi integration add --backend env --base-url https://astrovolga.bitrix24.ru --env-var BKPI_BITRIX_ASTROVOLGA_WEBHOOK
bkpi config fix-permissions
bkpi integration doctor astrovolga
```

`show` выводит только metadata и configured yes/no; doctor дополнительно проверяет права, Bitrix, пользователя и task access. Для file↔keychain сначала записывается и перечитывается destination, затем атомарно сохраняется config и только после этого удаляется неиспользуемый source. Ошибка удаления даёт предупреждение; новый credential работает. При ошибке до config commit source сохраняется; новый orphan credential может остаться для ручного восстановления. Общая ссылка другой integration/OpenRouter не удаляется.

Env move требует `--confirm-env` и существующий валидный webhook того же origin. BKPI не устанавливает переменную; старый credential остаётся для восстановления. Переменная должна наследоваться каждым процессом runtime/agent, одного изменения shell profile недостаточно для уже запущенного desktop приложения.

## Миграция прототипа

Старый config обнаруживается, но автоматически не изменяется:

```sh
bkpi config migrate
# Optional destination; default is file:
bkpi config migrate --backend keychain
bkpi init
```

На macOS путь по-прежнему `~/Library/Application Support/bkpi/config.toml`.
Перед изменением создаётся `config.legacy-backup.toml` с правами 0600. Single webhook переносится в integration `default`, OpenRouter key — в тот же credential store. Только после успешного сохранения credentials записывается versioned config. Config v1 читается без изменений; при следующей записи или `config migrate` создаётся `config.v1-backup.toml`, затем записывается schema v2 с поддержкой env. Keychain entries не изменяются. Повторная миграция v2 ничего не меняет. При ошибке исходный config и backup сохраняются; созданные до ошибки credential entries можно повторно использовать при повторе.

Старые plans/evidence остаются **в backup**, поскольку их фиксированную семантику нельзя честно перенести в произвольный KPI document. Перенесите нужные записи вручную после создания workspace. Старые OpenRouter privacy flags/model сохранены; остальные legacy настройки доступны в backup. Прежние `plan`, `kpi`, `ai plan/show/clear/model/privacy` больше не являются продуктовым API. Вместо них — workspace state, document и tools. Исходник прототипа сохранён в `examples/legacy-prototype/*.rs.txt`, не компилируется и не устанавливается.

## Optional standalone AI

```sh
cargo build --release --features standalone-ai
bkpi ai setup --model openrouter/auto
bkpi ai doctor
bkpi ai analyze --person eduard --month 2026-09 --json
```

Обычная Cargo-сборка не включает standalone AI. Release workflow включает feature, но вызов остаётся явным. Generic prompt использует документ человека; без KPI анализ блокируется **до сетевого обращения**. Сохранены ZDR/data_collection constraints и подход Responses → Chat fallback для соответствующих маршрутов прототипа. Вывод AI — advisory JSON, без автоматической записи mappings, вычисления фиксированной премии и прежнего disk cache. Live OpenRouter требует отдельной проверки.

## Архитектура и разработка

```text
src/
  lib.rs, main.rs
  bitrix.rs       # сохранённые DTO/read methods, безопасный HTTP
  credentials.rs  # file default, optional keychain/env, permissions, redaction
  config.rs       # integrations, schema version, legacy migration
  workspace.rs    # Person, KpiDocument, LocalState, path validation
  snapshot.rs     # общий core для CLI/MCP, team batching
  mcp.rs          # stdio protocol и tool schemas
  cli.rs          # команды и terminal wizard
  ai.rs           # optional standalone analysis
agent/skill/SKILL.md
installer/src/targets/{codex,claude,cli}.ts
installer/scripts/bundle.mjs
specs/workspace-agents/
tests/
.github/workflows/release.yml
```

Сохранены Bitrix API методы/DTO, пагинация, задачи, чеклисты, результаты, определение пользователя и идеи OpenRouter transport fallback. Профильные calculation/AI/TUI части выведены в архив; `tui` временно показывает обычный workspace JSON без ложного KPI-score. Отдельный Cargo workspace сейчас не нужен.

Проверки:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --all-features
cargo build --release
cd installer
npm ci
npm run typecheck
npm run lint
npm run build
npm test
```

Linux build prerequisites для Secret Service: `libdbus-1-dev`, `pkg-config` (Ubuntu). Пользователь prebuilt runtime не устанавливает Rust, но Linux Secret Service должен быть доступен для безопасного хранения.

Release workflow собирает macOS arm64/x86_64, Linux x86_64/arm64, Windows x86_64. Assets: `bkpi-vVERSION-TARGET[.exe]` + `.sha256`. Push тега создаёт **draft GitHub release** после проверок; npm публикации нет. Workflow ещё нужно выполнить в настоящем GitHub repository. Исходный каталог не является Git checkout, поэтому локальный Git diff недоступен.

Ограничения: нет богатого TUI, PDF/DOCX parser в runtime, дискового task cache, автоматического импорта семантики старого KPI, Bitrix write API или исторического task-state reconstruction. Live Bitrix/OS credential UI/agent-host sessions и сборки остальных платформ требуют соответствующей среды; mocks не заменяют их проверку.

## Изменить источник KPI

```sh
bkpi person kpi show
bkpi person kpi set "/path with spaces/targets.txt"
bkpi person kpi show eduard --json
bkpi person kpi set eduard ~/Documents/targets.md
```

При нескольких людях укажите person ID (или `self`), при одном аргумент необязателен.
`set` меняет только ссылку, без копирования, удаления или перезаписи документов.
`show` выводит Person, resolved path, exists и empty/non-empty; placeholder считается пустым.
`Person.kpi` остаётся произвольным путём. Новые относительные ссылки разрешаются от
корня проекта; legacy `people/<id>/kpi.md` — по-прежнему от `.bkpi`. Для ссылки на
одноимённую папку проекта используйте `./people/...` или абсолютный путь.
Существующие workspace и их документы автоматически не перемещаются.
