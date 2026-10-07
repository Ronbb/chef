ALTER TABLE learning_operations DROP CONSTRAINT learning_operations_pkey;
ALTER TABLE learning_operations ADD PRIMARY KEY(product_id,user_id,scope,idempotency_key);
