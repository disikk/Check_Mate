CREATE SEQUENCE IF NOT EXISTS import.ingest_bundles_queue_order_seq;

ALTER TABLE import.ingest_bundles
    ADD COLUMN IF NOT EXISTS queue_order BIGINT;

ALTER TABLE import.ingest_bundles
    ALTER COLUMN queue_order SET DEFAULT nextval('import.ingest_bundles_queue_order_seq');

WITH ordered_bundles AS (
    SELECT
        id,
        ROW_NUMBER() OVER (ORDER BY created_at, id) AS next_queue_order
    FROM import.ingest_bundles
    WHERE queue_order IS NULL
)
UPDATE import.ingest_bundles AS bundles
SET queue_order = ordered_bundles.next_queue_order
FROM ordered_bundles
WHERE bundles.id = ordered_bundles.id;

DO $$
DECLARE
    max_queue_order BIGINT;
BEGIN
    SELECT MAX(queue_order)
    INTO max_queue_order
    FROM import.ingest_bundles;

    IF max_queue_order IS NULL THEN
        PERFORM setval('import.ingest_bundles_queue_order_seq', 1, false);
    ELSE
        PERFORM setval('import.ingest_bundles_queue_order_seq', max_queue_order, true);
    END IF;
END $$;

ALTER TABLE import.ingest_bundles
    ALTER COLUMN queue_order SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_ingest_bundles_queue_order
    ON import.ingest_bundles(queue_order);
