-- Split layouts only, after migrate-layout and base learning-grants.sql.
-- psql -v schema=... -v role=... -f infra/database/learning-product-grants.sql
\set ON_ERROR_STOP on
BEGIN;
GRANT EXECUTE ON FUNCTION :"schema".chef_lock_product_release_state(TEXT) TO :"role";
COMMIT;
