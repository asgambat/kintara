-- Multiple library roots.
--
-- Every installation keeps working unchanged: existing rows all belong to root
-- 0 (KINTARA_LIBRARY_DIR). Extra roots from KINTARA_EXTRA_LIBRARY_DIRS take
-- ids 1..n in configuration order, and identity becomes (root_id,
-- relative_path) instead of relative_path alone, so two roots may hold the
-- same filename without colliding.
--
-- Why it is shaped this way: SQLite rewrites REFERENCES clauses when a table
-- is renamed, and dropping a table implicitly deletes its rows (firing
-- cascades). Renaming `documents` aside in place would therefore either point
-- every dependent at the dropped table or wipe their rows. Instead the new
-- table is built first under a temporary name, the dependents are re-pointed
-- at it explicitly, and only then is the old table dropped — at which point
-- nothing references it anymore. Ids are preserved throughout, so memberships,
-- annotations, reading state, pages, tags, and AI history survive untouched.

DROP TRIGGER documents_fts_insert;
DROP TRIGGER documents_fts_delete;
DROP TRIGGER documents_fts_update;
DROP TRIGGER documents_owner_required_insert;
DROP TRIGGER documents_owner_required_update;

CREATE TABLE documents_new (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id        INTEGER NOT NULL DEFAULT 0,
    title          TEXT    NOT NULL,
    author         TEXT,
    relative_path  TEXT    NOT NULL,
    document_type  TEXT    NOT NULL,
    file_hash      TEXT,
    file_size      INTEGER,
    thumbnail_name TEXT,
    extracted_text TEXT,
    summary        TEXT,
    keywords       TEXT,
    doi            TEXT,
    isbn           TEXT,
    page_count     INTEGER,
    year           INTEGER,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now')),
    modified_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    indexed_at     TEXT,
    text_extracted_at TEXT,
    text_status    TEXT    CHECK (text_status IN ('ok', 'empty', 'truncated', 'failed')),
    owner_id       INTEGER REFERENCES users (id) ON DELETE RESTRICT,
    UNIQUE (root_id, relative_path)
);

INSERT INTO documents_new
    (id, root_id, title, author, relative_path, document_type, file_hash,
     file_size, thumbnail_name, extracted_text, summary, keywords, doi, isbn,
     page_count, year, created_at, modified_at, indexed_at,
     text_extracted_at, text_status, owner_id)
SELECT id, 0, title, author, relative_path, document_type, file_hash,
     file_size, thumbnail_name, extracted_text, summary, keywords, doi, isbn,
     page_count, year, created_at, modified_at, indexed_at,
     text_extracted_at, text_status, owner_id
FROM documents;

CREATE TABLE library_documents_new (
    library_id  INTEGER NOT NULL REFERENCES libraries (id) ON DELETE CASCADE,
    document_id INTEGER NOT NULL REFERENCES documents_new (id) ON DELETE CASCADE,
    PRIMARY KEY (library_id, document_id)
);

CREATE TABLE document_collections_new (
    collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    document_id   INTEGER NOT NULL REFERENCES documents_new (id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, document_id)
);

CREATE TABLE document_tags_new (
    document_id INTEGER NOT NULL REFERENCES documents_new (id) ON DELETE CASCADE,
    tag_id      INTEGER NOT NULL REFERENCES tags (id)      ON DELETE CASCADE,
    PRIMARY KEY (document_id, tag_id)
);

