-- PR-12（任务书 T10 伏笔看板）。
--
-- plot_item 的回收章节：此前「已回收」只是一个状态位，
-- 没有记录在哪一回收的，导致伏笔看板无法算「距今章数」、
-- 也无一键跳回收章节。
--
-- 与埋设章节（chapter_id）分开存储：埋设章由提炼或人工编辑确定，
-- 回收章由作者在看板上选定。两者语义不同，不能共用一个列。
ALTER TABLE plot_item ADD COLUMN resolved_chapter_id TEXT REFERENCES chapter(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_plot_resolved_chapter
    ON plot_item(novel_id, resolved_chapter_id);