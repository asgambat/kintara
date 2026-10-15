-- Reverse indexes for the access filters.
--
-- The shared-document filters in `routes/documents/mod.rs` enter the join
-- tables from the document side: the library access check probes
-- `access_ld.document_id = d.id` once per candidate row, and the tag filter
-- looks documents up by `tag_id`. Both need the reverse of the tables'
-- primary keys, which order (owner-side, document-side) and never serve
-- these lookups: without the indexes SQLite scans the whole join table per
-- filtered query, on every page of every listing.
--
-- `document_collections` is deliberately not indexed: its filters start
-- from `collection_id`, which the (collection_id, document_id) primary key
-- already serves with a prefix lookup.

CREATE INDEX idx_library_documents_document ON library_documents (document_id);
CREATE INDEX idx_document_tags_tag ON document_tags (tag_id);
