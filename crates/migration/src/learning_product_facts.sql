-- Preparation only: legacy reads/unique keys remain Brioche-only until query and content scoping.
ALTER TABLE learning_sessions ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE lesson_progress ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE step_progress ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE exercise_hints ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE exercise_attempts ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE learning_operations ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE review_cards ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE review_attempts ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));
ALTER TABLE saved_items ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche' CHECK(product_id IN ('brioche','hargow'));

ALTER TABLE learning_sessions ADD CONSTRAINT chef_session_product UNIQUE(product_id,id);
ALTER TABLE learning_sessions ADD CONSTRAINT chef_session_product_owner UNIQUE(product_id,id,user_id);
ALTER TABLE learning_sessions ADD CONSTRAINT chef_session_product_source UNIQUE(product_id,id,user_id,lesson_id);
ALTER TABLE review_cards ADD CONSTRAINT chef_review_product_owner UNIQUE(product_id,id,user_id);
ALTER TABLE lesson_progress ADD CONSTRAINT chef_progress_product_session FOREIGN KEY(product_id,last_session_id,user_id,lesson_id) REFERENCES learning_sessions(product_id,id,user_id,lesson_id);
ALTER TABLE step_progress ADD CONSTRAINT chef_step_product_session FOREIGN KEY(product_id,session_id) REFERENCES learning_sessions(product_id,id);
ALTER TABLE exercise_hints ADD CONSTRAINT chef_hint_product_session FOREIGN KEY(product_id,session_id) REFERENCES learning_sessions(product_id,id);
ALTER TABLE exercise_attempts ADD CONSTRAINT chef_attempt_product_session FOREIGN KEY(product_id,session_id,user_id) REFERENCES learning_sessions(product_id,id,user_id);
ALTER TABLE review_attempts ADD CONSTRAINT chef_attempt_product_card FOREIGN KEY(product_id,card_id,user_id) REFERENCES review_cards(product_id,id,user_id);

CREATE FUNCTION chef_protect_learning_product() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.product_id IS DISTINCT FROM OLD.product_id THEN
        RAISE EXCEPTION 'learning facts cannot change product';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON learning_sessions FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON lesson_progress FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON step_progress FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON exercise_hints FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON exercise_attempts FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON learning_operations FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON review_cards FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON review_attempts FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
CREATE TRIGGER chef_product_owner BEFORE UPDATE ON saved_items FOR EACH ROW EXECUTE FUNCTION chef_protect_learning_product();
