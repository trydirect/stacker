DROP INDEX IF EXISTS idx_backup_run_status;
DROP INDEX IF EXISTS idx_backup_run_job;
DROP TABLE IF EXISTS backup_run;

DROP INDEX IF EXISTS idx_backup_job_project;
DROP INDEX IF EXISTS idx_backup_job_user_project_name;
DROP TABLE IF EXISTS backup_job;

DROP INDEX IF EXISTS idx_backup_destination_user_name;
DROP TABLE IF EXISTS backup_destination;
