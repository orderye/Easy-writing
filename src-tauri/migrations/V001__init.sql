-- 1. 大模型接口配置
CREATE TABLE IF NOT EXISTS llm_profile (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    protocol    TEXT NOT NULL CHECK(protocol IN ('openai', 'anthropic')),
    base_url    TEXT NOT NULL,
    key_ref     TEXT NOT NULL DEFAULT '',
    model       TEXT NOT NULL DEFAULT '',
    params      TEXT NOT NULL DEFAULT '{}',
    is_default  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 2. 作品表
CREATE TABLE IF NOT EXISTS novel (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL DEFAULT '未命名作品',
    synopsis    TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at  TEXT
);

-- 3. 卷目表
CREATE TABLE IF NOT EXISTS volume (
    id          TEXT PRIMARY KEY,
    novel_id    TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT '新卷',
    summary     TEXT NOT NULL DEFAULT '',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_volume_novel ON volume(novel_id, sort_order);

-- 4. 章节表
CREATE TABLE IF NOT EXISTS chapter (
    id              TEXT PRIMARY KEY,
    novel_id        TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    volume_id       TEXT NOT NULL REFERENCES volume(id) ON DELETE CASCADE,
    title           TEXT NOT NULL DEFAULT '新章节',
    summary         TEXT NOT NULL DEFAULT '',
    content         TEXT NOT NULL DEFAULT '{}',
    content_plain   TEXT NOT NULL DEFAULT '',
    word_count      INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft', 'editing', 'finalized')),
    extracted_hash  TEXT,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at      TEXT
);
CREATE INDEX IF NOT EXISTS idx_chapter_volume ON chapter(volume_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_chapter_novel  ON chapter(novel_id);
 
 -- 4.1 章节历史版本
 CREATE TABLE IF NOT EXISTS chapter_revision (
     id              TEXT PRIMARY KEY,
     chapter_id      TEXT NOT NULL REFERENCES chapter(id) ON DELETE CASCADE,
     novel_id        TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
     title           TEXT NOT NULL,
     content         TEXT NOT NULL DEFAULT '{}',
     content_plain   TEXT NOT NULL DEFAULT '',
     word_count      INTEGER NOT NULL DEFAULT 0,
     revision_type   TEXT NOT NULL DEFAULT 'auto' CHECK(revision_type IN ('auto', 'manual', 'ai_before', 'ai_after', 'rollback_before')),
     note            TEXT NOT NULL DEFAULT '',
     created_at      TEXT NOT NULL DEFAULT (datetime('now'))
 );
 CREATE INDEX IF NOT EXISTS idx_revision_chapter ON chapter_revision(chapter_id, created_at DESC);
 CREATE INDEX IF NOT EXISTS idx_revision_novel   ON chapter_revision(novel_id);


-- 5. 本地图片资产
CREATE TABLE IF NOT EXISTS asset (
    id          TEXT PRIMARY KEY,
    novel_id    TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    mime        TEXT NOT NULL,
    rel_path    TEXT NOT NULL,
    size        INTEGER NOT NULL DEFAULT 0,
    hash        TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    -- 软删除时间；NULL 表示在库。与作品/卷/章一致：移除可恢复。
    deleted_at  TEXT
);

-- 6. 世界观设定
CREATE TABLE IF NOT EXISTS worldview_entry (
    id                  TEXT PRIMARY KEY,
    novel_id            TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    group_name          TEXT NOT NULL DEFAULT '',
    title               TEXT NOT NULL,
    content             TEXT NOT NULL DEFAULT '',
    origin              TEXT NOT NULL DEFAULT 'manual' CHECK(origin IN ('manual', 'ai')),
    source_chapter_id   TEXT REFERENCES chapter(id) ON DELETE SET NULL,
    sort_order          INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at          TEXT
);
CREATE INDEX IF NOT EXISTS idx_worldview_novel ON worldview_entry(novel_id, sort_order);

-- 7. 角色设定
CREATE TABLE IF NOT EXISTS character (
    id                  TEXT PRIMARY KEY,
    novel_id            TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    aliases             TEXT NOT NULL DEFAULT '[]',
    profile             TEXT NOT NULL DEFAULT '{}',
    origin              TEXT NOT NULL DEFAULT 'manual' CHECK(origin IN ('manual', 'ai')),
    source_chapter_id   TEXT REFERENCES chapter(id) ON DELETE SET NULL,
    sort_order          INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at          TEXT
);
CREATE INDEX IF NOT EXISTS idx_character_novel ON character(novel_id, sort_order);

-- 8. 剧情与伏笔
CREATE TABLE IF NOT EXISTS plot_item (
    id                  TEXT PRIMARY KEY,
    novel_id            TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    title               TEXT NOT NULL,
    content             TEXT NOT NULL DEFAULT '',
    kind                TEXT NOT NULL DEFAULT 'mainline' CHECK(kind IN ('mainline', 'conflict', 'foreshadow')),
    status              TEXT NOT NULL DEFAULT 'planted' CHECK(status IN ('planted', 'resolved')),
    chapter_id          TEXT REFERENCES chapter(id) ON DELETE SET NULL,
    origin              TEXT NOT NULL DEFAULT 'manual' CHECK(origin IN ('manual', 'ai')),
    sort_order          INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at          TEXT
);
CREATE INDEX IF NOT EXISTS idx_plot_novel ON plot_item(novel_id, sort_order);

-- 9. 预设 Agent 表
CREATE TABLE IF NOT EXISTS agent (
    id              TEXT PRIMARY KEY,
    key             TEXT NOT NULL UNIQUE CHECK(key IN ('worldview', 'character', 'plot', 'outline', 'volume_chapter', 'writing_assistant')),
    name            TEXT NOT NULL,
    system_prompt   TEXT NOT NULL DEFAULT '',
    llm_profile_id  TEXT REFERENCES llm_profile(id) ON DELETE SET NULL,
    params          TEXT NOT NULL DEFAULT '{}',
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 10. 对话记录
CREATE TABLE IF NOT EXISTS chat_session (
    id          TEXT PRIMARY KEY,
    novel_id    TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    chapter_id  TEXT REFERENCES chapter(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT '对话',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_chat_session_novel ON chat_session(novel_id, chapter_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS chat_message (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES chat_session(id) ON DELETE CASCADE,
    role        TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
    content     TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_msg_session ON chat_message(session_id, created_at);

-- 11. 提炼候选池
CREATE TABLE IF NOT EXISTS extraction_candidate (
    id                  TEXT PRIMARY KEY,
    novel_id            TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    chapter_id          TEXT NOT NULL REFERENCES chapter(id) ON DELETE CASCADE,
    agent_id            TEXT NOT NULL REFERENCES agent(id),
    kind                TEXT NOT NULL CHECK(kind IN ('worldview', 'character', 'plot', 'chapter_summary')),
    action              TEXT NOT NULL CHECK(action IN ('create', 'update')),
    target_id           TEXT,
    title               TEXT NOT NULL DEFAULT '',
    payload             TEXT NOT NULL DEFAULT '{}',
    evidence            TEXT NOT NULL DEFAULT '',
    dedup_key           TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'accepted', 'ignored')),
    created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_extraction_novel ON extraction_candidate(novel_id, status);
CREATE INDEX IF NOT EXISTS idx_extraction_dedup ON extraction_candidate(novel_id, dedup_key);

-- 12. 设定修改留档
CREATE TABLE IF NOT EXISTS setting_change (
    id              TEXT PRIMARY KEY,
    novel_id        TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
    chapter_id      TEXT REFERENCES chapter(id) ON DELETE SET NULL,
    target_type     TEXT NOT NULL CHECK(target_type IN ('worldview', 'character', 'plot', 'outline', 'volume', 'chapter')),
    target_id       TEXT NOT NULL,
    target_title    TEXT NOT NULL DEFAULT '',
    before_content  TEXT NOT NULL DEFAULT '',
    after_content   TEXT NOT NULL DEFAULT '',
    reason          TEXT NOT NULL DEFAULT '',
    is_reverted     INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_setting_change_novel ON setting_change(novel_id, created_at DESC);


-- 13. 提示词模板
CREATE TABLE IF NOT EXISTS prompt_template (
    id          TEXT PRIMARY KEY,
    scope       TEXT NOT NULL,
    content     TEXT NOT NULL,
    is_default  INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 14. WebDAV 配置
CREATE TABLE IF NOT EXISTS webdav_config (
    id          TEXT PRIMARY KEY,
    url         TEXT NOT NULL DEFAULT '',
    username    TEXT NOT NULL DEFAULT '',
    pwd_ref     TEXT NOT NULL DEFAULT '',
    remote_dir  TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 15. 应用偏好
CREATE TABLE IF NOT EXISTS app_setting (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL DEFAULT ''
);

-- 初始偏好
INSERT OR IGNORE INTO app_setting (key, value) VALUES
    ('auto_save_interval', '30'),
    ('font_size', '16'),
    ('line_height', '1.8'),
    ('auto_extract', 'false'),
    ('theme', 'system'),
    ('left_panel_visible', 'true'),
    ('right_panel_visible', 'true');

-- 初始 6 大 Agent
INSERT OR IGNORE INTO agent (id, key, name, system_prompt) VALUES
    ('agent-worldview', 'worldview', '世界观 Agent', '你是一个专业的小说世界观架构师。你的职责是补全设定细节、规范专用术语，并严格保证与已有世界规则不存在矛盾。'),
    ('agent-character', 'character', '角色 Agent', '你是一个小说角色刻画专家。负责分析人物外貌、动机、性格弧光与关系网，使人物形象鲜活且前后言行一致。'),
    ('agent-plot', 'plot', '剧情 Agent', '你是一个剧情编剧助手。擅长梳理主支线推进逻辑、分析戏剧冲突，并严密追踪伏笔的设置与回收状态。'),
    ('agent-outline', 'outline', '大纲 Agent', '你是一个宏观大纲顾问。负责把控篇章节奏、起承转合，提炼并维护全篇梗概与分卷大纲。'),
    ('agent-volchap', 'volume_chapter', '卷章 Agent', '你是一个卷章管理助手。负责协助作者提炼单卷主题简介与精炼的单章核心摘要。'),
    ('agent-writer', 'writing_assistant', '写作助手', '你是一个贴身小说写作助理。你必须无条件严格遵循作者设定的世界观、角色特质与大纲规划进行扩写、润色与创作。若指令与既有设定冲突，须主动提示并中止写入。');

-- 16. 角色关系网络
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
