-- Valve's schema (GetSchemaItems) carries an image_url per item that was
-- parsed but discarded until now (docs/DESIGN.md Module 15 note).
ALTER TABLE items ADD COLUMN image_url TEXT;
