ALTER TABLE delivery_states
ADD COLUMN completion_token BLOB CHECK(
    completion_token IS NULL OR length(completion_token) = 32
);
