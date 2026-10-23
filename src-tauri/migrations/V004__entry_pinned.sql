-- PR-08（T06 上下文装配透明度）：为设定三表提供「硬性」标记。
-- 被标记为 pinned 的设定在上下文装配中始终带入，前端可强制加入某条。
-- 用 INTEGER(0/1) 而非 BOOLEAN，与库内既有开关列（如 is_default）一致。
ALTER TABLE worldview_entry ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE character ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE plot_item ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
