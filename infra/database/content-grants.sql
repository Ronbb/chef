-- Courses, assets, character directions, references, enrollment, auditions, plans and clips.
-- Apply as owner to a fresh non-owner NOINHERIT / NOSUPERUSER login.
-- psql -v schema=... -v identity_schema=... -v role=... -f infra/database/content-grants.sql
\set ON_ERROR_STOP on
BEGIN;
GRANT USAGE ON SCHEMA :"schema" TO :"role";
REVOKE ALL ON TABLE
    :"identity_schema".users, :"identity_schema".browser_sessions,
    :"identity_schema".identity_tokens, :"identity_schema".auth_throttle,
    :"identity_schema".product_memberships, :"identity_schema".product_membership_audit,
    :"identity_schema".account_admin_audit,
    :"schema".product_user_settings, :"schema".learning_sessions,
    :"schema".lesson_progress, :"schema".step_progress,
    :"schema".exercise_hints, :"schema".exercise_attempts,
    :"schema".learning_operations, :"schema".review_cards,
    :"schema".review_attempts, :"schema".saved_items FROM :"role";
GRANT SELECT ON TABLE
    :"schema".lesson_revisions, :"schema".content_state,
    :"schema".content_releases, :"schema".release_entries,
    :"schema".content_withdrawals, :"schema".content_audit,
    :"schema".editorial_reviews, :"schema".lesson_import_audit,
    :"schema".media_assets, :"schema".character_revisions,
    :"schema".asset_import_audit, :"schema".audio_assets, :"schema".audio_import_audit,
    :"schema".character_voice_profiles, :"schema".voice_reference_grants,
    :"schema".voice_reference_revocations, :"schema".voice_clone_jobs,
    :"schema".voice_reference_reads,
    :"schema".voice_clone_events, :"schema".voice_auditions, :"schema".voice_audition_reviews,
    :"schema".voice_audition_events,
    :"schema".course_speech_plans, :"schema".course_speech_clips,
    :"schema".course_speech_clip_events,
    :"schema".course_speech_clip_reviews, :"schema".speech_alignments,
    :"schema".speech_alignment_reviews, :"schema".speech_package_imports,
    :"schema".lesson_audio_reviews, :"schema".lesson_direct_publications TO :"role";
GRANT INSERT ON TABLE
    :"schema".lesson_revisions, :"schema".lesson_import_audit,
    :"schema".content_releases, :"schema".release_entries,
    :"schema".content_withdrawals, :"schema".content_audit,
    :"schema".editorial_reviews,
    :"schema".media_assets, :"schema".asset_import_audit,
    :"schema".audio_assets, :"schema".audio_import_audit,
    :"schema".character_revisions, :"schema".character_voice_profiles,
    :"schema".voice_reference_grants, :"schema".voice_reference_revocations,
    :"schema".voice_reference_reads,
    :"schema".voice_clone_jobs, :"schema".voice_clone_events,
    :"schema".voice_auditions, :"schema".voice_audition_events,
    :"schema".voice_audition_reviews, :"schema".course_speech_plans,
    :"schema".course_speech_clips, :"schema".course_speech_clip_events,
    :"schema".course_speech_clip_reviews TO :"role";
-- PostgreSQL FOR UPDATE requires UPDATE privilege; immutable trigger still
-- rejects actual changes. Only the grant key is granted for locking.
GRANT UPDATE(id) ON :"schema".voice_reference_grants TO :"role";
GRANT UPDATE(id) ON :"schema".voice_clone_jobs TO :"role";
GRANT UPDATE(id) ON :"schema".voice_auditions TO :"role";
GRANT UPDATE(published) ON :"schema".lesson_revisions TO :"role";
GRANT UPDATE(active_release,generation) ON :"schema".content_state TO :"role";
GRANT USAGE ON SEQUENCE :"schema".content_audit_id_seq TO :"role";
GRANT USAGE ON SEQUENCE :"schema".asset_import_audit_id_seq TO :"role";
GRANT USAGE ON SEQUENCE :"schema".audio_import_audit_id_seq TO :"role";
GRANT USAGE ON SEQUENCE :"schema".voice_reference_reads_id_seq TO :"role";
COMMIT;
