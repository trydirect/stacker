-- Rollback TryDirect Marketplace Schema

DROP TRIGGER IF EXISTS maintain_template_rating ON stack_template_rating;
DROP FUNCTION IF EXISTS update_template_average_rating();

DROP TRIGGER IF EXISTS update_stack_template_plan_updated_at ON stack_template_plan;
DROP TRIGGER IF EXISTS update_stack_template_updated_at ON stack_template;
DROP FUNCTION IF EXISTS update_updated_at_column();

DROP INDEX IF EXISTS idx_project_source_template;

DROP INDEX IF EXISTS idx_purchase_creator;
DROP INDEX IF EXISTS idx_purchase_buyer;
DROP INDEX IF EXISTS idx_purchase_template;

DROP INDEX IF EXISTS idx_template_rating_user;
DROP INDEX IF EXISTS idx_template_rating_template;

DROP INDEX IF EXISTS idx_review_decision;
DROP INDEX IF EXISTS idx_review_template;

DROP INDEX IF EXISTS idx_template_version_latest;
DROP INDEX IF EXISTS idx_template_version_template;

DROP INDEX IF EXISTS idx_stack_template_category;
DROP INDEX IF EXISTS idx_stack_template_slug;
DROP INDEX IF EXISTS idx_stack_template_status;
DROP INDEX IF EXISTS idx_stack_template_creator;

ALTER TABLE IF EXISTS stack DROP COLUMN IF EXISTS is_user_submitted;
ALTER TABLE IF EXISTS stack DROP COLUMN IF EXISTS marketplace_template_id;
ALTER TABLE IF EXISTS project DROP COLUMN IF EXISTS template_version;
ALTER TABLE IF EXISTS project DROP COLUMN IF EXISTS source_template_id;

DROP TABLE IF EXISTS template_purchase;
DROP TABLE IF EXISTS stack_template_plan;
DROP TABLE IF EXISTS stack_template_rating;
DROP TABLE IF EXISTS stack_template_review;
DROP TABLE IF EXISTS stack_template_version;
DROP TABLE IF EXISTS stack_template;

-- Keep categories table if used elsewhere; comment out to drop
-- DROP TABLE IF EXISTS stack_category;
