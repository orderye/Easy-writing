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
