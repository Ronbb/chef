-- Voice ownership and fixed reference edges; global identifiers remain for now.
DO $$
DECLARE item TEXT;
BEGIN
    FOREACH item IN ARRAY ARRAY['character_voice_profiles','voice_reference_grants','voice_reference_revocations','voice_reference_reads'] LOOP
        EXECUTE format('ALTER TABLE %I ADD COLUMN product_id TEXT NOT NULL DEFAULT ''brioche'' CHECK(product_id IN (''brioche'',''hargow''))',item);
        EXECUTE format('CREATE TRIGGER chef_voice_owner BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product()',item);
    END LOOP;
END $$;
ALTER TABLE character_voice_profiles ADD CONSTRAINT chef_voice_product UNIQUE(product_id,character_id,character_revision,revision);
ALTER TABLE character_voice_profiles ADD COLUMN reference_asset_id TEXT GENERATED ALWAYS AS (profile #>> '{referenceAudio,assetId}') STORED;
ALTER TABLE character_voice_profiles ADD COLUMN reference_asset_revision INTEGER GENERATED ALWAYS AS ((profile #>> '{referenceAudio,revision}')::INTEGER) STORED;
ALTER TABLE character_voice_profiles ADD CONSTRAINT chef_voice_reference_pair CHECK((reference_asset_id IS NULL)=(reference_asset_revision IS NULL));
ALTER TABLE character_voice_profiles ADD CONSTRAINT chef_voice_product_character FOREIGN KEY(product_id,character_id,character_revision) REFERENCES character_revisions(product_id,character_id,revision);
ALTER TABLE character_voice_profiles ADD CONSTRAINT chef_voice_product_reference FOREIGN KEY(product_id,reference_asset_id,reference_asset_revision) REFERENCES audio_assets(product_id,asset_id,revision);
ALTER TABLE voice_reference_grants ADD CONSTRAINT chef_reference_grant_product UNIQUE(product_id,id);
ALTER TABLE voice_reference_grants ADD CONSTRAINT chef_reference_grant_product_voice FOREIGN KEY(product_id,character_id,character_revision,voice_revision) REFERENCES character_voice_profiles(product_id,character_id,character_revision,revision);
ALTER TABLE voice_reference_grants ADD CONSTRAINT chef_reference_grant_product_audio FOREIGN KEY(product_id,asset_id,asset_revision) REFERENCES audio_assets(product_id,asset_id,revision);
ALTER TABLE voice_reference_revocations ADD CONSTRAINT chef_reference_revoke_product_grant FOREIGN KEY(product_id,grant_id) REFERENCES voice_reference_grants(product_id,id);
ALTER TABLE voice_reference_reads ADD CONSTRAINT chef_reference_read_product_grant FOREIGN KEY(product_id,grant_id) REFERENCES voice_reference_grants(product_id,id);
