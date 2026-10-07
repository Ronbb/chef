-- Apply as the schema owner after explicit migrations, to a dedicated non-owner,
-- NOINHERIT / NOSUPERUSER / NOCREATEDB / NOCREATEROLE learning login.
-- psql -v schema=... -v role=... -f infra/database/learning-grants.sql
-- Secrets and role creation belong to private deployment provisioning.
\set ON_ERROR_STOP on
BEGIN;
GRANT USAGE ON SCHEMA :"schema" TO :"role";
REVOKE ALL ON TABLE
    :"schema".users, :"schema".browser_sessions, :"schema".identity_tokens,
    :"schema".auth_throttle, :"schema".product_memberships,
    :"schema".product_membership_audit, :"schema".account_admin_audit
    FROM :"role";
GRANT SELECT ON TABLE
    :"schema".product_user_settings, :"schema".learning_sessions,
    :"schema".lesson_progress, :"schema".step_progress,
    :"schema".exercise_hints, :"schema".exercise_attempts,
    :"schema".learning_operations, :"schema".review_cards,
    :"schema".review_attempts, :"schema".saved_items,
    :"schema".lesson_revisions, :"schema".content_state,
    :"schema".content_releases, :"schema".release_entries,
    :"schema".content_withdrawals, :"schema".media_assets,
    :"schema".character_revisions TO :"role";
REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON TABLE
    :"schema".lesson_revisions, :"schema".content_state,
    :"schema".content_releases, :"schema".release_entries,
    :"schema".content_withdrawals, :"schema".media_assets,
    :"schema".character_revisions FROM :"role";
GRANT INSERT, UPDATE ON TABLE
    :"schema".product_user_settings, :"schema".learning_sessions,
    :"schema".lesson_progress, :"schema".step_progress,
    :"schema".review_cards, :"schema".saved_items TO :"role";
GRANT INSERT ON TABLE
    :"schema".exercise_hints, :"schema".exercise_attempts,
    :"schema".learning_operations, :"schema".review_attempts TO :"role";
GRANT EXECUTE ON FUNCTION :"schema".chef_lock_lesson(TEXT,INTEGER),
    :"schema".chef_lock_release_state() TO :"role";
COMMIT;