CREATE TABLE annotations_new (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id         INTEGER NOT NULL REFERENCES documents_new (id) ON DELETE CASCADE,
    user_id             INTEGER NOT NULL REFERENCES users (id)     ON DELETE CASCADE,
    annotation_type     TEXT    NOT NULL CHECK (annotation_type IN ('highlight', 'note')),
    serialized_position TEXT    NOT NULL,
    content             TEXT,
    color               TEXT,
    created_at          TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE user_document_state_new (
    user_id          INTEGER NOT NULL REFERENCES users (id)     ON DELETE CASCADE,
    document_id      INTEGER NOT NULL REFERENCES documents_new (id) ON DELETE CASCADE,
    reading_progress REAL    NOT NULL DEFAULT 0,
    is_favorite      INTEGER NOT NULL DEFAULT 0,
    last_opened_at   TEXT,
    PRIMARY KEY (user_id, document_id)
);

CREATE TABLE document_pages_new (
    document_id INTEGER NOT NULL REFERENCES documents_new (id) ON DELETE CASCADE,
    page_number INTEGER NOT NULL CHECK (page_number > 0),
    text        TEXT    NOT NULL,
    PRIMARY KEY (document_id, page_number)
);

CREATE TABLE ai_usage_new (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id       INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    document_id   INTEGER REFERENCES documents_new (id) ON DELETE SET NULL,
    feature       TEXT    NOT NULL,
    provider      TEXT    NOT NULL,
    model         TEXT    NOT NULL,
    input_tokens  INTEGER,
    output_tokens INTEGER,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_conversations_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    document_id INTEGER NOT NULL REFERENCES documents_new (id) ON DELETE CASCADE,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (user_id, document_id)
);

CREATE TABLE ai_messages_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES ai_conversations_new (id) ON DELETE CASCADE,
    role            TEXT    NOT NULL CHECK (role IN ('user', 'assistant')),
    kind            TEXT    NOT NULL CHECK (kind IN ('question', 'summary')),
    content         TEXT    NOT NULL CHECK (length(trim(content)) > 0),
    provider        TEXT    CHECK (provider IN ('openai', 'google')),
    model           TEXT,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ai_message_citations_new (
    message_id  INTEGER NOT NULL REFERENCES ai_messages_new (id) ON DELETE CASCADE,
    page_number INTEGER NOT NULL CHECK (page_number > 0),
    excerpt     TEXT,
    PRIMARY KEY (message_id, page_number)
);

INSERT INTO library_documents_new SELECT * FROM library_documents;
INSERT INTO document_collections_new SELECT * FROM document_collections;
INSERT INTO document_tags_new SELECT * FROM document_tags;
INSERT INTO annotations_new SELECT * FROM annotations;
INSERT INTO user_document_state_new SELECT * FROM user_document_state;
INSERT INTO document_pages_new SELECT * FROM document_pages;
INSERT INTO ai_usage_new SELECT * FROM ai_usage;
INSERT INTO ai_conversations_new SELECT * FROM ai_conversations;
INSERT INTO ai_messages_new SELECT * FROM ai_messages;
INSERT INTO ai_message_citations_new SELECT * FROM ai_message_citations;

DROP TABLE ai_message_citations;
DROP TABLE ai_messages;
DROP TABLE ai_conversations;
DROP TABLE ai_usage;
DROP TABLE document_pages;
DROP TABLE user_document_state;
DROP TABLE annotations;
DROP TABLE document_tags;
DROP TABLE document_collections;
DROP TABLE library_documents;
DROP TABLE documents;

ALTER TABLE documents_new RENAME TO documents;

ALTER TABLE library_documents_new RENAME TO library_documents;
ALTER TABLE document_collections_new RENAME TO document_collections;
ALTER TABLE document_tags_new RENAME TO document_tags;
ALTER TABLE annotations_new RENAME TO annotations;
ALTER TABLE user_document_state_new RENAME TO user_document_state;
ALTER TABLE document_pages_new RENAME TO document_pages;
ALTER TABLE ai_usage_new RENAME TO ai_usage;
ALTER TABLE ai_conversations_new RENAME TO ai_conversations;
ALTER TABLE ai_messages_new RENAME TO ai_messages;
ALTER TABLE ai_message_citations_new RENAME TO ai_message_citations;

CREATE INDEX idx_documents_modified_at ON documents (modified_at DESC);
CREATE INDEX idx_documents_file_hash   ON documents (file_hash);
CREATE INDEX idx_documents_owner ON documents (owner_id);
CREATE INDEX idx_documents_root ON documents (root_id, relative_path);
CREATE INDEX idx_annotations_doc_user ON annotations (document_id, user_id);
CREATE INDEX idx_uds_user_favorite ON user_document_state (user_id, is_favorite);
CREATE INDEX idx_document_pages_document ON document_pages (document_id, page_number);
CREATE INDEX idx_ai_usage_user_created ON ai_usage (user_id, created_at DESC);
CREATE INDEX idx_ai_conversations_user_document
    ON ai_conversations (user_id, document_id);
CREATE INDEX idx_ai_messages_conversation_created
    ON ai_messages (conversation_id, id);

CREATE TRIGGER documents_fts_insert AFTER INSERT ON documents BEGIN
    INSERT INTO documents_fts (rowid, title, author, keywords, summary, extracted_text)
    VALUES (new.id, new.title, new.author, new.keywords, new.summary, new.extracted_text);
END;

CREATE TRIGGER documents_fts_delete AFTER DELETE ON documents BEGIN
    INSERT INTO documents_fts (documents_fts, rowid, title, author, keywords, summary, extracted_text)
    VALUES ('delete', old.id, old.title, old.author, old.keywords, old.summary, old.extracted_text);
END;

CREATE TRIGGER documents_fts_update AFTER UPDATE ON documents BEGIN
    INSERT INTO documents_fts (documents_fts, rowid, title, author, keywords, summary, extracted_text)
    VALUES ('delete', old.id, old.title, old.author, old.keywords, old.summary, old.extracted_text);
    INSERT INTO documents_fts (rowid, title, author, keywords, summary, extracted_text)
    VALUES (new.id, new.title, new.author, new.keywords, new.summary, new.extracted_text);
END;

CREATE TRIGGER documents_owner_required_insert
BEFORE INSERT ON documents
WHEN NEW.owner_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'document owner is required');
END;

CREATE TRIGGER documents_owner_required_update
BEFORE UPDATE OF owner_id ON documents
WHEN NEW.owner_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'document owner is required');
END;

-- The FTS index is not carried over by the rebuild (its triggers were down
-- while rows were copied), so rebuild it from the new table.
INSERT INTO documents_fts(documents_fts) VALUES('rebuild');
