-- TryDirect Marketplace Schema Migration

-- Ensure UUID generation
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- 1. Categories (needed by templates)
CREATE TABLE IF NOT EXISTS stack_category (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL
);

-- 2. Core marketplace tables
CREATE TABLE IF NOT EXISTS stack_template (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creator_user_id VARCHAR(50) NOT NULL,
    creator_name VARCHAR(255),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) UNIQUE NOT NULL,
    short_description TEXT,
    long_description TEXT,
    category_id INTEGER REFERENCES stack_category(id),
    tags JSONB DEFAULT '[]'::jsonb,
    tech_stack JSONB DEFAULT '{}'::jsonb,
    status VARCHAR(50) NOT NULL DEFAULT 'draft' CHECK (
        status IN ('draft', 'submitted', 'under_review', 'approved', 'rejected', 'deprecated')
    ),
    plan_type VARCHAR(50) DEFAULT 'free' CHECK (
        plan_type IN ('free', 'one_time', 'subscription')
    ),
    price DOUBLE PRECISION,
    currency VARCHAR(3) DEFAULT 'USD',
    is_configurable BOOLEAN DEFAULT true,
    view_count INTEGER DEFAULT 0,
    deploy_count INTEGER DEFAULT 0,
    average_rating REAL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    approved_at TIMESTAMP WITH TIME ZONE
);

CREATE TABLE IF NOT EXISTS stack_template_version (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES stack_template(id) ON DELETE CASCADE,
    version VARCHAR(20) NOT NULL,
    stack_definition JSONB NOT NULL,
    definition_format VARCHAR(20) DEFAULT 'yaml',
    changelog TEXT,
    is_latest BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    UNIQUE(template_id, version)
);

CREATE TABLE IF NOT EXISTS stack_template_review (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES stack_template(id) ON DELETE CASCADE,
    reviewer_user_id VARCHAR(50),
    decision VARCHAR(50) NOT NULL DEFAULT 'pending' CHECK (
        decision IN ('pending', 'approved', 'rejected', 'needs_changes')
    ),
    review_reason TEXT,
    security_checklist JSONB DEFAULT '{
        "no_secrets": null,
        "no_hardcoded_creds": null,
        "valid_docker_syntax": null,
        "no_malicious_code": null
    }'::jsonb,
    submitted_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    reviewed_at TIMESTAMP WITH TIME ZONE
);

CREATE TABLE IF NOT EXISTS stack_template_rating (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES stack_template(id) ON DELETE CASCADE,
    user_id VARCHAR(50) NOT NULL,
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
    rate_category VARCHAR(100),
    review_text TEXT,
    is_flagged BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    UNIQUE(template_id, user_id, rate_category)
);

-- Monetization
CREATE TABLE IF NOT EXISTS stack_template_plan (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES stack_template(id) ON DELETE CASCADE,
    plan_code VARCHAR(50) NOT NULL,
    price DOUBLE PRECISION,
    currency VARCHAR(3) DEFAULT 'USD',
    period VARCHAR(20) DEFAULT 'one_time',
    description TEXT,
    includes JSONB DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now()
);

CREATE TABLE IF NOT EXISTS template_purchase (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES stack_template(id),
    plan_id UUID NOT NULL REFERENCES stack_template_plan(id),
    buyer_user_id VARCHAR(50) NOT NULL,
    creator_user_id VARCHAR(50) NOT NULL,
    amount DOUBLE PRECISION,
    currency VARCHAR(3),
    stripe_charge_id VARCHAR(255),
    creator_share DOUBLE PRECISION,
    platform_share DOUBLE PRECISION,
    status VARCHAR(50) DEFAULT 'completed',
    purchased_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    refunded_at TIMESTAMP WITH TIME ZONE
);

-- Extend existing tables
DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'project' AND column_name = 'source_template_id'
    ) THEN
        ALTER TABLE project ADD COLUMN source_template_id UUID REFERENCES stack_template(id);
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'project' AND column_name = 'template_version'
    ) THEN
        ALTER TABLE project ADD COLUMN template_version VARCHAR(20);
    END IF;
END $$;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_stack_template_creator ON stack_template(creator_user_id);
CREATE INDEX IF NOT EXISTS idx_stack_template_status ON stack_template(status);
CREATE INDEX IF NOT EXISTS idx_stack_template_slug ON stack_template(slug);
CREATE INDEX IF NOT EXISTS idx_stack_template_category ON stack_template(category_id);

CREATE INDEX IF NOT EXISTS idx_template_version_template ON stack_template_version(template_id);
CREATE INDEX IF NOT EXISTS idx_template_version_latest ON stack_template_version(template_id, is_latest) WHERE is_latest = true;

CREATE INDEX IF NOT EXISTS idx_review_template ON stack_template_review(template_id);
CREATE INDEX IF NOT EXISTS idx_review_decision ON stack_template_review(decision);

CREATE INDEX IF NOT EXISTS idx_template_rating_template ON stack_template_rating(template_id);
CREATE INDEX IF NOT EXISTS idx_template_rating_user ON stack_template_rating(user_id);

CREATE INDEX IF NOT EXISTS idx_purchase_template ON template_purchase(template_id);
CREATE INDEX IF NOT EXISTS idx_purchase_buyer ON template_purchase(buyer_user_id);
CREATE INDEX IF NOT EXISTS idx_purchase_creator ON template_purchase(creator_user_id);

CREATE INDEX IF NOT EXISTS idx_project_source_template ON project(source_template_id);

-- Triggers
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ language 'plpgsql';

DROP TRIGGER IF EXISTS update_stack_template_updated_at ON stack_template;
CREATE TRIGGER update_stack_template_updated_at
    BEFORE UPDATE ON stack_template
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

DROP TRIGGER IF EXISTS update_stack_template_plan_updated_at ON stack_template_plan;
CREATE TRIGGER update_stack_template_plan_updated_at
    BEFORE UPDATE ON stack_template_plan
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Maintain average_rating on stack_template
CREATE OR REPLACE FUNCTION update_template_average_rating()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE stack_template 
    SET average_rating = (
        SELECT AVG(rating::DECIMAL) 
        FROM stack_template_rating 
        WHERE template_id = COALESCE(OLD.template_id, NEW.template_id)
    )
    WHERE id = COALESCE(OLD.template_id, NEW.template_id);
    RETURN NULL;
END;
$$ language 'plpgsql';

DROP TRIGGER IF EXISTS maintain_template_rating ON stack_template_rating;
CREATE TRIGGER maintain_template_rating
    AFTER INSERT OR UPDATE OR DELETE ON stack_template_rating
    FOR EACH ROW EXECUTE FUNCTION update_template_average_rating();

-- Seed sample categories
INSERT INTO stack_category (name) 
VALUES 
    ('AI Agents'), 
    ('Data Pipelines'), 
    ('SaaS Starter'), 
    ('Dev Tools'),
    ('Automation')
ON CONFLICT DO NOTHING;
