//! 针对本次差异修复的回归测试：
//! - 迁移脚本可重复执行（含 chat_session.title 兼容 ALTER）
//! - 对话记录写入 / 读取 / 按章节筛选 / 清空
//! - 生成参数 JSON 往返与非法值拒绝
//! - restore.rs 补图查询使用的 asset 列名与 schema 一致

use rusqlite::Connection;
use shared_types::GenerationParams;

fn fresh_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(include_str!("../migrations/V001__init.sql"))
        .expect("run migration");
    conn
}

/// 模拟 db::init_db 里的兼容补丁，并验证可重复执行。
fn apply_compat_patches(conn: &Connection) {
    let _ = conn.execute("ALTER TABLE setting_change ADD COLUMN target_title TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE setting_change ADD COLUMN is_reverted INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute(
        "ALTER TABLE chat_session ADD COLUMN title TEXT NOT NULL DEFAULT '对话'",
        [],
    );
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chat_session_novel ON chat_session(novel_id, chapter_id, updated_at DESC)",
        [],
    );
}

#[test]
fn migration_is_idempotent_with_compat_patches() {
    let conn = fresh_conn();
    // V001 本身用 IF NOT EXISTS，可重复执行
    conn.execute_batch(include_str!("../migrations/V001__init.sql"))
        .expect("re-run migration");
    // 兼容补丁在已有 title 列时应静默失败而非中断
    apply_compat_patches(&conn);
    apply_compat_patches(&conn);

    let has_title: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('chat_session') WHERE name = 'title'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(has_title, 1, "chat_session 应含 title 列");
}

#[test]
fn restore_asset_query_uses_existing_columns() {
    let conn = fresh_conn();
    // 迁移建表后即刻执行 restore.rs 里那条补图查询，必须能成功 prepare
    let stmt = conn.prepare("SELECT rel_path, hash FROM asset");
    assert!(
        stmt.is_ok(),
        "restore.rs 的补图查询必须能在 asset 表上 prepare：{:?}",
        stmt.err()
    );

    // 反向确认：旧字段名确实不存在（即原缺陷可复现）
    let bad = conn.prepare("SELECT file_path, hash FROM asset");
    assert!(bad.is_err(), "asset 表不应存在 file_path 列");
}

#[test]
fn chat_session_roundtrip_and_chapter_filter() {
    let conn = fresh_conn();
    apply_compat_patches(&conn);

    conn.execute(
        "INSERT INTO novel (id, title) VALUES ('n1', '测试作品')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO volume (id, novel_id, title) VALUES ('v1', 'n1', '卷一')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chapter (id, novel_id, volume_id, title) VALUES ('c1', 'n1', 'v1', '第一章')",
        [],
    )
    .unwrap();

    // 章节会话
    let s1 = easy_writing_lib::db::chat::ensure_session(&conn, "n1", Some("c1")).unwrap();
    assert_eq!(s1.chapter_id.as_deref(), Some("c1"));
    // 幂等：同一章节重复调用返回同一会话
    let s1_again = easy_writing_lib::db::chat::ensure_session(&conn, "n1", Some("c1")).unwrap();
    assert_eq!(s1.id, s1_again.id);

    // 全书会话（chapter_id IS NULL）
    let s2 = easy_writing_lib::db::chat::ensure_session(&conn, "n1", None).unwrap();
    assert_eq!(s2.chapter_id, None);
    assert_ne!(s1.id, s2.id, "章节会话与全书会话应相互独立");

    easy_writing_lib::db::chat::append_message(&conn, &s1.id, "user", "续写第一章").unwrap();
    easy_writing_lib::db::chat::append_message(&conn, &s1.id, "assistant", "好的").unwrap();
    easy_writing_lib::db::chat::append_message(&conn, &s1.id, "user", "续写第一章").unwrap();

    let msgs = easy_writing_lib::db::chat::list_messages(&conn, &s1.id, 0).unwrap();
    assert_eq!(msgs.len(), 3);
    // 重复内容必须都能保留（不再被 key 冲突吞掉）
    assert_eq!(msgs[0].content, msgs[2].content);

    // 限制条数时保留最近 N 条
    let recent = easy_writing_lib::db::chat::list_messages(&conn, &s1.id, 2).unwrap();
    assert_eq!(recent.len(), 2);

    // 按章节筛选
    let by_chapter = easy_writing_lib::db::chat::list_sessions(&conn, "n1", Some("c1")).unwrap();
    assert_eq!(by_chapter.len(), 1);
    assert_eq!(by_chapter[0].id, s1.id);
    assert_eq!(by_chapter[0].message_count, 3);

    let all = easy_writing_lib::db::chat::list_sessions(&conn, "n1", None).unwrap();
    assert_eq!(all.len(), 2, "不过滤时应列出该作品全部会话");

    // 清空只删消息，保留会话
    easy_writing_lib::db::chat::clear_messages(&conn, &s1.id).unwrap();
    assert!(easy_writing_lib::db::chat::list_messages(&conn, &s1.id, 0).unwrap().is_empty());
    assert!(easy_writing_lib::db::chat::get_session(&conn, &s1.id).unwrap().is_some());

    // 非法角色被拒绝
    assert!(easy_writing_lib::db::chat::append_message(&conn, &s1.id, "system", "x").is_err());

    // 删除会话连带消息
    easy_writing_lib::db::chat::delete_session(&conn, &s1.id).unwrap();
    assert!(easy_writing_lib::db::chat::get_session(&conn, &s1.id).unwrap().is_none());
}

#[test]
fn generation_params_roundtrip_and_empty_defaults() {
    // 空 / 非法 JSON 一律退化为「未设置」
    assert!(GenerationParams::parse_from_str("{}").is_empty());
    assert!(GenerationParams::parse_from_str("").is_empty());
    assert!(GenerationParams::parse_from_str("not json").is_empty());

    let p = GenerationParams {
        temperature: Some(0.7),
        max_tokens: Some(2048),
        stream: Some(false),
        context_window: None,
    };
    assert!(!p.is_empty());

    let json = p.to_json_string();
    let back = GenerationParams::parse_from_str(&json);
    assert_eq!(back.temperature, Some(0.7));
    assert_eq!(back.max_tokens, Some(2048));
    assert_eq!(back.stream, Some(false));

    // 只设置部分字段时，未设置字段不应污染 JSON（skip_serializing_if）
    let partial = GenerationParams {
        temperature: Some(1.2),
        ..Default::default()
    };
    let partial_json = partial.to_json_string();
    assert!(!partial_json.contains("max_tokens"));
    assert!(!partial_json.contains("stream"));
}

#[test]
fn auto_extract_seed_defaults_to_false() {
    let conn = fresh_conn();
    let value: String = conn
        .query_row(
            "SELECT value FROM app_setting WHERE key = 'auto_extract'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value, "false", "auto_extract 种子值必须为 false（PRD §4.2 默认关闭）");
}
