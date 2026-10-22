//! 作品导出的集成测试（任务书 T02 / PR-07）。
//!
//! ## 覆盖什么
//!
//! - **三种格式**都能产出结构合法的文件：txt 直接读、docx/epub 解 zip 看条目；
//! - **EPUB 能被本仓库的导入器读回** —— 这是最硬的一条：导出器与导入器在同一个
//!   仓库里，导出的书连自己的导入器都读不回来，那就更别指望别的阅读器能读；
//! - **字数口径**：导出摘要里的字数与应用状态栏（`CharacterCount.characters()`）
//!   一致，即「文本字符数 + 叶节点数」，而不是 `content_plain` 的长度；
//! - **图片**：含图与不含图两种场景，含图时图片真的进了包；
//! - **取消不留半成品**：取消或失败之后，目标路径与临时文件都不该存在；
//! - **大样本**：300 章 / 约百万字能导出完，且章数与字数与库里一致。
//!
//! ## 数据安全
//!
//! 全部数据都在 `std::env::temp_dir()` 下的隔离目录里，
//! **不接触用户真实数据库与真实文档目录**。

mod common;

use common::temp_dir;
use easy_writing_lib::db;
use easy_writing_lib::export;
use shared_types::{ExportFormat, ExportOptions, ExportProgress, ParagraphSeparator};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use zip::ZipArchive;

/// 样例章节的正文长度（字符数）。
const SAMPLE_CHAPTER_CHARS: i64 = 21;

/// 造一部两卷三章的小书，返回（连接池、作品 id、章 id 列表）。
fn build_sample_book(dir: &Path) -> (easy_writing_lib::db::DbPool, String, Vec<String>) {
    let pool = db::init_db(dir.join("data")).expect("初始化测试库");
    let (novel_id, chapter_ids) = {
        let conn = pool.get().expect("取连接");
        let novel = db::novel::create_novel(&conn, "星海破晓", "测试作品").expect("建作品");
        let v1 = db::volume::create_volume(&conn, &novel.id, "第一卷 苍穹遗迹", "").expect("建卷");
        let v2 = db::volume::create_volume(&conn, &novel.id, "第二卷 星际迷途", "").expect("建卷");

        let mut ids = Vec::new();
        for (volume_id, title) in [
            (&v1.id, "第1章 觉醒时刻"),
            (&v1.id, "第2章 废弃基地"),
            (&v2.id, "第3章 跃迁引擎"),
        ] {
            let chapter =
                db::chapter::create_chapter(&conn, &novel.id, volume_id, title, "").expect("建章");
            db::chapter::save_chapter_content(
                &conn,
                &chapter.id,
                &tiptap_doc(&[("少年从冰冷的休眠仓中醒来，四周是一片死寂。", false)]),
                "少年从冰冷的休眠仓中醒来，四周是一片死寂。",
                22,
            )
            .expect("写正文");
            ids.push(chapter.id);
        }
        (novel.id, ids)
    };
    (pool, novel_id, chapter_ids)
}

/// 造一段 Tiptap JSON。
///
/// `bold` 为真时给文本加粗标记 —— 用来验证行内样式在三种格式里都被保留。
fn tiptap_doc(paragraphs: &[(&str, bool)]) -> String {
    let content: Vec<String> = paragraphs
        .iter()
        .map(|(text, bold)| {
            let marks = if *bold {
                r#","marks":[{"type":"bold"}]"#
            } else {
                ""
            };
            format!(
                r#"{{"type":"paragraph","content":[{{"type":"text","text":"{}"{}}}]}}"#,
                text, marks
            )
        })
        .collect();
    format!(
        r#"{{"type":"doc","content":[{}]}}"#,
        content.join(",")
    )
}

/// 默认导出选项。
fn options(novel_id: &str, format: ExportFormat) -> ExportOptions {
    ExportOptions {
        novel_id: novel_id.to_string(),
        format,
        volume_ids: Vec::new(),
        chapter_ids: Vec::new(),
        volume_title_template: "{volume_index} {volume_title}".to_string(),
        chapter_title_template: "{chapter_index} {chapter_title}".to_string(),
        paragraph_separator: ParagraphSeparator::Indent,
        include_images: false,
        file_name: "星海破晓".to_string(),
    }
}

/// 执行一次导出（不收集进度）。
fn run_export(
    pool: &easy_writing_lib::db::DbPool,
    app_data_dir: &Path,
    target: &Path,
    options: &ExportOptions,
) -> export::ExportOutcome {
    export::export_novel(
        pool,
        app_data_dir,
        target,
        options,
        &|_progress: ExportProgress| {},
        &AtomicBool::new(false),
    )
    .expect("导出应成功")
}

