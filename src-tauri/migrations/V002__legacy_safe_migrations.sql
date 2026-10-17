-- V002 · 引入版本表之前那些「每次启动无条件执行」的安全迁移。
--
-- 历史背景：这些语句原先散落在 db/mod.rs 的 init_db 里，且全部用 `let _ =` 吞掉错误。
-- 任何在这个文件出现之前跑过的数据库，都已经生效过它们；重复执行会报
-- 「duplicate column name」—— 这是预期内的，由 Rust 侧显式白名单容忍。
--
-- 因此本文件有两条硬约束：
--   1. 每条语句必须自身幂等，或仅依赖「duplicate column」这一种可容忍错误；
--   2. **语句内不得出现字符串字面量里的分号** —— Rust 侧是按 ';' 朴素切分的。
--      新增语句请保持一行一条、以分号结尾。

ALTER TABLE setting_change ADD COLUMN target_title TEXT NOT NULL DEFAULT '';
ALTER TABLE setting_change ADD COLUMN is_reverted INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS character_relation (
    id              TEXT PRIMARY KEY,
    novel_id        TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    from_char_id    TEXT NOT NULL REFERENCES character(id) ON DELETE CASCADE,
    to_char_id      TEXT NOT NULL REFERENCES character(id) ON DELETE CASCADE,
    relation_name   TEXT NOT NULL DEFAULT '',
    description     TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_relation_novel ON character_relation(novel_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_prompt_template_scope ON prompt_template(scope);

ALTER TABLE chat_session ADD COLUMN title TEXT NOT NULL DEFAULT '对话';
CREATE INDEX IF NOT EXISTS idx_chat_session_novel ON chat_session(novel_id, chapter_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS novel_lexicon (
    id          TEXT PRIMARY KEY,
    novel_id    TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    term        TEXT NOT NULL,
    category    TEXT NOT NULL DEFAULT 'custom',
    frequency   INTEGER NOT NULL DEFAULT 1,
    source_id   TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_lexicon_novel_term ON novel_lexicon(novel_id, term);
CREATE INDEX IF NOT EXISTS idx_lexicon_novel ON novel_lexicon(novel_id);

ALTER TABLE asset ADD COLUMN deleted_at TEXT;
ALTER TABLE extraction_candidate ADD COLUMN title TEXT NOT NULL DEFAULT '';
