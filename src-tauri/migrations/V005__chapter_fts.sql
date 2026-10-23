-- PR-09（任务书 T09 全书搜索与替换）。
--
-- 本迁移做两件事，都是 T09 的硬依赖：
--
-- 1. `chapter_revision.batch_id` —— 批量替换的「整批撤销」钥匙。
--    同一轮替换为每章写的快照共享一个 id，撤销时按它一次找回全部。
--    没有这一列，「撤销本次替换」只能按时间猜，跨章撤销必然误伤。
--
-- 2. `chapter_fts` —— 全书搜索的索引。
--    trigram 分词器按三字符滑窗建索引，中文无需分词即可命中；
--    代价是 2 字查询不在索引里，必须回退 LIKE（见 db/search.rs 的说明）。
--
-- 为什么用 standalone 表（不带 content= 外部内容表）：
-- 外部内容表要求索引与 chapter 的 rowid 严格对应，而 chapter 是软删
-- （deleted_at），软删后 rowid 仍存在、索引也必须跟着消失 ——
-- standalone 表用 chapter_id 整行删/插，语义最直白，也不会因为
-- VACUUM 或 rowid 重用产生幽灵命中。
--
-- 本文件走 `execute_batch`（db/mod.rs），因此触发器里的 BEGIN…END
-- 分号不会像 V002 那样被朴素切分破坏。

-- 1. 批次撤销列 ------------------------------------------------------------
ALTER TABLE chapter_revision ADD COLUMN batch_id TEXT;
CREATE INDEX IF NOT EXISTS idx_revision_batch ON chapter_revision(batch_id);

-- 2. 全文索引 --------------------------------------------------------------
CREATE VIRTUAL TABLE IF NOT EXISTS chapter_fts USING fts5(
    chapter_id    UNINDEXED,
    novel_id      UNINDEXED,
    title,
    content_plain,
    tokenize = 'trigram'
);

-- 3. 同步触发器 ------------------------------------------------------------
-- 只索引未删除的章节；软删时把整行从索引里移除。
CREATE TRIGGER IF NOT EXISTS chapter_fts_ai AFTER INSERT ON chapter
WHEN new.deleted_at IS NULL
BEGIN
    INSERT INTO chapter_fts (chapter_id, novel_id, title, content_plain)
    VALUES (new.id, new.novel_id, new.title, new.content_plain);
END;

CREATE TRIGGER IF NOT EXISTS chapter_fts_au AFTER UPDATE ON chapter
BEGIN
    DELETE FROM chapter_fts WHERE chapter_id = old.id;
    INSERT INTO chapter_fts (chapter_id, novel_id, title, content_plain)
    SELECT new.id, new.novel_id, new.title, new.content_plain
    WHERE new.deleted_at IS NULL;
END;

CREATE TRIGGER IF NOT EXISTS chapter_fts_ad AFTER DELETE ON chapter
BEGIN
    DELETE FROM chapter_fts WHERE chapter_id = old.id;
END;

-- 4. 回填存量 --------------------------------------------------------------
INSERT INTO chapter_fts (chapter_id, novel_id, title, content_plain)
SELECT id, novel_id, title, content_plain
FROM chapter
WHERE deleted_at IS NULL;
