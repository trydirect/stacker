# Notifications — каналы уведомлений (Slack / Discord / Telegram / email / webhook)

Статус: WS-N завершён (2026-11-24). N1 ✅ схема, N2 ✅ REST/адаптеры, N3 ✅ шина
событий, N4 ✅ email/`container.down`/CLI. Тесты: notification-core 10/10 (~10мс),
`notification_channels` 9/9, lib зелёный. Остаток усиления (не блокирует):
SSRF-блок приватных диапазонов, admin-harness тест глобальных каналов,
интеграция email с живым notify-сервисом.

План: `config/PLAN-notifications.md`.
Контракты (источник истины):
- `config/shared-fixtures/api-contracts/notifications-channels.json`
- `config/shared-fixtures/api-contracts/notifications-events.json`
BDD-сценарии: `config/shared-fixtures/features/notifications.feature`.

## Модель

Архитектура модулей (2026-11-24): чистая логика вынесена в листовой крейт
`crates/notification-core` (события, payload'ы каналов, маскирование, redaction,
retry/backoff, HTTP-доставка) — его юнит-тесты гоняются без сборки монолита;
обвязка (Vault, db-слой, роуты) остаётся в `src/services/notification.rs` и
`src/routes/notification/`. Такой же принцип действует для следующих WS-потоков:
чистая логика — в `crates/`, интеграция — в монолит.

- `notification_channel` — канал: per-project или глобальный admin-канал
  (`project_id IS NULL`, решение N1 от 2026-11-24). Типы: `slack`, `discord`,
  `telegram`, `email`, `webhook`. Глобальный канал может создать только
  пользователь с ролью `root`/`group_admin`. Секреты (webhook URL, bot token)
  хранятся **только в Vault** по `vault_path` и никогда не возвращаются
  API/логами/MCP — в ответах только `targetHint` (замаскированный адрес).
- `notification_dispatch` — попытка доставки: `status` (`pending`/`sent`/`failed`/
  `skipped`), `attempts`, `last_error` (секреты редактируются перед записью).

## События (edge-triggered)

`deploy.succeeded`, `deploy.failed`, `backup.succeeded`, `backup.failed`,
`preview.ready`, `preview.failed`, `container.down`, `channel.test`.

Одно уведомление на переход состояния, не на каждый опрос. Доставка at-least-once
с retry/backoff на 429/5xx. Фильтр `events` канала — allow-list; пустой список =
все события. **События проекта уходят только в каналы этого проекта**; глобальные
admin-каналы зарезервированы под события уровня организации (защита от
кросс-тенантной утечки, code-review B1).

## Связи с другими потоками

- WS-1 (бэкапы, `PLAN-backups-syncopia.md`): события `backup.succeeded/failed`.
- WS-G (GitHub, `PLAN-github-webhooks.md`): `preview.ready/failed` дополнительно
  доставляются PR-комментарием через GitHub App (не через адаптеры каналов).
- Существующий webhook-таргет `stacker monitor`
  (`src/console/commands/cli/monitor.rs:63-92`) подключается к той же шине как
  событие `container.down`.
- Email — через существующий канал RabbitMQ → notify
  (`src/services/project_cleanup_notifier.rs`).

## Правила проекта

- Только новые миграции; `cargo sqlx prepare` после изменения sqlx-запросов.
- Новые endpoints — ревью `api-reviewer` + casbin-миграция
  (маршруты `/api/v1/notifications/*`).
- Секреты — только в Vault (`src/services/vault_service.rs`).
