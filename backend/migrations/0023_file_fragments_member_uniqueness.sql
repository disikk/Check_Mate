ALTER TABLE import.file_fragments
    DROP CONSTRAINT IF EXISTS file_fragments_source_file_id_fragment_index_key;

CREATE UNIQUE INDEX IF NOT EXISTS uniq_file_fragments_member_fragment
    ON import.file_fragments(source_file_member_id, fragment_index);
