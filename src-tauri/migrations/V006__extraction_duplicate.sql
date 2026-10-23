-- PR-10（任务书 T07 提炼成本与去重）。
--
-- 提炼候选的「疑似重复」锚点：生成候选时，若标题（或其别名）与既有角色
-- 或候选池中的另一个角色高度相似，把对方 id 记在这里，由审核面板提示
-- 「疑似同一人物，是否合并」。
--
-- 为什么把锚点放在候选上而不是在展示时现算：
--   1. 判定依赖生成当时的设定库快照 —— 之后作者可能改名、合并、删除，
--      现算出来的结论会漂移，而候选本身的内容没有变；
--   2. 审核面板需要在**入库前**就把提示摆出来，否则作者已经点了「采纳」，
--      再提示合并就晚了（此时库里已经多了一条重复角色）。
--
-- 值的语义：
--   * 命中既有角色 → 角色 id（`character.id`）；
--   * 命中候选池中的另一条待审候选 → 候选 id（`extraction_candidate.id`）。
--   前端按「先查角色、再查候选」的顺序解析展示文案。
ALTER TABLE extraction_candidate ADD COLUMN possible_duplicate_of TEXT;

CREATE INDEX IF NOT EXISTS idx_extraction_duplicate
    ON extraction_candidate(novel_id, possible_duplicate_of);