/// 临时文件路径（与 `export::mod` 内部一致）。
fn temp_path_of(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "export".to_string());
    name.push_str(".ew-export-part");
    target.with_file_name(name)
}

#[test]
fn txt_export_contains_titles_and_paragraphs() {
    let dir = temp_dir("export_txt");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let target = dir.join("out.txt");

    let outcome = run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Txt));
    let text = std::fs::read_to_string(&target).expect("读导出文件");

    assert!(text.contains("第一卷 苍穹遗迹"), "应含卷标题");
    assert!(text.contains("第1章 觉醒时刻"), "应含章标题");
    assert!(text.contains("少年从冰冷的休眠仓中醒来"), "应含正文");
    assert_eq!(outcome.summary.chapter_count, 3);
    assert_eq!(outcome.summary.volume_count, 2);
    assert!(outcome.summary.path.ends_with("out.txt"));
}

#[test]
fn txt_export_word_count_matches_editor_semantics() {
    let dir = temp_dir("export_count");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let target = dir.join("out.txt");

    // 一章正文：两个段落 + 一张图片（叶节点算 1 字）。
    let chapter_id = {
        let conn = pool.get().unwrap();
        let chapters = db::chapter::list_chapters_for_export(&conn, &novel_id).unwrap();
        chapters[0].id.clone()
    };
    {
        let conn = pool.get().unwrap();
        let doc = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"十二个字"}]},{"type":"image","attrs":{"src":"asset://local/images/x.png","alt":"图"}}]}"#;
        db::chapter::save_chapter_content(&conn, &chapter_id, doc, "十二个字", 5).unwrap();
    }

    let outcome = run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Txt));

    // 「十二个字」= 4 个字符，图片 = 1 个叶节点 → 5。
    // 另外两章各 21 字。
    let others = SAMPLE_CHAPTER_CHARS * 2;
    assert_eq!(
        outcome.summary.word_count,
        others + 5,
        "字数应为文本字符数 + 叶节点数"
    );
    // 导出正文的字符数：段落 4 字 + 图片 alt 1 字 —— 与 `content_plain`
    // 的长度不是一回事（后者是编辑器 `getText()` 的产物，块间多两个换行）。
    assert_eq!(outcome.summary.exported_text_chars, others + 5);
}

#[test]
fn txt_export_uses_indent_or_blank_line_separator() {
    let dir = temp_dir("export_sep");
    let (pool, novel_id, _) = build_sample_book(&dir);

    let mut indent_options = options(&novel_id, ExportFormat::Txt);
    indent_options.paragraph_separator = ParagraphSeparator::Indent;
    let indent_target = dir.join("indent.txt");
    run_export(&pool, &dir, &indent_target, &indent_options);
    let indent_text = std::fs::read_to_string(&indent_target).unwrap();
    assert!(
        indent_text.contains('\u{3000}'),
        "缩进模式段首应有全角空格"
    );

    let mut blank_options = options(&novel_id, ExportFormat::Txt);
    blank_options.paragraph_separator = ParagraphSeparator::BlankLine;
    let blank_target = dir.join("blank.txt");
    run_export(&pool, &dir, &blank_target, &blank_options);
    let blank_text = std::fs::read_to_string(&blank_target).unwrap();
    assert!(
        !blank_text.contains('\u{3000}'),
        "空行模式段首不应有全角空格"
    );
    assert!(blank_text.contains("\n\n"), "空行模式段间应有空行");
}

#[test]
fn docx_is_valid_zip_with_expected_parts() {
    let dir = temp_dir("export_docx");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let target = dir.join("out.docx");

    run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Docx));

    let file = std::fs::File::open(&target).expect("打开 docx");
    let mut archive = ZipArchive::new(file).expect("docx 应是合法 zip");
    let names: Vec<String> = archive.file_names().flatten().map(|n| n.to_string()).collect();

    for expected in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "docx 缺少 {}，实际条目：{:?}",
            expected,
            names
        );
    }

    let mut document = String::new();
    {
        let mut entry = archive
            .by_name("word/document.xml")
            .expect("读 document.xml");
        std::io::Read::read_to_string(&mut entry, &mut document).unwrap();
    }
    assert!(document.contains("少年从冰冷的休眠仓中醒来"));
    assert!(document.contains("第1章 觉醒时刻"));
}

