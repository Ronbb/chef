-- Trusted author CLI, separate from a web runtime login; apply as owner.
-- psql -v schema=... -v identity_schema=... -v role=... -f infra/database/author-grants.sql
-- Reuse content permissions rather than maintain a second business grant list.
\set ON_ERROR_STOP on
\ir content-grants.sql
BEGIN;
GRANT SELECT ON TABLE :"schema".chef_schema_layout,
    :"schema".chef_layout_migrations, :"schema".seaql_migrations TO :"role";
COMMIT;
