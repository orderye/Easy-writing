-- V003 · 补齐性能关键索引与历史修剪索引

-- 图片资源检索与去重索引
CREATE INDEX IF NOT EXISTS idx_asset_novel_hash ON asset(novel_id, hash);
CREATE INDEX IF NOT EXISTS idx_asset_hash ON asset(hash);

-- 情节大纲按章节关联检索索引
CREATE INDEX IF NOT EXISTS idx_plot_item_chapter ON plot_item(chapter_id);

-- 设定变更审计日志按对象检索索引
CREATE INDEX IF NOT EXISTS idx_setting_change_target ON setting_change(target_type, target_id);

-- 章节历史版本自动保存修剪与检索索引
CREATE INDEX IF NOT EXISTS idx_chapter_revision_auto ON chapter_revision(chapter_id, revision_type, created_at DESC);
