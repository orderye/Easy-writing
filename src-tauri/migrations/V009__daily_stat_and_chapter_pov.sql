-- PR-13（任务书 T11 写作辅助）。
--
-- 三件事：
--   1. daily_stat：按「作品 + 自然日」记录净增字数，支撑状态栏的每日目标进度。
--      净增字数在 save_chapter_content 里按保存前后 word_count 差值累加；
--      导入（commit_import_novel）与批量替换不经过该命令，天然不计入。
--   2. chapter.pov_character_id：本章视角角色（可空），右栏按其筛选章节。
--      角色删除时置空，而不是连同章节一起删（ON DELETE SET NULL）。
--   3. chapter.story_time：章节故事内时间（自由文本，可空），
--      例如「第三年·冬」。不做结构化解析，作者想怎么写就怎么写。
--
-- daily_goal 不建表：它是全局单值偏好，走既有 app_setting 键值表
-- （与 auto_extract / theme 同机制）。

CREATE TABLE IF NOT EXISTS daily_stat (
    id          TEXT PRIMARY KEY,
    novel_id    TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    stat_date   TEXT NOT NULL,
    net_words   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    -- 同一作品同一天只允许一行：累加走 UPSERT，而不是靠查询后插入
    UNIQUE(novel_id, stat_date)
);

CREATE INDEX IF NOT EXISTS idx_daily_stat_novel_date
    ON daily_stat(novel_id, stat_date);

ALTER TABLE chapter ADD COLUMN pov_character_id TEXT REFERENCES character(id) ON DELETE SET NULL;
ALTER TABLE chapter ADD COLUMN story_time TEXT;

CREATE INDEX IF NOT EXISTS idx_chapter_pov ON chapter(novel_id, pov_character_id);
