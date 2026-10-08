-- Owner-only product-local speech artifact graph. Unknown dependencies must be migrated explicitly.
DO $chef$
DECLARE
    item record;
    parent_oid oid;
    product_column smallint;
    legacy_columns smallint[];
    primary_name text;
    edge record;
BEGIN
    FOR item IN SELECT * FROM (VALUES
        ('course_speech_plans',ARRAY['id'],'chef_local_speech_plan_primary'),
        ('course_speech_clips',ARRAY['id'],'chef_local_speech_clip_primary'),
        ('course_speech_clip_events',ARRAY['clip_id','version'],'chef_local_speech_event_primary'),
        ('course_speech_clip_reviews',ARRAY['clip_id'],'chef_local_speech_review_primary'),
        ('speech_alignments',ARRAY['id'],'chef_local_alignment_primary'),
        ('speech_alignment_reviews',ARRAY['alignment_id','clip_id'],'chef_local_alignment_review_primary'),
        ('speech_package_imports',ARRAY['id'],'chef_local_package_primary')
    ) keys(table_name,column_names,constraint_name)
    LOOP
        parent_oid := format('%I.%I',current_schema(),item.table_name)::regclass;
        EXECUTE format('LOCK TABLE %I.%I IN ACCESS EXCLUSIVE MODE',current_schema(),item.table_name);
        SELECT attnum INTO STRICT product_column FROM pg_catalog.pg_attribute
            WHERE attrelid=parent_oid AND attname='product_id' AND NOT attisdropped;
        SELECT array_agg(a.attnum ORDER BY k.position) INTO legacy_columns
            FROM unnest(item.column_names) WITH ORDINALITY k(name,position)
            JOIN pg_catalog.pg_attribute a ON a.attrelid=parent_oid AND a.attname=k.name AND NOT a.attisdropped;
        SELECT conname INTO STRICT primary_name FROM pg_catalog.pg_constraint
            WHERE conrelid=parent_oid AND contype='p' AND conkey=legacy_columns;
        FOR edge IN
            SELECT c.*,n.nspname,t.relname FROM pg_catalog.pg_constraint c
            JOIN pg_catalog.pg_class t ON t.oid=c.conrelid
            JOIN pg_catalog.pg_namespace n ON n.oid=t.relnamespace
            WHERE c.contype='f' AND c.confrelid=parent_oid AND c.confkey=legacy_columns
        LOOP
            IF edge.nspname <> current_schema() OR NOT EXISTS (
                SELECT 1 FROM pg_catalog.pg_constraint replacement
                JOIN pg_catalog.pg_attribute a ON a.attrelid=edge.conrelid AND a.attname='product_id' AND NOT a.attisdropped
                WHERE replacement.contype='f' AND replacement.conrelid=edge.conrelid
                    AND replacement.confrelid=parent_oid AND replacement.convalidated
                    AND replacement.conkey=ARRAY[a.attnum]::smallint[] || edge.conkey
                    AND replacement.confkey=ARRAY[product_column]::smallint[] || edge.confkey
                    AND replacement.confupdtype=edge.confupdtype AND replacement.confdeltype=edge.confdeltype
                    AND replacement.confmatchtype=edge.confmatchtype AND replacement.condeferrable=edge.condeferrable
                    AND replacement.condeferred=edge.condeferred
            ) THEN RAISE EXCEPTION 'Unverified legacy speech work dependency'; END IF;
        END LOOP;
        FOR edge IN
            SELECT c.conname,n.nspname,t.relname FROM pg_catalog.pg_constraint c
            JOIN pg_catalog.pg_class t ON t.oid=c.conrelid
            JOIN pg_catalog.pg_namespace n ON n.oid=t.relnamespace
            WHERE c.contype='f' AND c.confrelid=parent_oid AND c.confkey=legacy_columns
        LOOP
            EXECUTE format('ALTER TABLE %I.%I DROP CONSTRAINT %I',edge.nspname,edge.relname,edge.conname);
        END LOOP;
        EXECUTE format('ALTER TABLE %I.%I DROP CONSTRAINT %I',current_schema(),item.table_name,primary_name);
        EXECUTE format('ALTER TABLE %I.%I ADD CONSTRAINT %I PRIMARY KEY(product_id,%s)',current_schema(),item.table_name,item.constraint_name,
            (SELECT string_agg(format('%I',k.name),',' ORDER BY k.position) FROM unnest(item.column_names) WITH ORDINALITY k(name,position)));
    END LOOP;
    SELECT conname INTO STRICT primary_name FROM pg_catalog.pg_constraint
        WHERE conrelid='speech_package_imports'::regclass AND contype='u'
        AND (SELECT array_agg(a.attname::text ORDER BY k.position) FROM unnest(conkey) WITH ORDINALITY k(column_number,position)
            JOIN pg_catalog.pg_attribute a ON a.attrelid=conrelid AND a.attnum=k.column_number)=ARRAY['lesson_id','revision']::text[];
    EXECUTE format('ALTER TABLE %I.speech_package_imports DROP CONSTRAINT %I',current_schema(),primary_name);
END
$chef$;
ALTER TABLE speech_package_imports ADD CONSTRAINT chef_local_package_lesson UNIQUE(product_id,lesson_id,revision);
-- Retain product candidate keys used by validated foreign keys; every event/review remains immutable.
