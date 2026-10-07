-- Ownership preparation. Global IDs and singleton remain until scoped runtime migration.
DO $$
DECLARE item TEXT;
BEGIN
    FOREACH item IN ARRAY ARRAY['lesson_revisions','content_releases','release_entries','content_state','content_withdrawals','content_audit','lesson_import_audit','editorial_reviews','lesson_audio_reviews','lesson_direct_publications'] LOOP
        EXECUTE format('ALTER TABLE %I ADD COLUMN product_id TEXT NOT NULL DEFAULT ''brioche'' CHECK(product_id IN (''brioche'',''hargow''))',item);
        EXECUTE format('CREATE TRIGGER chef_content_owner BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product()',item);
    END LOOP;
END $$;
ALTER TABLE lesson_revisions ADD CONSTRAINT chef_lesson_product UNIQUE(product_id,lesson_id,revision);
ALTER TABLE content_releases ADD CONSTRAINT chef_release_product UNIQUE(product_id,id);
ALTER TABLE release_entries ADD CONSTRAINT chef_entry_product_release FOREIGN KEY(product_id,release_id) REFERENCES content_releases(product_id,id);
ALTER TABLE release_entries ADD CONSTRAINT chef_entry_product_lesson FOREIGN KEY(product_id,lesson_id,revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE content_state ADD CONSTRAINT chef_state_product_release FOREIGN KEY(product_id,active_release) REFERENCES content_releases(product_id,id);
ALTER TABLE content_withdrawals ADD CONSTRAINT chef_withdrawal_product_lesson FOREIGN KEY(product_id,lesson_id,revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE content_audit ADD CONSTRAINT chef_audit_product_release FOREIGN KEY(product_id,release_id) REFERENCES content_releases(product_id,id);
ALTER TABLE lesson_import_audit ADD CONSTRAINT chef_import_product_lesson FOREIGN KEY(product_id,lesson_id,revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE editorial_reviews ADD CONSTRAINT chef_editorial_product_lesson FOREIGN KEY(product_id,lesson_id,revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE lesson_audio_reviews ADD CONSTRAINT chef_audio_review_product_lesson FOREIGN KEY(product_id,lesson_id,revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE lesson_direct_publications ADD CONSTRAINT chef_publication_product_lesson FOREIGN KEY(product_id,lesson_id,revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
