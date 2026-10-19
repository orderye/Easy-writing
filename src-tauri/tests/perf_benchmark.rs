//! 长篇作品性能基准。
//!
//! ## 这个文件为什么存在
//!
//! V0.3.8 之前，仓库里**没有任何性能测试**。章节目录加载、全书搜索、
//! 保存、字数统计的耗时全凭手感，改完代码也无法判断是变快还是变慢 ——
//! 「性能优化」因此没有可证伪的依据。
//!
//! ## 数据集是合成的，且明确声明
//!
//! 下面生成的是**合成作品**，不是真实用户数据。规模写在每个用例的名字里，
//! 报告里也必须原样引用，不能把合成测试说成真实作品测试。
//!
//! ## 怎么用
//!
//! ```text
//! cargo test -p easy-writing-core --test perf_benchmark -- --ignored --nocapture
//! ```
//!
//! 默认**不运行**（`#[ignore]`）：编译 + 造数据的成本不适合放进 CI 的常规门禁。
//! 需要在 CI 中设门槛时，用下面的 `EW_PERF_GATE` 环境变量打开严格模式，
//! 超过阈值即失败。
//!
//! ## 阈值的诚实说明
//!
//! 基线是**在开发机（macOS arm64、外置机械盘）上实测得到的**，
//! 不是任何用户环境的承诺。CI runner 的 CPU 差异很大，
//! 因此 `EW_PERF_GATE` 的阈值刻意宽松，只用来拦截「数量级退化」，
//! 精确的性能治理仍以本机定期跑出的报告为准。

use easy_writing_lib::db;
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::Instant;

