//! 改动传播（P2-4）的确定性影响面扫描测试。
//!
//! 方案里的验收是「改一条世界观条目的标题，断言列出所有含该词的历史章节」。
//! 这里把「列出」这一步（后端能力）锁住；UI 展示由 setting_changes_modal 承担。
//!
//! 之所以坚持不引入模型：影响面扫描的价值是**先把候选收窄**，
//! 这一步必须是确定、可复现、可断言的；要不要真的同步修改才需要人/模型判断。
use easy_writing_lib::db;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_impact_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn finds_every_chapter_mentioning_the_term() {
    let dir = temp_dir("hits");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let novel = db::novel::create_novel(&conn, "影响面", "").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();

    // 第 1 章埋设定、第 2 章命中两次、第 3 章完全无关
    let texts = [
        "断水剑是沈家的祖传之物。",
        "他握紧断水剑。断水剑嗡鸣不止。",
        "这一章只写天气。",
    ];
    let mut ids = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let ch = db::chapter::create_chapter(
            &conn, &novel.id, &vol.id, &format!("第{}章", i + 1), "",
        )
        .unwrap();
        db::chapter::save_chapter_content(&conn, &ch.id, "{}", text, text.chars().count() as i32)
            .unwrap();
        ids.push(ch.id);
    }

    let affected = db::chapter::find_chapters_mentioning(&conn, &novel.id, "断水剑").unwrap();

    assert_eq!(affected.len(), 2, "应当只命中第 1、2 章，实际：{affected:?}");
    // 命中次数多者排前
    assert_eq!(affected[0].id, ids[1]);
    assert_eq!(affected[0].hits, 2);
    assert_eq!(affected[1].id, ids[0]);
    assert_eq!(affected[1].hits, 1);
    // 上下文片段要能让人判断相关性
    assert!(affected[0].snippet.contains("断水剑"), "片段应包含命中词");
}

#[test]
fn empty_or_absent_term_yields_nothing() {
    let dir = temp_dir("empty");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "空", "").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "V", "").unwrap();
    let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "C", "").unwrap();
    db::chapter::save_chapter_content(&conn, &ch.id, "{}", "正文", 2).unwrap();

    // 空词不该匹配一切（否则影响面列表会把整本书都列出来）
    assert!(db::chapter::find_chapters_mentioning(&conn, &novel.id, "")
        .unwrap()
        .is_empty());
    assert!(db::chapter::find_chapters_mentioning(&conn, &novel.id, "   ")
        .unwrap()
        .is_empty());
    assert!(db::chapter::find_chapters_mentioning(&conn, &novel.id, "不存在的词")
        .unwrap()
        .is_empty());
}

#[test]
fn scan_is_scoped_to_one_novel() {
    let dir = temp_dir("scope");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let mut novels = Vec::new();
    for title in ["甲", "乙"] {
        let novel = db::novel::create_novel(&conn, title, "").unwrap();
        let vol = db::volume::create_volume(&conn, &novel.id, "V", "").unwrap();
        let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "C", "").unwrap();
        db::chapter::save_chapter_content(&conn, &ch.id, "{}", "青云宗", 3).unwrap();
        novels.push(novel.id);
    }

    let a = db::chapter::find_chapters_mentioning(&conn, &novels[0], "青云宗").unwrap();
    assert_eq!(a.len(), 1, "影响面必须限制在当前作品内");
}
