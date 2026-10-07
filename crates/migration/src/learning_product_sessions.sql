DROP INDEX learning_one_active;
CREATE UNIQUE INDEX learning_one_active ON learning_sessions(product_id,user_id,lesson_id) WHERE completed_at IS NULL;
ALTER TABLE lesson_progress DROP CONSTRAINT lesson_progress_pkey;
ALTER TABLE lesson_progress ADD PRIMARY KEY(product_id,user_id,lesson_id);
CREATE INDEX chef_learning_product_recent ON learning_sessions(product_id,user_id,updated_at DESC,id DESC);