/// 性能门禁阈值（毫秒）。留足余量：CI runner 普遍比开发机慢数倍，
/// 这里只拦截「劣化一个数量级」这类明显回归。
fn gate_ms() -> Option<f64> {
    std::env::var("EW_PERF_GATE_MS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
}

/// 记录一次测量并打印结果；有严格门禁时按阈值判定。
struct Bench {
    name: String,
    dataset: String,
}

impl Bench {
    fn new(name: impl Into<String>, dataset: impl Into<String>) -> Self {
        Bench {
            name: name.into(),
            dataset: dataset.into(),
        }
    }

    fn measure<T>(&self, mut f: impl FnMut() -> T) -> T {
        let t0 = Instant::now();
        let out = f();
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "[PERF] {:<42} {:<28} {:>9.2} ms",
            self.name, self.dataset, ms
        );
        if let Some(limit) = gate_ms() {
            assert!(
                ms <= limit,
                "性能门禁未通过：{} 在 {} 数据集上耗时 {:.2} ms，超过 {:.0} ms 阈值",
                self.name,
                self.dataset,
                ms,
                limit
            );
        }
        out
    }
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_perf_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 造一个含 `chapters` 章的作品。
///
/// 每章正文长度 `words` 个汉字左右。中文小说的典型章节是 2000~5000 字，
/// 因此 `long_chapter` 用例取 4000。
fn seed(conn: &Connection, title: &str, volumes: usize, chapters: usize, words: usize) -> String {
    let novel = db::novel::create_novel(conn, title, "性能基准用合成数据").unwrap();
    let mut made = 0usize;

    for v in 0..volumes {
        let vol =
            db::volume::create_volume(conn, &novel.id, &format!("第{}卷", v + 1), "").unwrap();
        for c in 0..chapters {
            // 确定性正文：不含随机数，保证每次运行的数据完全一致，
            // 否则 P50/P95 的波动会掩盖真实的代码改动影响。
            let body: String = (0..words)
                .map(|i| char::from_u32(0x4E00 + ((i + c + v) % 500) as u32).unwrap_or('字'))
                .collect();
            // 用 create_chapter 的返回值直接拿到 id —— 造数据本身也必须是 O(n)，
            // 否则后面的规模效应报告会被造数据的开销污染。
            let chapter = db::chapter::create_chapter(
                conn,
                &novel.id,
                &vol.id,
                &format!("第{}章", made + 1),
                "",
            )
            .unwrap();
            db::chapter::save_chapter_content(
                conn,
                &chapter.id,
                &format!("<p>{}</p>", body),
                &body,
                words as i32,
            )
            .unwrap();
            made += 1;
        }
    }
    novel.id
}

/// 单个作品内的章节总数。
fn chapter_count(conn: &Connection, novel_id: &str) -> usize {
    db::chapter::list_all_chapter_headers_for_novel(conn, novel_id)
        .unwrap()
        .len()
}

// ---------------------------------------------------------------------------
// 基线 1：小作品（作者日常最常见的规模）
// ---------------------------------------------------------------------------

#[test]
#[ignore = "性能基准，默认不随 cargo test 运行；用 --ignored 显式触发"]
fn perf_small_novel_baseline() {
    let dir = workdir("small");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    let nid = seed(&conn, "小作品", 2, 25, 2000);
    let ds = format!("{} 章", chapter_count(&conn, &nid));

    Bench::new("作品章节目录加载", ds.clone()).measure(|| {
        db::chapter::list_all_chapter_headers_for_novel(&conn, &nid).unwrap()
    });
    Bench::new("全书字数统计", ds.clone()).measure(|| {
        conn.query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM chapter WHERE novel_id = ?1",
            rusqlite::params![nid],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    });
    Bench::new("打开单章正文", ds.clone()).measure(|| {
        let id = db::chapter::list_all_chapter_headers_for_novel(&conn, &nid).unwrap()[0].id.clone();
        db::chapter::get_chapter(&conn, &id).unwrap()
    });
}

// ---------------------------------------------------------------------------
// 基线 2：数百章 —— 长篇网文的主流规模
// ---------------------------------------------------------------------------

#[test]
#[ignore = "性能基准，默认不随 cargo test 运行；用 --ignored 显式触发"]
fn perf_hundreds_of_chapters_baseline() {
    let dir = workdir("hundreds");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    let nid = seed(&conn, "数百章作品", 12, 30, 3000);
    let ds = format!("{} 章", chapter_count(&conn, &nid));

    Bench::new("作品章节目录加载", ds.clone()).measure(|| {
        db::chapter::list_all_chapter_headers_for_novel(&conn, &nid).unwrap()
    });
    Bench::new("全书字数统计", ds.clone()).measure(|| {
        conn.query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM chapter WHERE novel_id = ?1",
            rusqlite::params![nid],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    });
}

// ---------------------------------------------------------------------------
// 基线 3：千章级 + 长章节 —— 本仓库声称要支撑的上限场景
// ---------------------------------------------------------------------------

#[test]
#[ignore = "性能基准，默认不随 cargo test 运行；用 --ignored 显式触发"]
fn perf_thousand_chapter_long_paragraph_baseline() {
    let dir = workdir("thousand");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    let nid = seed(&conn, "千章长篇", 50, 20, 4000);
    let ds = format!("{} 章 x 4000 字", chapter_count(&conn, &nid));
    println!("[PERF] 数据集：{}（合成数据，非真实用户作品）", ds);

    let headers =
        Bench::new("作品章节目录加载", ds.clone()).measure(|| {
            db::chapter::list_all_chapter_headers_for_novel(&conn, &nid).unwrap()
        });
    let first_id = headers[0].id.clone();

    // 目录加载是章切换与自动保存统计的前置，单独看它的规模效应。
    println!(
        "[PERF] 目录条目数 {}，平均每条 {:.1} 字节（序列化开销）",
        headers.len(),
        (headers.iter().map(|h| h.title.len()).sum::<usize>() as f64) / headers.len().max(1) as f64
    );

    Bench::new("打开单章正文（4000 字）", ds.clone()).measure(|| {
        db::chapter::get_chapter(&conn, &first_id).unwrap()
    });

    Bench::new("保存单章（自动保存路径）", ds.clone()).measure(|| {
        db::chapter::save_chapter_content(&conn, &first_id, "<p>x</p>", "x", 1).unwrap()
    });

    Bench::new("全书字数统计", ds.clone()).measure(|| {
        conn.query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM chapter WHERE novel_id = ?1",
            rusqlite::params![nid],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    });
}

// ---------------------------------------------------------------------------
// 基线 4：全文搜索 —— LIKE '%term%' 无法走索引，规模效应最明显的操作
// ---------------------------------------------------------------------------

#[test]
#[ignore = "性能基准，默认不随 cargo test 运行；用 --ignored 显式触发"]
fn perf_fulltext_search_baseline() {
    let dir = workdir("search");
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    let nid = seed(&conn, "搜索基准", 30, 20, 3000);
    let ds = format!("{} 章", chapter_count(&conn, &nid));

    // 合成正文里每个汉字都会出现，因此任取一个字都能命中全部章节 ——
    // 这是最坏情况，也正是长篇作者反复做的那类操作。
    for term in ["人", "国", "天"] {
        Bench::new(&format!("全书搜索「{term}」（全命中）"), ds.clone()).measure(|| {
            db::chapter::find_chapters_mentioning(&conn, &nid, term).unwrap()
        });
    }
    Bench::new("全书搜索（零命中）", ds.clone()).measure(|| {
        db::chapter::find_chapters_mentioning(&conn, &nid, "龘").unwrap()
    });
}

// ---------------------------------------------------------------------------
// 基线 5：大量图片资产 —— 图库与存储对账
// ---------------------------------------------------------------------------

#[test]
#[ignore = "性能基准，默认不随 cargo test 运行；用 --ignored 显式触发"]
fn perf_many_assets_baseline() {
    let dir = workdir("assets");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let nid = db::novel::create_novel(&conn, "图片基准", "").unwrap().id;

    // 1x1 PNG
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    for i in 0..200 {
        let name = format!("图{}", i);
        easy_writing_lib::asset::save_image(
            &conn,
            &dir,
            &nid,
            &name,
            "image/png",
            png,
        )
        .unwrap();
    }
    let ds = "200 张图".to_string();

    Bench::new("图库资产列表", ds.clone()).measure(|| {
        easy_writing_lib::asset::list_assets(&conn, &nid).unwrap()
    });
    Bench::new("存储对账（孤儿文件扫描）", ds.clone()).measure(|| {
        easy_writing_lib::asset::scan_storage(&conn, &dir).unwrap()
    });
}

// ---------------------------------------------------------------------------
// 规模效应报告：把「随章节数增长」这件事量化
// ---------------------------------------------------------------------------

#[test]
#[ignore = "性能基准，默认不随 cargo test 运行；用 --ignored 显式触发"]
fn perf_scaling_report() {
    // 目的不是给出绝对耗时，而是回答一个问题：
    // **章节目录加载是否随章节数线性增长？**
    // 如果某次改动引入了「每章一次查询」这类 N+1，
    // 这里会立刻从「随规模温和上升」变成「陡增」。
    println!("\n[PERF] === 规模效应：章节目录加载 ===");
    println!("[PERF] {:>8}  {:>12}  {:>16}", "章节数", "耗时(ms)", "每章(µs)");
    for (vols, chaps) in [(5usize, 10usize), (10, 20), (20, 20), (40, 25)] {
        let dir = workdir(&format!("scale_{}_{}", vols, chaps));
        let pool = db::init_db(dir).unwrap();
        let conn = pool.get().unwrap();
        let nid = seed(&conn, "规模测试", vols, chaps, 500);
        let _n = chapter_count(&conn, &nid);

        // 预热一次，避免把首次页读算进来
        let _ = db::chapter::list_all_chapter_headers_for_novel(&conn, &nid).unwrap();

        let t0 = Instant::now();
        let headers = db::chapter::list_all_chapter_headers_for_novel(&conn, &nid).unwrap();
        let ms = t0.elapsed().as_secs_f64() * 1000.0;

        println!(
            "[PERF] {:>8}  {:>12.3}  {:>16.2}",
            headers.len(),
            ms,
            ms * 1000.0 / headers.len().max(1) as f64
        );
    }
    println!("[PERF] 若「每章(µs)」随规模上升，说明存在 N+1 查询或重复加载。\n");
}
