# GitHub-интеграция — App, вебхуки, watch paths, auto-deploy (WS-G)

Статус: в работе. G1 ✅ (2026-11-24): схема `github_installation`, `github_repo`,
`github_webhook_delivery` + casbin-правило анонимного вебхука.
G2 ✅ (2026-11-24): ресивер `POST /api/v1/github/webhook` — проверка
`X-Hub-Signature-256` (HMAC, constant-time), anti-replay по `X-GitHub-Delivery`,
нормализация событий. G3 ✅ (2026-11-24): watch paths (glob + `!`-исключения).
G4 ✅ (2026-11-24): Repo Connect API `/api/v1/github/repos` (CRUD, owner-gated),
installation-события пишутся в `github_installation`; переключатель auto-deploy —
`github_repo.autodeploy` (поле каталога `autodeploy` вытеснено).
G5 (триггер деплоя из matched-списка) — следующий шаг: примитив
`redeploy_saved_project` (см. план).

План: `config/PLAN-github-webhooks.md` (продолжение `PLAN-managed-previews.md`).
Контракт: `config/shared-fixtures/api-contracts/github-webhooks.json`.
BDD-сценарии: `config/shared-fixtures/features/github-autodeploy.feature`.

## Архитектура модулей

Чистая логика — в листовом крейте `crates/github-events` (верификация подписи,
нормализация `push`/`pull_request`/`installation`/`installation_repositories`,
матчинг watch paths) — его тесты идут без сборки монолита. Обвязка (Vault,
db, роуты) — в `src/routes/github/`, `src/db/github.rs`, `src/models/github.rs`.

## Модель

- `github_installation` — установка GitHub App (user_id ↔ installation_id).
- `github_repo` — подключённый репозиторий: repo, branch, `watch_paths[]`,
  `autodeploy`, привязка к `project_id`.
- `github_webhook_delivery` — anti-replay: delivery id обрабатывается один раз.

## Вебхук

- Аутентификация — только `X-Hub-Signature-256` (секрет в Vault по пути
  `github/webhook_secret`, casbin-маршрут анонимный — прецедент payout webhook).
- Повтор delivery id → `200 {duplicate: true}` без обработки.
- Push матчится по repo + branch + watch paths → список `matched` в ответе
  (шов для G5: триггер deploy plan оттуда).

## Правила проекта

- Только новые миграции; `cargo sqlx prepare` после изменения sqlx-запросов.
- G4 добавит REST-эндпоинты подключения репо → casbin-миграция + `api-reviewer`.
- Секреты (webhook secret, App private key) — только в Vault.
