# bkpi v0.5.1

Личный Rust CLI/TUI для Bitrix24, KPI-профиля **«Наставничество и Senior»** и AI-анализа через OpenRouter.

## Что умеет

- забирает задачи текущего пользователя из Bitrix24 через входящий webhook;
- считает локальный KPI по профилю «Наставничество и Senior»;
- хранит KPI-план отдельно по месяцам;
- использует чеклисты задач как operational proxy прогресса;
- забирает зафиксированные результаты задач (`tasks.task.result.list`) как дополнительный evidence для AI;
- отправляет ограниченный snapshot задач/KPI в OpenRouter;
- AI оценивает риски, прогресс и evidence как `confirmed / likely / unknown / negative`;
- фонд показывается тремя уровнями: **формально подтверждено / вероятно закрыто / потенциал периода**;
- task.results считается сильным, но **не обязательным** evidence: закрытая/принятая задача в трекере сама по себе может подтверждать delivery;
- AI предлагает каждую task → KPI связь отдельно, поэтому у `result`, `initiative` и `improvement` теперь свой confidence;
- `bkpi ai plan` по умолчанию интерактивный: можно принять/отклонить каждую связь отдельно; `--apply` остаётся автоматическим режимом;
- в пользовательском AI-выводе показываются **названия задач**, а numeric ID остаются внутренней деталью;
- TUI имеет отдельную вкладку AI; клавиша `a` запускает свежий анализ.

## KPI-профиль

Премиальный фонд: **170 000 ₽**.

| KPI | Фонд |
|---|---:|
| Согласованный результат | 40 000 ₽ |
| Ведение инициативы | 50 000 ₽ |
| Автоматизация / улучшения | 20 000 ₽ |
| Качество изменений | 20 000 ₽ |
| Прозрачность и знания | 40 000 ₽ |

Для согласованного результата применяется порог **85%**.

## Сборка

```bash
cargo build --release
./target/release/bkpi
```

Можно установить бинарник:

```bash
cargo install --path .
```

## Bitrix

Старый config.toml из v0.1–v0.4 совместим.

Новая установка:

```bash
bkpi init 'https://example.bitrix24.ru/rest/123/TOKEN/'
bkpi doctor
```

Webhook должен иметь как минимум scopes `task` и доступ, достаточный для `user.current`.

## OpenRouter

### 1. Сохранить API key

Не передавай ключ аргументом командной строки — `setup` читает его скрыто, чтобы он не попал в shell history:

```bash
bkpi ai setup
```

По умолчанию модель:

```text
openrouter/auto
```

Сразу выбрать другую:

```bash
bkpi ai setup --model openrouter/auto
```

Или поменять потом:

```bash
bkpi ai model openrouter/auto
```

Например, для GPT-5.6 Sol через OpenRouter:

```bash
bkpi ai model openai/gpt-5.6-sol
```

Проверка ключа не делает LLM completion — используется `GET /api/v1/key`:

```bash
bkpi ai doctor
```

### 2. Privacy defaults

По умолчанию bkpi отправляет OpenRouter provider constraints:

```toml
zdr = true
deny_data_collection = true
include_descriptions = true
```

То есть запрос требует Zero Data Retention endpoint и запрещает провайдеров, которые собирают/используют входные данные. Если конкретная модель не имеет совместимого endpoint, OpenRouter может вернуть ошибку — тогда лучше выбрать другую модель, а не сразу выключать privacy.

Посмотреть настройки:

```bash
bkpi ai privacy
```

Изменить:

```bash
bkpi ai privacy --descriptions false
bkpi ai privacy --zdr false
bkpi ai privacy --deny-data-collection false
```

**Важно:** названия, статусы, чеклисты и зафиксированные результаты выбранных рабочих задач отправляются во внешний AI API; при `include_descriptions=true` добавляются и описания. Не используй это для данных, которые политика компании запрещает передавать третьим сторонам.

### 3. Анализ

```bash
bkpi ai analyze
bkpi ai analyze --month 2026-09
bkpi ai analyze --model openrouter/free
```

Snapshot включает в таком порядке:

1. задачи, уже привязанные к KPI-плану;
2. текущие открытые задачи;
3. задачи, закрытые в выбранном месяце.

Для выбранных задач snapshot также включает чеклисты и до 10 последних зафиксированных результатов задачи. Комментарии чата и вложения не читаются и не отправляются.

По умолчанию максимум 40 задач. Очень большие descriptions обрезаются до 4000 символов.

Ответ модели сохраняется в локальный cache с правами `0600`.

