-- Only lock the fixed deployment product's course; return no documents or private source.
DO $$
DECLARE schema_name TEXT := current_schema();
BEGIN
    EXECUTE format('CREATE FUNCTION %I.chef_lock_product_lesson(TEXT,TEXT,INTEGER) RETURNS BOOLEAN LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path=pg_catalog AS %L',schema_name,
        format('SELECT EXISTS(SELECT 1 FROM %I.lesson_revisions WHERE product_id=$1 AND lesson_id=$2 AND revision=$3 FOR SHARE)',schema_name));
    EXECUTE format('REVOKE ALL ON FUNCTION %I.chef_lock_product_lesson(TEXT,TEXT,INTEGER) FROM PUBLIC',schema_name);
END $$;
