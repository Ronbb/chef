-- Validate every existing source while installing constraints; never repair ownership silently.
ALTER TABLE learning_sessions ADD CONSTRAINT chef_session_product_lesson
    FOREIGN KEY(product_id,lesson_id,revision)
    REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE review_cards ADD CONSTRAINT chef_review_product_lesson
    FOREIGN KEY(product_id,source_lesson_id,source_revision)
    REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE saved_items ADD CONSTRAINT chef_saved_product_lesson
    FOREIGN KEY(product_id,source_lesson_id,source_revision)
    REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE lesson_progress ADD CONSTRAINT chef_progress_product_lesson
    FOREIGN KEY(product_id,lesson_id,latest_completed_revision)
    REFERENCES lesson_revisions(product_id,lesson_id,revision);
