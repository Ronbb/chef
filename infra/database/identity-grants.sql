-- Apply after explicit migrations, to a dedicated non-owner NOINHERIT,
-- NOSUPERUSER / NOCREATEDB / NOCREATEROLE identity login.
-- psql -v schema=... -v learning_schema=... -v role=... -f infra/database/identity-grants.sql
-- The schema owner and migration connection remain separate from runtime logins.
\set ON_ERROR_STOP on
BEGIN;
GRANT USAGE ON SCHEMA :"schema" TO :"role";
REVOKE ALL ON TABLE
    :"schema".users, :"schema".product_memberships,
    :"schema".browser_sessions, :"schema".identity_tokens,
    :"schema".auth_throttle, :"schema".account_admin_audit,
    :"schema".product_membership_audit FROM :"role";
GRANT SELECT, INSERT, UPDATE ON TABLE
    :"schema".users, :"schema".product_memberships TO :"role";
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE
    :"schema".browser_sessions, :"schema".identity_tokens,
    :"schema".auth_throttle TO :"role";
GRANT SELECT, INSERT ON TABLE
    :"schema".account_admin_audit, :"schema".product_membership_audit TO :"role";
GRANT USAGE ON SEQUENCE
    :"schema".users_id_seq, :"schema".account_admin_audit_id_seq,
    :"schema".product_membership_audit_id_seq TO :"role";
REVOKE ALL ON TABLE
    :"learning_schema".product_user_settings, :"learning_schema".learning_sessions,
    :"learning_schema".lesson_progress, :"learning_schema".step_progress,
    :"learning_schema".exercise_hints, :"learning_schema".exercise_attempts,
    :"learning_schema".learning_operations, :"learning_schema".review_cards,
    :"learning_schema".review_attempts, :"learning_schema".saved_items,
    :"learning_schema".lesson_revisions, :"learning_schema".content_state,
    :"learning_schema".content_releases, :"learning_schema".release_entries,
    :"learning_schema".content_withdrawals, :"learning_schema".media_assets,
    :"learning_schema".character_revisions, :"learning_schema".chef_schema_layout FROM :"role";
REVOKE ALL ON FUNCTION :"learning_schema".chef_lock_lesson(TEXT,INTEGER),
    :"learning_schema".chef_lock_release_state() FROM :"role";
COMMIT;
