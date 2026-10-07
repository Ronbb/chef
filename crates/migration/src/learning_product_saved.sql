ALTER TABLE saved_items DROP CONSTRAINT saved_items_user_id_knowledge_id_key;
ALTER TABLE saved_items ADD CONSTRAINT chef_saved_product_knowledge UNIQUE(product_id,user_id,knowledge_id);
CREATE INDEX chef_saved_product_recent ON saved_items(product_id,user_id,created_at DESC,id DESC) WHERE saved;
