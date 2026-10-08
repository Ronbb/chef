-- Product ownership and fixed parent graph for course speech artifacts.
-- Legacy global keys remain until all runtime consumers use product scope.
DO $$
DECLARE item TEXT;
BEGIN
 FOREACH item IN ARRAY ARRAY['course_speech_plans','course_speech_clips','course_speech_clip_events','course_speech_clip_reviews','speech_alignments','speech_alignment_reviews','speech_package_imports'] LOOP
  EXECUTE format('ALTER TABLE %I ADD COLUMN product_id TEXT NOT NULL DEFAULT ''brioche'' CHECK(product_id IN (''brioche'',''hargow''))',item);
  EXECUTE format('CREATE TRIGGER chef_speech_work_owner BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product()',item);
 END LOOP;
END $$;
ALTER TABLE course_speech_plans ADD CONSTRAINT chef_speech_plan_product UNIQUE(product_id,id);
ALTER TABLE course_speech_clips ADD CONSTRAINT chef_speech_clip_product UNIQUE(product_id,id);
ALTER TABLE speech_alignments ADD CONSTRAINT chef_alignment_product UNIQUE(product_id,id);
ALTER TABLE course_speech_plans ADD CONSTRAINT chef_speech_plan_product_lesson FOREIGN KEY(product_id,lesson_id,lesson_revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
ALTER TABLE course_speech_clips ADD CONSTRAINT chef_speech_clip_product_plan FOREIGN KEY(product_id,plan_id) REFERENCES course_speech_plans(product_id,id);
ALTER TABLE course_speech_clips ADD CONSTRAINT chef_speech_clip_product_reuse FOREIGN KEY(product_id,reused_from) REFERENCES course_speech_clips(product_id,id);
ALTER TABLE course_speech_clip_events ADD CONSTRAINT chef_speech_event_product_clip FOREIGN KEY(product_id,clip_id) REFERENCES course_speech_clips(product_id,id);
ALTER TABLE course_speech_clip_reviews ADD CONSTRAINT chef_speech_review_product_clip FOREIGN KEY(product_id,clip_id) REFERENCES course_speech_clips(product_id,id);
ALTER TABLE speech_alignments ADD CONSTRAINT chef_alignment_product_plan FOREIGN KEY(product_id,plan_id) REFERENCES course_speech_plans(product_id,id);
ALTER TABLE speech_alignment_reviews ADD CONSTRAINT chef_alignment_review_product_alignment FOREIGN KEY(product_id,alignment_id) REFERENCES speech_alignments(product_id,id);
ALTER TABLE speech_alignment_reviews ADD CONSTRAINT chef_alignment_review_product_clip FOREIGN KEY(product_id,clip_id) REFERENCES course_speech_clips(product_id,id);
ALTER TABLE speech_package_imports ADD CONSTRAINT chef_package_product_alignment FOREIGN KEY(product_id,alignment_id) REFERENCES speech_alignments(product_id,id);
ALTER TABLE speech_package_imports ADD CONSTRAINT chef_package_product_lesson FOREIGN KEY(product_id,lesson_id,revision) REFERENCES lesson_revisions(product_id,lesson_id,revision);
