-- Keep singleton as the legacy Brioche selector until every old caller is migrated.
ALTER TABLE content_state DROP CONSTRAINT content_state_pkey;
ALTER TABLE content_state DROP CONSTRAINT content_state_singleton_check;
ALTER TABLE content_state ADD PRIMARY KEY(product_id);
ALTER TABLE content_state ADD CONSTRAINT chef_state_legacy_selector CHECK(singleton=(product_id='brioche'));
DO $$
DECLARE schema_name TEXT := current_schema();
BEGIN
    EXECUTE format('CREATE FUNCTION %I.chef_lock_product_release_state(TEXT) RETURNS TEXT LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path=pg_catalog AS %L',schema_name,
        format('SELECT active_release FROM %I.content_state WHERE product_id=$1 FOR SHARE',schema_name));
    EXECUTE format('REVOKE ALL ON FUNCTION %I.chef_lock_product_release_state(TEXT) FROM PUBLIC',schema_name);
END $$;
