-- Recording ownership preparation. Existing global IDs and consumers remain.
DO $$
DECLARE item TEXT;
BEGIN
    FOREACH item IN ARRAY ARRAY['audio_assets','audio_import_audit'] LOOP
        EXECUTE format('ALTER TABLE %I ADD COLUMN product_id TEXT NOT NULL DEFAULT ''brioche'' CHECK(product_id IN (''brioche'',''hargow''))',item);
        EXECUTE format('CREATE TRIGGER chef_recording_owner BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product()',item);
    END LOOP;
END $$;
ALTER TABLE audio_assets ADD CONSTRAINT chef_recording_product UNIQUE(product_id,asset_id,revision);
