use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn create_sample_epub(
    title: &str,
    volumes_and_chapters: &[(&str, &[(&str, &str)])],
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 1. mimetype
    let stored_opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", stored_opt).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();

    // 2. META-INF/container.xml
    zip.start_file("META-INF/container.xml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
</container>"#,
    )
    .unwrap();

    // 3. Chapter files & manifest/spine metadata
    let mut manifest_items = String::new();
    let mut spine_items = String::new();
    let mut ncx_navpoints = String::new();
    let mut doc_index = 0;

    manifest_items.push_str(
        r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
    );

    for (vol_title, chapters) in volumes_and_chapters {
        // If volume title is not empty, write a volume cover/separator file
        if !vol_title.is_empty() {
            doc_index += 1;
            let file_name = format!("OEBPS/vol_{}.xhtml", doc_index);
            let item_id = format!("vol_{}", doc_index);
            manifest_items.push_str(&format!(
                r#"<item id="{}" href="vol_{}.xhtml" media-type="application/xhtml+xml"/>"#,
                item_id, doc_index
            ));
            spine_items.push_str(&format!(r#"<itemref idref="{}"/>"#, item_id));

            zip.start_file(&file_name, options).unwrap();
            let vol_html = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{}</title></head>
<body><h1>{}</h1></body>
</html>"#,
                vol_title, vol_title
            );
            zip.write_all(vol_html.as_bytes()).unwrap();
        }

        for (chap_title, chap_body) in *chapters {
            doc_index += 1;
            let rel_file = format!("chapter_{}.xhtml", doc_index);
            let file_name = format!("OEBPS/{}", rel_file);
            let item_id = format!("chap_{}", doc_index);

            manifest_items.push_str(&format!(
                r#"<item id="{}" href="{}" media-type="application/xhtml+xml"/>"#,
                item_id, rel_file
            ));
            spine_items.push_str(&format!(r#"<itemref idref="{}"/>"#, item_id));

            ncx_navpoints.push_str(&format!(
                r#"<navPoint id="np_{}" playOrder="{}">
                    <navLabel><text>{}</text></navLabel>
                    <content src="{}"/>
                </navPoint>"#,
                doc_index, doc_index, chap_title, rel_file
            ));

            zip.start_file(&file_name, options).unwrap();
            let chap_html = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{}</title></head>
<body>
    <h2>{}</h2>
    <p>{}</p>
    <p>第二段测试文字，描述宏大世界观与冒险征程。</p>
</body>
</html>"#,
                chap_title, chap_title, chap_body
            );
            zip.write_all(chap_html.as_bytes()).unwrap();
        }
    }

    // 4. OEBPS/content.opf
    zip.start_file("OEBPS/content.opf", options).unwrap();
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId">
    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>{}</dc:title>
        <dc:language>zh-CN</dc:language>
        <dc:creator>测试作者</dc:creator>
    </metadata>
    <manifest>
        {}
    </manifest>
    <spine toc="ncx">
        {}
    </spine>
</package>"#,
        title, manifest_items, spine_items
    );
    zip.write_all(opf.as_bytes()).unwrap();

    // 5. OEBPS/toc.ncx
    zip.start_file("OEBPS/toc.ncx", options).unwrap();
    let ncx = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
    <head><meta name="dtb:uid" content="urn:uuid:test"/></head>
    <docTitle><text>{}</text></docTitle>
    <navMap>
        {}
    </navMap>
</ncx>"#,
        title, ncx_navpoints
    );
    zip.write_all(ncx.as_bytes()).unwrap();

    zip.finish().unwrap();
    buf
}

