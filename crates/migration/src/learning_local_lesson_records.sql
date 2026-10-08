-- Forward-only owner maintenance. No CASCADE: unknown child dependencies fail atomically.
ALTER TABLE content_withdrawals DROP CONSTRAINT content_withdrawals_pkey;
ALTER TABLE content_withdrawals ADD CONSTRAINT chef_local_withdrawal_primary PRIMARY KEY(product_id,lesson_id,revision);
ALTER TABLE lesson_import_audit DROP CONSTRAINT lesson_import_audit_pkey;
ALTER TABLE lesson_import_audit ADD CONSTRAINT chef_local_import_primary PRIMARY KEY(product_id,lesson_id,revision);
ALTER TABLE editorial_reviews DROP CONSTRAINT editorial_reviews_pkey;
ALTER TABLE editorial_reviews ADD CONSTRAINT chef_local_editorial_primary PRIMARY KEY(product_id,lesson_id,revision,version);
ALTER TABLE lesson_audio_reviews DROP CONSTRAINT lesson_audio_reviews_pkey;
ALTER TABLE lesson_audio_reviews ADD CONSTRAINT chef_local_audio_review_primary PRIMARY KEY(product_id,lesson_id,revision,version);
ALTER TABLE lesson_direct_publications DROP CONSTRAINT lesson_direct_publications_pkey;
ALTER TABLE lesson_direct_publications ADD CONSTRAINT chef_local_publication_primary PRIMARY KEY(product_id,lesson_id,revision);
-- Withdrawal in another product must not affect this product's immutable snapshot.
CREATE OR REPLACE FUNCTION protect_lesson_snapshot() RETURNS trigger LANGUAGE plpgsql AS $chef$
BEGIN
    IF NEW.lesson_id IS DISTINCT FROM OLD.lesson_id OR NEW.revision IS DISTINCT FROM OLD.revision
        OR NEW.public_document IS DISTINCT FROM OLD.public_document
        OR NEW.server_document IS DISTINCT FROM OLD.server_document THEN
        RAISE EXCEPTION 'immutable lesson snapshot';
    END IF;
    IF NEW.published AND EXISTS(SELECT 1 FROM content_withdrawals w
        WHERE w.product_id=NEW.product_id AND w.lesson_id=NEW.lesson_id AND w.revision=NEW.revision) THEN
        RAISE EXCEPTION 'withdrawn revision cannot be restored';
    END IF;
    RETURN NEW;
END
$chef$;