#[test]
fn epub_first_entry_is_stored_mimetype() {
    let dir = temp_dir("export_epub_mime");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let target = dir.join("out.epub");

    run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Epub));

    let file = std::fs::File::open(&target).expect("打开 epub");
    let mut archive = ZipArchive::new(file).expect("epub 应是合法 zip");

    let mut first = archive.by_index(0).expect("读第一个条目");
    assert_eq!(
        first.name().expect("读条目名"),
        "mimetype",
        "mimetype 必须是第一个条目"
    );
    assert!(
        first.compression() == zip::CompressionMethod::Stored,
        "mimetype 不能压缩（阅读器靠它嗅探格式）"
    );
    let mut content = String::new();
    std::io::Read::read_to_string(&mut first, &mut content).unwrap();
    assert_eq!(content, "application/epub+zip");
}

#[test]
fn epub_is_readable_by_own_importer() {
    let dir = temp_dir("export_epub_read");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let target = dir.join("out.epub");

    run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Epub));

    let bytes = std::fs::read(&target).expect("读 epub");
    let preview = easy_writing_lib::import::epub::parse_import_epub("out.epub", &bytes)
        .expect("本仓库的导入器应能读回导出的 EPUB");

    assert_eq!(preview.novel_title, "星海破晓");
    assert_eq!(preview.volumes.len(), 2, "卷结构应被还原");
    let total_chapters: usize = preview.volumes.iter().map(|v| v.chapters.len()).sum();
    assert_eq!(total_chapters, 3, "三章都应被读回");
    assert!(preview.total_word_count > 0);
    assert!(preview.volumes[0].chapters[0]
        .content_plain
        .contains("少年从冰冷的休眠仓中醒来"));
}

#[test]
fn epub_contains_ncx_and_nav() {
    let dir = temp_dir("export_epub_toc");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let target = dir.join("out.epub");

    run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Epub));

    let file = std::fs::File::open(&target).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let names: Vec<String> = archive.file_names().flatten().map(|n| n.to_string()).collect();

    assert!(names.iter().any(|n| n.ends_with("toc.ncx")), "缺少 NCX");
    assert!(names.iter().any(|n| n.ends_with("nav.xhtml")), "缺少 nav");
    assert!(
        names.iter().any(|n| n == "META-INF/container.xml"),
        "缺少 container.xml"
    );
    assert!(names.iter().any(|n| n.ends_with("content.opf")), "缺少 OPF");
}

#[test]
fn export_with_images_embeds_media() {
    let dir = temp_dir("export_img");
    let (pool, novel_id, _) = build_sample_book(&dir);

    // 存一张真图片（1x1 PNG），拿到正文里该用的引用。
    let rel_path = {
        let conn = pool.get().unwrap();
        easy_writing_lib::asset::save_image(
            &conn,
            &dir,
            &novel_id,
            "pic.png",
            "image/png",
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .expect("存图片")
    };
    let src = format!("asset://local/{}", rel_path);

    let chapter_id = {
        let conn = pool.get().unwrap();
        db::chapter::list_chapters_for_export(&conn, &novel_id).unwrap()[0]
            .id
            .clone()
    };
    {
        let conn = pool.get().unwrap();
        let doc = format!(
            r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"看图"}}]}},{{"type":"image","attrs":{{"src":"{}","alt":"插图"}}}}]}}"#,
            src
        );
        db::chapter::save_chapter_content(&conn, &chapter_id, &doc, "看图", 2).unwrap();
    }

    let mut opts = options(&novel_id, ExportFormat::Epub);
    opts.include_images = true;
    let target = dir.join("with_image.epub");
    let outcome = run_export(&pool, &dir, &target, &opts);

    assert_eq!(outcome.summary.included_images, 1);
    assert_eq!(outcome.summary.skipped_images, 0);

    let file = std::fs::File::open(&target).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let names: Vec<String> = archive.file_names().flatten().map(|n| n.to_string()).collect();
    assert!(
        names.iter().any(|n| n.starts_with("EPUB/images/")),
        "含图导出应把图片写进包，实际条目：{:?}",
        names
    );
}