#[test]
fn test_parse_import_epub_success() {
    let epub_bytes = create_sample_epub(
        "星海破晓传说",
        &[
            (
                "第一卷 苍穹遗迹",
                &[
                    ("第1章 觉醒时刻", "少年从冰冷的休眠仓中醒来，四周是一片死寂。"),
                    ("第2章 废弃基地", "金属闸门缓缓开启，尘封万年的星图重新亮起。"),
                ],
            ),
            (
                "第二卷 星际迷途",
                &[
                    ("第3章 跃迁引擎", "引擎轰鸣作响，飞船化作一道流光穿入超空间。"),
                ],
            ),
        ],
    );

    let preview = easy_writing_lib::import::epub::parse_import_epub("星海破晓.epub", &epub_bytes)
        .expect("解析标准 EPUB 应成功");

    assert_eq!(preview.novel_title, "星海破晓传说");
    assert!(preview.total_word_count > 0);
    assert_eq!(preview.volumes.len(), 2);

    let v1 = &preview.volumes[0];
    assert_eq!(v1.title, "第一卷 苍穹遗迹");
    assert_eq!(v1.chapters.len(), 2);
    assert_eq!(v1.chapters[0].title, "第1章 觉醒时刻");
    assert!(v1.chapters[0].content.contains("<p>"));
    assert!(v1.chapters[0].content_plain.contains("休眠仓"));

    assert_eq!(v1.chapters[1].title, "第2章 废弃基地");

    let v2 = &preview.volumes[1];
    assert_eq!(v2.title, "第二卷 星际迷途");
    assert_eq!(v2.chapters.len(), 1);
    assert_eq!(v2.chapters[0].title, "第3章 跃迁引擎");
}

#[test]
fn test_parse_import_epub_corrupted_data() {
    let bad_bytes = b"not a valid zip or epub file";
    let res = easy_writing_lib::import::epub::parse_import_epub("损坏.epub", bad_bytes);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("无法解压 EPUB 文件"));
}

#[test]
fn test_parse_plain_text_document_structure() {
    let raw_text = r#"
作品相关

前言背景
这是一段介绍正文创作背景的文字。

1、参考史书：《宋史》、《续资治通鉴长编》。
2、地理志对照：《元丰九域志》。
以上考据仅供参考。

第一卷 问鼎明道宫

第一章 明道宫
赵玖站在明道宫前，看着满庭的积雪。
第五卷第三十七章提到过的御营禁军在此集结。

第二章 召见
群臣在大殿内肃立。

番外卷 闲话杂记

番外一 东京梦华
汴梁城内依旧繁华。
"#;

    let preview = easy_writing_lib::import::parse_import_document("测试长篇演义 (精校版).txt", raw_text);
    assert_eq!(preview.novel_title, "测试长篇演义 (精校版)");
    assert_eq!(preview.volumes.len(), 3, "应该识别为3个分卷：默认/作品相关卷、第一卷、番外卷");

    assert_eq!(preview.volumes[0].title, "作品相关");
    assert_eq!(preview.volumes[0].chapters.len(), 1);
    assert_eq!(preview.volumes[0].chapters[0].title, "前言背景");
    // 确保 1、 和 2、 没有被误切分为独立章节
    assert!(preview.volumes[0].chapters[0].content_plain.contains("1、参考史书"));
    assert!(preview.volumes[0].chapters[0].content_plain.contains("2、地理志对照"));

    assert_eq!(preview.volumes[1].title, "第一卷 问鼎明道宫");
    assert_eq!(preview.volumes[1].chapters.len(), 2);
    assert_eq!(preview.volumes[1].chapters[0].title, "第一章 明道宫");
    assert_eq!(preview.volumes[1].chapters[1].title, "第二章 召见");
    // 确保正文中的“第五卷第三十七章提到过的御营禁军在此集结。”没有被误切为分卷
    assert!(preview.volumes[1].chapters[0].content_plain.contains("第五卷第三十七章提到过的御营禁军在此集结。"));

    assert_eq!(preview.volumes[2].title, "番外卷 闲话杂记");
    assert_eq!(preview.volumes[2].chapters.len(), 1);
    assert_eq!(preview.volumes[2].chapters[0].title, "番外一 东京梦华");

    assert!(preview.total_word_count > 0);
}


// ---------------------------------------------------------------------------
// V0.3.8 · 导入边界（基线 C-09）
//
// 原有 3 个测试全部是正常路径。这里补上异常输入：
// 恶意 / 畸形 EPUB 不得让解析阶段无界占用内存。
// ---------------------------------------------------------------------------

