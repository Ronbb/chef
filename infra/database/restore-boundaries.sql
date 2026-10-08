-- Explicit owner step after --no-owner / --no-acl restoration, before runtime grants.
-- Only use the two restored application schemas; not system or unrelated schemas.
-- psql -v identity_schema=... -v learning_schema=... -f restore-boundaries.sql
\set ON_ERROR_STOP on
BEGIN;
REVOKE ALL ON SCHEMA :"identity_schema", :"learning_schema" FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA :"identity_schema", :"learning_schema" FROM PUBLIC;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA :"identity_schema", :"learning_schema" FROM PUBLIC;
-- PostgreSQL restores functions with default PUBLIC EXECUTE when ACLs are omitted.
-- This includes SECURITY DEFINER helpers. Dedicated runtime templates grant back
-- only the permitted lock functions; trigger execution remains tied to its tables.
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA :"identity_schema", :"learning_schema" FROM PUBLIC;
COMMIT;