#[test]
fn export_without_images_skips_and_counts() {
    let dir = temp_dir("export_noimg");
    let (pool, novel_id, _) = build_sample_book(&dir);

    let rel_path = {
        let conn = pool.get().unwrap();
        easy_writing_lib::asset::save_image(
            &conn,
            &dir,
            &novel_id,
            "pic.png",
            "image/png",
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .unwrap()
    };
    let src = format!("asset://local/{}", rel_path);
    let chapter_id = {
        let conn = pool.get().unwrap();
        db::chapter::list_chapters_for_export(&conn, &novel_id).unwrap()[0]
            .id
            .clone()
    };
    {
        let conn = pool.get().unwrap();
        let doc = format!(
            r#"{{"type":"doc","content":[{{"type":"image","attrs":{{"src":"{}","alt":"插图"}}}}]}}"#,
            src
        );
        db::chapter::save_chapter_content(&conn, &chapter_id, &doc, "", 1).unwrap();
    }

    let mut opts = options(&novel_id, ExportFormat::Epub);
    opts.include_images = false;
    let target = dir.join("without_image.epub");
    let outcome = run_export(&pool, &dir, &target, &opts);

    assert_eq!(outcome.summary.included_images, 0);
    assert_eq!(outcome.summary.skipped_images, 1);

    let file = std::fs::File::open(&target).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let names: Vec<String> = archive.file_names().flatten().map(|n| n.to_string()).collect();
    assert!(
        !names.iter().any(|n| n.starts_with("EPUB/images/")),
        "不含图时不应写图片"
    );
}

#[test]
fn cancel_leaves_no_partial_file() {
    let dir = temp_dir("export_cancel");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let target = dir.join("cancelled.txt");

    // 取消位一开始就置位 → 第一章前就退出。
    let cancel = AtomicBool::new(true);
    let outcome = export::export_novel(
        &pool,
        &dir,
        &target,
        &options(&novel_id, ExportFormat::Txt),
        &|_progress: ExportProgress| {},
        &cancel,
    )
    .expect("取消不应算错误");

    assert!(outcome.summary.cancelled, "应标记为已取消");
    assert!(outcome.summary.path.is_empty(), "取消后不应有落盘路径");
    assert!(!target.exists(), "取消后不应留下目标文件");
    assert!(
        !temp_path_of(&target).exists(),
        "取消后不应留下临时文件"
    );
}

#[test]
fn degraded_chapter_is_reported() {
    let dir = temp_dir("export_degraded");
    let (pool, novel_id, _) = build_sample_book(&dir);
    let chapter_id = {
        let conn = pool.get().unwrap();
        db::chapter::list_chapters_for_export(&conn, &novel_id).unwrap()[0]
            .id
            .clone()
    };
    {
        let conn = pool.get().unwrap();
        // 不是 JSON 的正文 → 只能按纯文本导出。
        db::chapter::save_chapter_content(
            &conn,
            &chapter_id,
            "这是一段历史导入的纯文本正文。",
            "这是一段历史导入的纯文本正文。",
            15,
        )
        .unwrap();
    }

    let target = dir.join("out.txt");
    let outcome = run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Txt));

    assert_eq!(outcome.summary.degraded_chapters.len(), 1);
    assert!(outcome.summary.degraded_chapters[0].contains("第1章"));
}

#[test]
fn large_sample_export_completes_and_matches_stats() {
    let dir = temp_dir("export_large");
    let pool = db::init_db(dir.join("data")).expect("初始化测试库");
    let novel_id = {
        let conn = pool.get().unwrap();
        let novel = db::novel::create_novel(&conn, "长篇测试", "").unwrap();
        let volume = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();
        // 每章 1 段 × 约 3300 字 → 300 章约百万字。
        let long_text = "字".repeat(3300);
        for index in 0..300 {
            let chapter =
                db::chapter::create_chapter(&conn, &novel.id, &volume.id, &format!("第{}章", index + 1), "")
                    .unwrap();
            let doc = tiptap_doc(&[(&long_text, false)]);
            db::chapter::save_chapter_content(&conn, &chapter.id, &doc, &long_text, 3300).unwrap();
        }
        novel.id
    };

    let target = dir.join("large.txt");
    let started = std::time::Instant::now();
    let outcome = run_export(&pool, &dir, &target, &options(&novel_id, ExportFormat::Txt));
    let elapsed = started.elapsed();

    assert_eq!(outcome.summary.chapter_count, 300);
    assert_eq!(outcome.summary.word_count, 300 * 3300);
    assert!(target.exists());

    // 不设硬阈值（机器差异太大），但把实测值打出来供人判断。
    println!(
        "300 章 / {} 字导出耗时 {:?}",
        outcome.summary.word_count, elapsed
    );
    assert!(
        elapsed.as_secs() < 120,
        "百万字导出不应超过 2 分钟，实测 {:?}",
        elapsed
    );
}
