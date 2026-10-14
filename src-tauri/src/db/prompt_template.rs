use rusqlite::{params, Connection, Result};
use shared_types::PromptTemplate;

pub fn get_default_template(key: &str) -> &'static str {
    crate::agent::polish::get_polish_prompt(key)
}

pub fn list_prompt_templates(conn: &Connection) -> Result<Vec<PromptTemplate>> {
    let modules = vec![
        ("worldview", "世界观设定修饰"),
        ("character", "角色卡塑造修饰"),
        ("plot", "剧情与伏笔修饰"),
        ("outline", "全篇梗概节奏梳理"),
        ("volume_summary", "分卷主题概要润色"),
        ("chapter_summary", "单章摘要梗概提炼"),
    ];

    let mut result = Vec::new();
    for (key, name) in modules {
        let default_tmpl = get_default_template(key).to_string();
        let (template, updated_at) = conn
            .query_row(
                "SELECT content, updated_at FROM prompt_template WHERE scope = ?1",
                params![key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .unwrap_or((default_tmpl.clone(), String::new()));

        result.push(PromptTemplate {
            key: key.to_string(),
            name: name.to_string(),
            template,
            default_template: default_tmpl,
            updated_at,
        });
    }
    Ok(result)
}

pub fn save_prompt_template(conn: &Connection, key: &str, template: &str) -> Result<()> {
    let id = format!("tmpl_{}", key);
    conn.execute(
        "INSERT INTO prompt_template (id, scope, content, is_default, updated_at)
         VALUES (?1, ?2, ?3, 0, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
            content = excluded.content,
            is_default = 0,
            updated_at = datetime('now')",
        params![id, key, template],
    )?;
    Ok(())
}

pub fn reset_prompt_template(conn: &Connection, key: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM prompt_template WHERE scope = ?1",
        params![key],
    )?;
    Ok(())
}

pub fn get_active_prompt_template(conn: &Connection, key: &str) -> String {
    conn.query_row(
        "SELECT content FROM prompt_template WHERE scope = ?1",
        params![key],
        |r| r.get(0),
    )
    .unwrap_or_else(|_| get_default_template(key).to_string())
}
