-- Product ownership for cloning/audition history, retaining immutable snapshots.
DO $$
DECLARE item TEXT;
BEGIN
    FOREACH item IN ARRAY ARRAY['voice_clone_jobs','voice_clone_events','voice_auditions','voice_audition_events','voice_audition_reviews'] LOOP
        EXECUTE format('ALTER TABLE %I ADD COLUMN product_id TEXT NOT NULL DEFAULT ''brioche'' CHECK(product_id IN (''brioche'',''hargow''))',item);
        EXECUTE format('CREATE TRIGGER chef_voice_work_owner BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product()',item);
    END LOOP;
END $$;
ALTER TABLE voice_clone_jobs ADD CONSTRAINT chef_clone_product UNIQUE(product_id,id);
ALTER TABLE voice_clone_events ADD CONSTRAINT chef_clone_event_product UNIQUE(product_id,job_id,version);
ALTER TABLE voice_auditions ADD CONSTRAINT chef_audition_product UNIQUE(product_id,id);
ALTER TABLE voice_clone_jobs ADD CONSTRAINT chef_clone_product_grant FOREIGN KEY(product_id,grant_id) REFERENCES voice_reference_grants(product_id,id);
ALTER TABLE voice_clone_events ADD CONSTRAINT chef_clone_event_product_job FOREIGN KEY(product_id,job_id) REFERENCES voice_clone_jobs(product_id,id);
ALTER TABLE voice_auditions ADD CONSTRAINT chef_audition_product_clone FOREIGN KEY(product_id,clone_job_id,clone_version) REFERENCES voice_clone_events(product_id,job_id,version);
ALTER TABLE voice_auditions ADD CONSTRAINT chef_audition_product_character FOREIGN KEY(product_id,character_id,character_revision) REFERENCES character_revisions(product_id,character_id,revision);
-- A base version of zero uses a provisional system direction, not a stored profile.
ALTER TABLE voice_auditions ADD COLUMN base_profile_revision INTEGER GENERATED ALWAYS AS (NULLIF(base_voice_revision,0)) STORED;
ALTER TABLE voice_auditions ADD CONSTRAINT chef_audition_product_base_voice FOREIGN KEY(product_id,character_id,character_revision,base_profile_revision) REFERENCES character_voice_profiles(product_id,character_id,character_revision,revision);
ALTER TABLE voice_auditions ADD COLUMN reference_asset_id TEXT GENERATED ALWAYS AS (profile #>> '{referenceAudio,assetId}') STORED;
ALTER TABLE voice_auditions ADD COLUMN reference_asset_revision INTEGER GENERATED ALWAYS AS ((profile #>> '{referenceAudio,revision}')::INTEGER) STORED;
ALTER TABLE voice_auditions ADD CONSTRAINT chef_audition_reference_pair CHECK((reference_asset_id IS NULL)=(reference_asset_revision IS NULL));
ALTER TABLE voice_auditions ADD CONSTRAINT chef_audition_product_reference FOREIGN KEY(product_id,reference_asset_id,reference_asset_revision) REFERENCES audio_assets(product_id,asset_id,revision);
ALTER TABLE voice_audition_events ADD CONSTRAINT chef_audition_event_product_audition FOREIGN KEY(product_id,audition_id) REFERENCES voice_auditions(product_id,id);
ALTER TABLE voice_audition_reviews ADD CONSTRAINT chef_audition_review_product_audition FOREIGN KEY(product_id,audition_id) REFERENCES voice_auditions(product_id,id);
ALTER TABLE voice_audition_reviews ADD CONSTRAINT chef_audition_review_product_character FOREIGN KEY(product_id,character_id,character_revision) REFERENCES character_revisions(product_id,character_id,revision);
ALTER TABLE voice_audition_reviews ADD CONSTRAINT chef_audition_review_product_voice FOREIGN KEY(product_id,character_id,character_revision,voice_revision) REFERENCES character_voice_profiles(product_id,character_id,character_revision,revision);
