-- Product ownership preparation; scoped registry/runtime and local IDs follow.
DO $$
DECLARE item TEXT;
BEGIN
    FOREACH item IN ARRAY ARRAY['media_assets','character_revisions','asset_import_audit'] LOOP
        EXECUTE format('ALTER TABLE %I ADD COLUMN product_id TEXT NOT NULL DEFAULT ''brioche'' CHECK(product_id IN (''brioche'',''hargow''))',item);
        EXECUTE format('CREATE TRIGGER chef_visual_owner BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product()',item);
    END LOOP;
END $$;
ALTER TABLE media_assets ADD CONSTRAINT chef_visual_product UNIQUE(product_id,asset_id,revision);
ALTER TABLE character_revisions ADD CONSTRAINT chef_character_product UNIQUE(product_id,character_id,revision);
ALTER TABLE character_revisions ADD CONSTRAINT chef_character_product_avatar FOREIGN KEY(product_id,avatar_id,avatar_revision) REFERENCES media_assets(product_id,asset_id,revision);
