-- Per-link delete capability. A random secret is returned once at creation and
-- stored here; deleting a link via the API requires presenting it, so only its
-- holder can remove the link (names alone are guessable). NULL for links made
-- without one (e.g. the no-JS form), which therefore cannot be deleted via the
-- API -- fail closed.
ALTER TABLE links ADD COLUMN delete_token TEXT;
