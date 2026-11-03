-- GDPR user data deletion function for the stacker database.
-- Deletes all data associated with a given user_id across all stacker tables.
-- Follows the pattern of stacker_cleanup_* functions (migration 20260731120000).
--
-- ACCESS CONTROL: Only the postgres superuser (application DB role) can execute.
-- Direct access is revoked from all other roles via REVOKE EXECUTE.
--
-- Usage (admin only):
--   SELECT stacker_delete_user_data('user_123');                    -- dry-run (default)
--   SELECT stacker_delete_user_data('user_123', false);             -- actual deletion
--   SELECT stacker_delete_user_data('user_123', false, true);       -- delete + return vault paths

CREATE OR REPLACE FUNCTION stacker_delete_user_data(
    p_user_id VARCHAR,
    p_dry_run BOOLEAN DEFAULT true,
    p_return_vault_paths BOOLEAN DEFAULT false
)
RETURNS TABLE(table_name TEXT, rows_affected BIGINT, vault_paths TEXT[])
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
    v_count BIGINT;
    v_project_ids INTEGER[];
    v_server_ids INTEGER[];
    v_deployment_hashes TEXT[];
    v_vault_paths TEXT[] := '{}';
    r RECORD;
BEGIN
    -- Admin-only gate: reject if not the postgres superuser
    IF current_user != 'postgres' THEN
        RAISE EXCEPTION 'Access denied: stacker_delete_user_data requires postgres superuser (current_user=%)', current_user
            USING ERRCODE = '42501';  -- insufficient_privilege
    END IF;

    -- Gather project IDs for this user
    SELECT ARRAY(SELECT id FROM project WHERE user_id = p_user_id) INTO v_project_ids;

    -- Gather server IDs for this user
    SELECT ARRAY(SELECT id FROM server WHERE user_id = p_user_id) INTO v_server_ids;

    -- Gather deployment hashes for this user (for Vault agent token cleanup)
    SELECT ARRAY(SELECT deployment_hash FROM deployment WHERE user_id = p_user_id)
        INTO v_deployment_hashes;

    -- Collect Vault paths for SSH keys
    IF array_length(v_server_ids, 1) > 0 THEN
        FOR r IN SELECT unnest(v_server_ids) AS sid
        LOOP
            v_vault_paths := array_append(v_vault_paths,
                'secret/data/users/' || p_user_id || '/ssh_keys/' || r.sid);
        END LOOP;
    END IF;

    -- Collect Vault paths for agent tokens
    IF array_length(v_deployment_hashes, 1) > 0 THEN
        FOR r IN SELECT unnest(v_deployment_hashes) AS dh
        LOOP
            v_vault_paths := array_append(v_vault_paths,
                'agent/' || r.dh || '/token');
            v_vault_paths := array_append(v_vault_paths,
                'agent/' || r.dh || '/runtime');
        END LOOP;
    END IF;

    -- Collect Vault paths for remote secrets
    IF array_length(v_project_ids, 1) > 0 THEN
        FOR r IN SELECT vault_path FROM remote_secret
                 WHERE user_id = p_user_id
        LOOP
            v_vault_paths := array_append(v_vault_paths, r.vault_path);
        END LOOP;
    END IF;

    IF p_dry_run THEN
        -- Dry run: count rows only
        table_name := 'remote_secret';
        SELECT count(*) INTO rows_affected FROM remote_secret WHERE user_id = p_user_id;
        vault_paths := CASE WHEN p_return_vault_paths THEN v_vault_paths ELSE NULL END;
        RETURN NEXT;

        table_name := 'marketplace_install_authorization';
        SELECT count(*) INTO rows_affected FROM marketplace_install_authorization WHERE user_id = p_user_id;
        vault_paths := NULL;
        RETURN NEXT;

        table_name := 'marketplace_event';
        SELECT count(*) INTO rows_affected FROM marketplace_event
            WHERE viewer_user_id = p_user_id OR deployer_user_id = p_user_id;
        RETURN NEXT;

        table_name := 'marketplace_template_event';
        SELECT count(*) INTO rows_affected FROM marketplace_template_event
            WHERE user_id = p_user_id OR viewer_user_id = p_user_id OR deployer_user_id = p_user_id;
        RETURN NEXT;

        table_name := 'marketplace_vendor_profile';
        SELECT count(*) INTO rows_affected FROM marketplace_vendor_profile WHERE creator_user_id = p_user_id;
        RETURN NEXT;

        table_name := 'stack_template_review';
        SELECT count(*) INTO rows_affected FROM stack_template_review WHERE reviewer_user_id = p_user_id;
        RETURN NEXT;

        table_name := 'stack_template';
        SELECT count(*) INTO rows_affected FROM stack_template WHERE creator_user_id = p_user_id;
        RETURN NEXT;

        table_name := 'deployment';
        SELECT count(*) INTO rows_affected FROM deployment WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'chat_conversations';
        SELECT count(*) INTO rows_affected FROM chat_conversations WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'rating';
        SELECT count(*) INTO rows_affected FROM rating WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'user_agreement';
        SELECT count(*) INTO rows_affected FROM user_agreement WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'client';
        SELECT count(*) INTO rows_affected FROM client WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'server';
        SELECT count(*) INTO rows_affected FROM server WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'project_member (as member)';
        SELECT count(*) INTO rows_affected FROM project_member WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'cloud';
        SELECT count(*) INTO rows_affected FROM cloud WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'project';
        SELECT count(*) INTO rows_affected FROM project WHERE user_id = p_user_id;
        RETURN NEXT;

        table_name := 'TOTAL';
        rows_affected := 0;
        vault_paths := CASE WHEN p_return_vault_paths THEN v_vault_paths ELSE NULL END;
        RETURN NEXT;

        RETURN;
    END IF;

    -- === Actual deletion (order respects FK constraints) ===

    -- 1. remote_secret (FK to project and server, both CASCADE)
    DELETE FROM remote_secret WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'remote_secret'; rows_affected := v_count;
    vault_paths := NULL; RETURN NEXT;

    -- 2. marketplace_install_authorization
    DELETE FROM marketplace_install_authorization WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'marketplace_install_authorization'; rows_affected := v_count;
    RETURN NEXT;

    -- 3. marketplace_event (viewer or deployer)
    DELETE FROM marketplace_event WHERE viewer_user_id = p_user_id OR deployer_user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'marketplace_event'; rows_affected := v_count;
    RETURN NEXT;

    -- 4. marketplace_template_event
    DELETE FROM marketplace_template_event
        WHERE user_id = p_user_id OR viewer_user_id = p_user_id OR deployer_user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'marketplace_template_event'; rows_affected := v_count;
    RETURN NEXT;

    -- 5. stack_template_review (reviewer only — templates they created are handled below)
    DELETE FROM stack_template_review WHERE reviewer_user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'stack_template_review'; rows_affected := v_count;
    RETURN NEXT;

    -- 6. stack_template (creator) — ON DELETE CASCADE handles stack_template_review for these
    DELETE FROM stack_template WHERE creator_user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'stack_template'; rows_affected := v_count;
    RETURN NEXT;

    -- 7. deployment (FK to project)
    DELETE FROM deployment WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'deployment'; rows_affected := v_count;
    RETURN NEXT;

    -- 8. chat_conversations
    DELETE FROM chat_conversations WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'chat_conversations'; rows_affected := v_count;
    RETURN NEXT;

    -- 9. rating
    DELETE FROM rating WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'rating'; rows_affected := v_count;
    RETURN NEXT;

    -- 10. user_agreement
    DELETE FROM user_agreement WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'user_agreement'; rows_affected := v_count;
    RETURN NEXT;

    -- 11. client
    DELETE FROM client WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'client'; rows_affected := v_count;
    RETURN NEXT;

    -- 12. server (FK to cloud and project)
    DELETE FROM server WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'server'; rows_affected := v_count;
    RETURN NEXT;

    -- 13. project_member where user is a member (not owner)
    DELETE FROM project_member WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'project_member'; rows_affected := v_count;
    RETURN NEXT;

    -- 14. marketplace_vendor_profile
    DELETE FROM marketplace_vendor_profile WHERE creator_user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'marketplace_vendor_profile'; rows_affected := v_count;
    RETURN NEXT;

    -- 15. cloud
    DELETE FROM cloud WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'cloud'; rows_affected := v_count;
    RETURN NEXT;

    -- 16. project (ON DELETE CASCADE handles project_member for owned projects)
    DELETE FROM project WHERE user_id = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'project'; rows_affected := v_count;
    vault_paths := CASE WHEN p_return_vault_paths THEN v_vault_paths ELSE NULL END;
    RETURN NEXT;

    -- Also clean casbin rules referencing this user
    DELETE FROM casbin_rule WHERE v1 = p_user_id OR v2 = p_user_id;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    table_name := 'casbin_rule'; rows_affected := v_count;
    vault_paths := NULL;
    RETURN NEXT;
END;
$$;

-- Restrict execution to superuser only. The application connects as 'postgres'
-- (superuser), so the function works through the app. Direct access from other
-- DB roles is blocked.
REVOKE EXECUTE ON FUNCTION stacker_delete_user_data(VARCHAR, BOOLEAN, BOOLEAN) FROM PUBLIC;