Посмотреть последний результат без нового API-вызова:

```bash
bkpi ai show
```

Удалить cache:

```bash
bkpi ai clear
```


### Evidence-модель v0.5

AI больше не трактует пустой `task.results` как автоматический провал доказательной базы. В регламенте accepted task / PR/MR / demo / release являются допустимыми подтверждениями, поэтому `task.results` используется как усиление evidence, а не как обязательный чекбокс.

В анализе каждый KPI получает `evidence_level`:

```text
confirmed  прямо подтверждено доступными данными
likely     очень похоже на выполнение, но остаётся существенная неопределённость
unknown    данных недостаточно
negative   есть evidence невыполнения
```

Из этого локально, а не арифметикой модели, считается:

```text
Формально подтверждено
Вероятно закрыто
Потенциал периода
```

## AI → KPI plan

Модель может предложить, какие задачи подходят под:

- `result`;
- `initiative`;
- `improvement`.

Интерактивно разобрать предложения (по умолчанию confidence ≥ 0.75):

```bash
bkpi ai plan
```

Для каждой связи программа покажет название задачи, конкретный KPI, отдельный confidence и спросит `y/N/q`.

Только посмотреть, ничего не спрашивая и не меняя:

```bash
bkpi ai plan --dry-run
```

Поднять порог:

```bash
bkpi ai plan --min-confidence 0.85
```

Без вопросов применить все предложения выше порога:

```bash
bkpi ai plan --min-confidence 0.85 --apply
```

`quality` и `knowledge` AI автоматически в `met` не переводит: для них нужны реальные evidence/root cause/артефакты.

## Ручной KPI plan

```bash
bkpi plan add result 256473
bkpi plan add result 256472 --weight 2
bkpi plan add initiative 256472
bkpi plan add improvement 256473
bkpi plan list
```

Одна задача может быть привязана к нескольким KPI.

Для другого месяца:

```bash
bkpi plan add result 123456 --month 2026-10
bkpi plan list --month 2026-10
```

Для `result` приложение не даст записать больше 3 задач на период.

## Quality и Knowledge

```bash
bkpi evidence set quality met --note 'Нет повторного критического дефекта по одной причине'
bkpi evidence set knowledge met --note 'Runbook по OKR Engine + разбор для команды'
```

Состояния: `pending`, `met`, `failed`, `blocked`.

## TUI

```bash
bkpi
```

Горячие клавиши:

```text
1        KPI
2        AI
3        Tasks
4 / ?    Help
a        запустить AI-анализ текущего месяца
r        обновить Bitrix
[ / ]    месяц назад / вперёд
q / Esc  выход
```

AI не запускается автоматически при каждом старте/refresh, чтобы не делать неожиданные платные запросы. На вкладке AI показывается последний cached result; `a` обновляет его вручную.

## Основные команды

```bash
bkpi
bkpi doctor
bkpi tasks --open
bkpi tasks --late
bkpi kpi
bkpi kpi --month 2026-09
bkpi ai setup
bkpi ai doctor
bkpi ai analyze
bkpi ai show
bkpi ai plan
bkpi plan list
bkpi config show
```


## v0.5.1 — OpenRouter/Luna compatibility

- HTTP 200 с пустым `choices[0].message.content` теперь считается неуспешной попыткой и автоматически запускает fallback без `response_format`.
- Для JSON-mode включается `provider.require_parameters=true`, чтобы OpenRouter выбирал endpoint с поддержкой параметров запроса.
- Для `openai/gpt-5.6-luna*` используется `reasoning.effort=low` и `reasoning.exclude=true`, чтобы не тратить лишние output tokens на reasoning trace.
- В ошибке теперь видны `finish_reason`, наличие reasoning и refusal, если обе попытки вернули пустой content.


## v0.5.2 — GPT-5.6 через OpenRouter Responses API

- Для `openai/gpt-5.6-*` и будущих `openai/gpt-6-*` анализ сначала идёт через `POST /api/v1/responses`, а `chat/completions` остаётся fallback.
- Это обходит кейс OpenRouter, где отдельный Chat route отвечал HTTP 200 с пустым `choices`.
- `/responses` читает как top-level `output_text`, так и `output[].content[].text`.
- Включён `X-OpenRouter-Metadata: enabled`; при ошибке CLI показывает generation id, router summary и сокращённый raw body вместо бессмысленного «choices пуст».
- HTTP 200 с top-level `error` теперь распознаётся как ошибка и в Chat API, и в Responses API.
- ZDR и `data_collection=deny` сохраняются и для Responses API.
