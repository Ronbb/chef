ALTER TABLE review_cards DROP CONSTRAINT review_cards_user_id_knowledge_id_key;
ALTER TABLE review_cards ADD CONSTRAINT chef_review_product_knowledge UNIQUE(product_id,user_id,knowledge_id);
CREATE INDEX chef_review_product_due ON review_cards(product_id,user_id,due_at,id) WHERE NOT suspended;
CREATE INDEX chef_review_product_recent ON review_cards(product_id,user_id,created_at DESC,id DESC);
CREATE INDEX chef_review_attempt_product_recent ON review_attempts(product_id,user_id,reviewed_at DESC,id DESC);
