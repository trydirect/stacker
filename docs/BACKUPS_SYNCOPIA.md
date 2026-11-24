# Бэкапы — дампы в Stacker, хранение в Syncopia (WS-1)

Статус: в работе. 1.1 ✅ (2026-11-24): миграция `20261124142356_create_backup_tables`
(`backup_destination`, `backup_job`, `backup_run`). Крейт `crates/backup-core` ✅:
построение команд дампа/рестора для agent-exec (postgres/mysql/mongo/volume,
токен только через `$BACKUP_TOKEN`), retention-политика B2 (7 daily + 4 weekly) —
тесты 9/9 за ~10мс. 1.2-1.6 впереди (db-слой, REST, CLI, syncopia-коннектор,
планировщик, restore e2e).

План: `config/PLAN-backups-syncopia.md`.
Контракт: `config/shared-fixtures/api-contracts/backups-syncopia.json`.
BDD-сценарии: `config/shared-fixtures/features/backups.feature`.

## Решения (2026-11-24)

- **B1:** дамп выполняется агентом на целевом сервере (одна shell-команда через
  существующую command queue — изменений в агенте не требуется).
- **B2:** retention — 7 ежедневных + 4 еженедельных копии.
- **B3:** хардениг syncopia (default master-key) — внешняя задача в репо syncopia;
  до её закрытия интеграция не выкатывается в прод.

## Архитектура

Чистая логика — в листовом крейте `crates/backup-core` (команды + retention).
Обвязка (Vault, db, планировщик, REST) — в монолите.

- Дамп: `pg_dump -Fc` / `mysqldump` / `mongodump --archive` / `tar | gzip`
  конвейером в `POST {endpoint}/upload` syncopia (Bearer JWT устройства).
- Рестор: `GET {endpoint}/upload/files/{id}` → `pg_restore` / `mysql` /
  `mongorestore` / распаковка тома.
- Секреты устройства syncopia — только в Vault; токен прогона передаётся
  команде через переменную окружения, в строке команды его нет.

## Правила проекта

- Только новые миграции; `cargo sqlx prepare` после изменения sqlx-запросов.
- REST `/api/v1/backups*` — casbin-миграция + `api-reviewer`.
- События `backup.succeeded`/`backup.failed` — в шину WS-N.
