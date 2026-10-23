-- V007 · 冲突检测「本章忽略此类提示」
--
-- 记录的是「作者对某个章节明确说过：这类冲突不要再提示」。
-- 签名由后端统一计算（<entity_type>:<id> 或 module:<类型>），
-- 前端只负责把后端给的签名原样写回，避免两端各算一套、互相漂移。
--
-- 不记录冲突正文，也不记录模型返回：忽略是作者的决定，
-- 与某一次具体判定无关；同一签名的后续提示都会被放行。

CREATE TABLE chapter_conflict_ignore (
    id         TEXT PRIMARY KEY,
    novel_id   TEXT NOT NULL,
    chapter_id TEXT NOT NULL,
    signature  TEXT NOT NULL,
    label      TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (novel_id, chapter_id, signature)
);

CREATE INDEX idx_conflict_ignore_chapter ON chapter_conflict_ignore (chapter_id, novel_id);