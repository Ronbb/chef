-- Apply after explicit migrations, to a dedicated non-owner NOINHERIT,
-- NOSUPERUSER / NOCREATEDB / NOCREATEROLE identity login.
-- psql -v schema=... -v role=... -f infra/database/identity-grants.sql
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
    :"schema".product_user_settings, :"schema".learning_sessions,
    :"schema".lesson_progress, :"schema".step_progress,
    :"schema".exercise_hints, :"schema".exercise_attempts,
    :"schema".learning_operations, :"schema".review_cards,
    :"schema".review_attempts, :"schema".saved_items,
    :"schema".lesson_revisions, :"schema".content_state,
    :"schema".content_releases, :"schema".release_entries,
    :"schema".content_withdrawals, :"schema".media_assets,
    :"schema".character_revisions FROM :"role";
REVOKE ALL ON FUNCTION :"schema".chef_lock_lesson(TEXT,INTEGER),
    :"schema".chef_lock_release_state() FROM :"role";
COMMIT;