#[test]
fn rejects_oversized_epub_payload() {
    // 远超 64 MiB 入口上限。构造方式是直接喂字节，不真的生成 64 MiB 文件，
    // 边界判断发生在解码之前。
    let huge = vec![b'A'; 65 * 1024 * 1024];
    let err = easy_writing_lib::import::epub::parse_import_epub("huge.epub", &huge)
        .expect_err("超过入口上限的文件必须被拒绝");
    assert!(
        err.contains("过大") || err.contains("上限"),
        "错误信息应说明是体积超限，实际：{err}"
    );
}

#[test]
fn rejects_non_zip_payload_with_clear_message() {
    // 一个 HTML 错误页被当成 .epub 传入 —— 常见的用户误操作。
    let html = b"<!DOCTYPE html><html><body>404 Not Found</body></html>";
    let err = easy_writing_lib::import::epub::parse_import_epub("page.epub", html)
        .expect_err("非 ZIP 数据必须被拒绝");
    assert!(
        err.contains("ZIP") || err.contains("EPUB"),
        "错误信息应说明不是有效 EPUB，实际：{err}"
    );
}

#[test]
fn rejects_zip_bomb_entry() {
    // 压缩炸弹：一个解压后 ~48 MiB 的高重复内容条目，远超单条目 16 MiB 上限，
    // 但压缩后只有几十 KB。解析必须在读取阶段就中止。
    let mut buf = Vec::new();
    let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
    let options =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("mimetype", options).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", options).unwrap();
    zip.write_all(br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#).unwrap();
    zip.start_file("OEBPS/content.opf", options).unwrap();
    zip.write_all(br#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>BombBook</dc:title></metadata>
<manifest><item id="c1" href="bomb.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="c1"/></spine>
</package>"#).unwrap();

    // 48 MiB 的高重复内容 —— deflate 压缩率极高
    zip.start_file("OEBPS/bomb.xhtml", options).unwrap();
    let bomb_chunk = "A".repeat(1024 * 1024);
    for _ in 0..48 {
        zip.write_all(bomb_chunk.as_bytes()).unwrap();
    }
    zip.finish().unwrap();

    let result = easy_writing_lib::import::epub::parse_import_epub("bomb.epub", &buf);
    // 关键断言：无论它是「被拒绝」还是「成功但正文为空」，
    // 都**不能**是因为把 48 MiB 读进了内存。压缩后体积很小，
    // 因此必须走的是上限拒绝路径。
    match result {
        Err(e) => {
            assert!(
                e.contains("上限") || e.contains("超过"),
                "应因上限被拒绝，实际：{e}"
            );
        }
        Ok(preview) => panic!(
            "48 MiB 条目不应被接受为正常 EPUB，实际解析出 {} 卷",
            preview.volumes.len()
        ),
    }
}

#[test]
fn rejects_zip_with_too_many_entries() {
    let mut buf = Vec::new();
    let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
    let options =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    // 超过 20000 条上限
    for i in 0..20_050 {
        zip.start_file(format!("OEBPS/spam{:05}.txt", i), options).unwrap();
        zip.write_all(b"x").unwrap();
    }
    zip.finish().unwrap();

    let err = easy_writing_lib::import::epub::parse_import_epub("spam.epub", &buf)
        .expect_err("条目数超限必须被拒绝");
    assert!(
        err.contains("上限") || err.contains("超过"),
        "错误信息应说明条目数超限，实际：{err}"
    );
}

#[test]
fn normal_epub_still_imports_after_limits_were_added() {
    // 加了上限不能误伤正常文件 —— 这是本组测试存在的意义。
    let data = create_sample_epub(
        "正常作品",
        &[("第一卷", &[("第一章", "这是正文内容，应当正常解析。")])],
    );
    let preview = easy_writing_lib::import::epub::parse_import_epub("ok.epub", &data)
        .expect("正常 EPUB 不得被上限误伤");
    assert_eq!(preview.novel_title, "正常作品");
}
