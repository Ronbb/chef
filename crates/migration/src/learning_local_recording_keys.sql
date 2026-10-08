-- Owner-only maintenance: replace only proven legacy recording edges, never CASCADE.
LOCK TABLE audio_assets IN ACCESS EXCLUSIVE MODE;
DO $chef$
DECLARE
    parent_oid oid := 'audio_assets'::regclass;
    product_column smallint;
    legacy_columns smallint[];
    primary_name text;
    edge record;
BEGIN
    SELECT attnum INTO STRICT product_column FROM pg_catalog.pg_attribute
        WHERE attrelid=parent_oid AND attname='product_id' AND NOT attisdropped;
    SELECT array_agg(attnum ORDER BY CASE attname WHEN 'asset_id' THEN 0 ELSE 1 END)
        INTO legacy_columns FROM pg_catalog.pg_attribute
        WHERE attrelid=parent_oid AND attname IN ('asset_id','revision') AND NOT attisdropped;
    SELECT conname INTO STRICT primary_name FROM pg_catalog.pg_constraint
        WHERE conrelid=parent_oid AND contype='p' AND conkey=legacy_columns;
    -- Verify every old edge before dropping any. Unknown extensions must be migrated explicitly.
    FOR edge IN
        SELECT c.*,n.nspname,t.relname FROM pg_catalog.pg_constraint c
        JOIN pg_catalog.pg_class t ON t.oid=c.conrelid
        JOIN pg_catalog.pg_namespace n ON n.oid=t.relnamespace
        WHERE c.contype='f' AND c.confrelid=parent_oid AND c.confkey=legacy_columns
    LOOP
        IF edge.nspname <> current_schema() OR NOT EXISTS (
            SELECT 1 FROM pg_catalog.pg_constraint replacement
            JOIN pg_catalog.pg_attribute a ON a.attrelid=edge.conrelid
                AND a.attname='product_id' AND NOT a.attisdropped
            WHERE replacement.contype='f' AND replacement.conrelid=edge.conrelid
                AND replacement.confrelid=parent_oid AND replacement.convalidated
                AND replacement.conkey=ARRAY[a.attnum]::smallint[] || edge.conkey
                AND replacement.confkey=ARRAY[product_column]::smallint[] || edge.confkey
                AND replacement.confupdtype=edge.confupdtype
                AND replacement.confdeltype=edge.confdeltype
                AND replacement.confmatchtype=edge.confmatchtype
                AND replacement.condeferrable=edge.condeferrable
                AND replacement.condeferred=edge.condeferred
        ) THEN
            RAISE EXCEPTION 'Unverified legacy recording dependency';
        END IF;
    END LOOP;
    FOR edge IN
        SELECT c.conname,n.nspname,t.relname FROM pg_catalog.pg_constraint c
        JOIN pg_catalog.pg_class t ON t.oid=c.conrelid
        JOIN pg_catalog.pg_namespace n ON n.oid=t.relnamespace
        WHERE c.contype='f' AND c.confrelid=parent_oid AND c.confkey=legacy_columns
    LOOP
        EXECUTE format('ALTER TABLE %I.%I DROP CONSTRAINT %I',edge.nspname,edge.relname,edge.conname);
    END LOOP;
    EXECUTE format('ALTER TABLE %I.audio_assets DROP CONSTRAINT %I',current_schema(),primary_name);
END
$chef$;
ALTER TABLE audio_assets ADD CONSTRAINT chef_local_recording_primary
    PRIMARY KEY(product_id,asset_id,revision);
-- Retain chef_recording_product: validated product foreign keys depend on that candidate key.
