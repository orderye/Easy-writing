use rusqlite::{params, Connection};
use shared_types::{ImportPreview, Novel};
use uuid::Uuid;

/// 将解析完成的 ImportPreview 结构以事务写入数据库（支持新建作品或追加合并至现有作品）
pub fn commit_import_novel(
    conn: &mut Connection,
    target_novel_id: Option<String>,
    preview: ImportPreview,
) -> Result<Novel, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (novel_id, novel) = match target_novel_id {
        Some(nid) => {
            let novel = tx
                .query_row(
                    "SELECT id, title, synopsis, created_at, updated_at, deleted_at FROM novel WHERE id = ?1 AND deleted_at IS NULL",
                    params![nid],
                    |r| {
                        Ok(Novel {
                            id: r.get(0)?,
                            title: r.get(1)?,
                            synopsis: r.get(2)?,
                            created_at: r.get(3)?,
                            updated_at: r.get(4)?,
                            deleted_at: r.get(5)?,
                        })
                    },
                )
                .map_err(|e| format!("目标作品未找到: {}", e))?;
            (nid, novel)
        }
        None => {
            let new_id = Uuid::new_v4().to_string();
            let synopsis = format!(
                "批量导入长篇文稿，共包含 {} 卷，{} 字。",
                preview.volumes.len(),
                preview.total_word_count
            );
            tx.execute(
                "INSERT INTO novel (id, title, synopsis) VALUES (?1, ?2, ?3)",
                params![new_id, preview.novel_title, synopsis],
            )
            .map_err(|e| format!("创建作品失败: {}", e))?;

            let novel = tx
                .query_row(
                    "SELECT id, title, synopsis, created_at, updated_at, deleted_at FROM novel WHERE id = ?1",
                    params![new_id],
                    |r| {
                        Ok(Novel {
                            id: r.get(0)?,
                            title: r.get(1)?,
                            synopsis: r.get(2)?,
                            created_at: r.get(3)?,
                            updated_at: r.get(4)?,
                            deleted_at: r.get(5)?,
                        })
                    },
                )
                .map_err(|e| format!("读取新建作品失败: {}", e))?;
            (new_id, novel)
        }
    };

    // 查询当前已有卷的最大序号
    let mut vol_order: i32 = tx
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) FROM volume WHERE novel_id = ?1 AND deleted_at IS NULL",
            params![novel_id],
            |r| r.get(0),
        )
        .unwrap_or(-1)
        + 1;

    for vol_preview in preview.volumes {
        let vol_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO volume (id, novel_id, title, summary, sort_order) VALUES (?1, ?2, ?3, '', ?4)",
            params![vol_id, novel_id, vol_preview.title, vol_order],
        )
        .map_err(|e| format!("创建分卷失败: {}", e))?;
        vol_order += 1;

        let mut chap_order = 0;
        for chap_preview in vol_preview.chapters {
            let chap_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO chapter (id, novel_id, volume_id, title, summary, content, content_plain, word_count, status, sort_order)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, 'draft', ?8)",
                params![
                    chap_id,
                    novel_id,
                    vol_id,
                    chap_preview.title,
                    chap_preview.content,
                    chap_preview.content_plain,
                    chap_preview.word_count,
                    chap_order,
                ],
            )
            .map_err(|e| format!("创建章节失败: {}", e))?;
            chap_order += 1;
        }
    }

    tx.commit().map_err(|e| format!("提交导入事务失败: {}", e))?;
    Ok(novel)
}
