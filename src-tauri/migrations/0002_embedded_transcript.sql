ALTER TABLE source_files
    ADD COLUMN contains_embedded_history INTEGER NOT NULL DEFAULT 0
    CHECK (contains_embedded_history IN (0, 1));
