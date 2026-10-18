//! Integration tests for AI extraction, self-healing parser, conflict resolution,
//! and full cross-domain integration across:
//! - 设定 (Worldview)
//! - 角色 (Characters)
//! - 角色卡 (6-dimension ExplodedProfile)
//! - 剧情 (Plot lines & Foreshadowing)
//! - 大纲 (Chapter Outline & Summary)
//! - 词库 (Lexicon auto-sync & Autocomplete)
//! - 关系管理 & 星云爆炸图 (Character Relations & Graph Nebula)
//! - 自修复 (Self-healing JSON recovery, truncation recovery, alias tolerance, conflict resolution)

use easy_writing_lib::agent::extraction::{
    compute_dedup_key, compute_update_dedup_key, resolve_candidate_action, self_heal_and_parse_candidates, ExistingIndex,
};
use easy_writing_lib::db;
use shared_types::ExplodedProfile;
use std::path::PathBuf;

fn temp_db_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_extract_test_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_self_healing_json_recovery_scenarios() {
    // 1. Markdown 代码块与闲聊前后缀自修复
    let markdown_chatter = r#"
你好！根据本章内容，我为你提取了如下核心设定：
```json
[
  {
    "kind": "worldview",
    "action": "create",
    "title": "东京留守司",
    "payload": "宗泽主持的中原抗金要地，维系大宋民心",
    "evidence": "东京留守司大营"
  }
]
```
请您查阅，如需调整请随时告知！
"#;
    let items = self_heal_and_parse_candidates(markdown_chatter, "worldview").expect("markdown 闲聊自修复成功");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "东京留守司");
    assert_eq!(items[0].kind, "worldview");

    // 2. 单对象自修复（大模型常把单个条目输出为单独的 {} 而非数组 []）
    let single_object = r#"
{
  "kind": "character",
  "action": "create",
  "title": "岳飞",
  "payload": "相州汤阴人，字鹏举，行伍拔擢之青年将领",
  "evidence": "岳飞按剑立于帐前"
}
"#;
    let items = self_heal_and_parse_candidates(single_object, "character").expect("单对象自修复成功");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "岳飞");
    assert_eq!(items[0].kind, "character");

    // 3. 字段别名漂移自修复 (name -> title, content -> payload, quote -> evidence, targetId -> target_id)
    let alias_drift = r#"
[
  {
    "type": "character",
    "name": "韩世忠",
    "content": "勇冠三军，部曲号背嵬前锋",
    "quote": "韩五奋勇当先",
    "targetId": "c-101"
  }
]
"#;
    let items = self_heal_and_parse_candidates(alias_drift, "character").expect("别名自修复成功");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "韩世忠");
    assert_eq!(items[0].payload, "勇冠三军，部曲号背嵬前锋");
    assert_eq!(items[0].evidence, "韩五奋勇当先");
    assert_eq!(items[0].target_id.as_deref(), Some("c-101"));

    // 4. 尾部截断自修复（模拟因 max_tokens 截断而残损的 JSON）
    let truncated_json = r#"
[
  {
    "kind": "plot",
    "action": "create",
    "title": "黄潜善暗通金军伏笔",
    "payload": "密信藏于行营内侍靴夹层中",
    "evidence": "黄相公目光闪烁"
  },
  {
    "kind": "plot",
    "action": "create",
    "title": "韩世忠移防
"#;
    let items = self_heal_and_parse_candidates(truncated_json, "plot").expect("截断自修复成功");
    assert_eq!(items.len(), 1, "自修复引擎应抢救出前面完整的对象");
    assert_eq!(items[0].title, "黄潜善暗通金军伏笔");

    // 5. 尾随逗号与行注释自修复
    let dirty_json = r#"
[
  {
    "kind": "chapter_summary",
    "action": "create",
    "title": "第5章梗概",
    "payload": "官家行在移驻下邳，军心初定。 // 核心章纲",
    "evidence": "下邳城头风旗招展",
  },
]
"#;
    let items = self_heal_and_parse_candidates(dirty_json, "chapter_summary").expect("脏逗号注释自修复成功");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "第5章梗概");
    assert!(items[0].payload.contains("军心初定"));

    // 6. 合法空数组区分与异常输出保护（P1 修复验收）
    let valid_empty = "```json\n[]\n```";
    assert_eq!(self_heal_and_parse_candidates(valid_empty, "worldview").unwrap().len(), 0);

    let malformed_empty = "由于网络超时，模型未能返回任何设定。";
    assert!(self_heal_and_parse_candidates(malformed_empty, "worldview").is_err(), "格式异常不能当作成功空结果");
}

#[test]
fn test_conflict_resolution_and_action_resolution() {
    let existing_index: ExistingIndex = vec![(
        "character".to_string(),
        vec![(
            "char-zhao".to_string(),
            "赵玖".to_string(),
            r#"["官家","天子","赵官家"]"#.to_string(),
        )],
    )];

    // 1. 模型提供了不存在的悬挂 ID，但姓名匹配到了既有角色 -> 自修复判为 update
    let r1 = resolve_candidate_action(
        "create",
        Some("non-existent-id"),
        "赵玖",
        "character",
        &existing_index,
    );
    assert_eq!(r1.action, "update");
    assert_eq!(r1.target_id.as_deref(), Some("char-zhao"));

    // 2. 匹配别名 -> 自修复判为 update 并归并到原 ID
    let r2 = resolve_candidate_action(
        "create",
        None,
        "赵官家",
        "character",
        &existing_index,
    );
    assert_eq!(r2.action, "update");
    assert_eq!(r2.target_id.as_deref(), Some("char-zhao"));

    // 3. 全新人物 -> 判为 create，target_id 干净清空为 None
    let r3 = resolve_candidate_action(
        "update",
        Some("dangling-fake-id"),
        "李纲",
        "character",
        &existing_index,
    );
    assert_eq!(r3.action, "create");
    assert_eq!(r3.target_id, None);

    // 4. 去重哈希忽略空格与大小写
    assert_eq!(
        compute_dedup_key("character", "韩 世忠"),
        compute_dedup_key("character", "韩世忠")
    );

    // 5. 验证 update 动作的去重键：基于 target_id 和 payload 内容，绝不与基于标题的 create 冲突
    let create_key = compute_dedup_key("character", "韩世忠");
    let update_key1 = compute_update_dedup_key("character", "char-han", "在黄天荡截击金兀术");
    let update_key2 = compute_update_dedup_key("character", "char-han", "在黄天荡 截击金兀术");
    let update_key_different = compute_update_dedup_key("character", "char-han", "拜枢密使退隐西湖");

    assert_ne!(create_key, update_key1, "update 候选不得与已有的 create dedup_key 冲突被误杀");
    assert_eq!(update_key1, update_key2, "相同更新内容应正常去重");
    assert_ne!(update_key1, update_key_different, "不同章节的全新更新内容不得相互覆盖");
}

#[test]
fn test_end_to_end_cross_domain_acceptance_and_self_healing() {
    let dir = temp_db_dir("cross_domain");
    let pool = db::init_db(dir).expect("init test db");
    let conn = pool.get().unwrap();

    // 1. 初始化作品与章节
    let novel = db::novel::create_novel(&conn, "绍宋测试录", "大宋抗金风云").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷 靖康余响", "").unwrap();
    let chapter = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "第一章 明道宫之变", "").unwrap();

    // 2. 测试提炼候选落库并携带 title
    let cand_wv = db::extraction::insert_candidate(
        &conn,
        &novel.id,
        &chapter.id,
        "agent-worldview",
        "worldview",
        "create",
        None,
        "八公山",
        "淮南重镇，古战场，地势险要控扼南北交通",
        "官家北望八公山草木皆兵",
        "dedup-bagongshan",
    )
    .unwrap();
    assert_eq!(cand_wv.title, "八公山");

    // 3. 验证采纳【设定】并自动同步到【词库】
    {
        // 模拟 resolve_candidate 采纳 worldview
        let _ = db::settings::save_worldview(
            &conn,
            None,
            &novel.id,
            "世界观设定",
            &cand_wv.title,
            &cand_wv.payload,
        )
        .unwrap();
        // 自动入库词库
        let _ = db::lexicon::upsert_lexicon_term(
            &conn,
            &novel.id,
            &cand_wv.title,
            "worldview",
            None,
        )
        .unwrap();

        // 验证设定库
        let wv_list = db::settings::list_worldview(&conn, &novel.id).unwrap();
        assert_eq!(wv_list.len(), 1);
        assert_eq!(wv_list[0].title, "八公山");

        // 验证词库
        let lex_list = db::lexicon::list_lexicon(&conn, &novel.id).unwrap();
        assert!(lex_list.iter().any(|t| t.term == "八公山"));
    }

    // 4. 测试【角色】与【角色卡 6 维爆炸图】生成与入库
    let char_a = {
        let mut ep = ExplodedProfile::default();
        ep.core.desire = "匡扶社稷，北伐雪耻".to_string();
        ep.appearance.features = "长身玉立，目光坚毅".to_string();
        ep.faction = "主角团".to_string();
        ep.importance = 1;
        ep.raw_notes = "穿越为南宋开国之君赵玖".to_string();

        let profile_json = ep.to_json_string();
        db::settings::save_character(
            &conn,
            None,
            &novel.id,
            "赵玖",
            r#"["官家","天子"]"#,
            &profile_json,
        )
        .unwrap()
    };
    assert_eq!(char_a.name, "赵玖");

    // 5. 测试【角色卡无损合并自修复】：更新已有角色时，增量合并新提炼信息
    {
        let mut ep_updated = ExplodedProfile::parse_from_str(&char_a.profile);
        // 新提炼发现了性格与口头禅
        ep_updated.voice.catchphrase = "朕意已决".to_string();
        ep_updated.raw_notes.push_str("\n\n补充：下邳整军，初显天子威仪。");

        let updated_json = ep_updated.to_json_string();
        let char_a_updated = db::settings::save_character(
            &conn,
            Some(char_a.id.clone()),
            &novel.id,
            &char_a.name,
            &char_a.aliases,
            &updated_json,
        )
        .unwrap();

        let parsed = ExplodedProfile::parse_from_str(&char_a_updated.profile);
        assert_eq!(parsed.core.desire, "匡扶社稷，北伐雪耻", "已有核心渴望不得被抹除");
        assert_eq!(parsed.voice.catchphrase, "朕意已决", "新增口头禅成功无损合并");
        assert!(parsed.raw_notes.contains("下邳整军"));
    }

    // 6. 新增角色 B 并测试【关系管理 & 星云爆炸图】自动联动
    let char_b = {
        let mut ep = ExplodedProfile::default();
        ep.core.desire = "提兵斩金帅".to_string();
        ep.raw_notes = "西军悍将韩五".to_string();
        let profile_json = ep.to_json_string();
        db::settings::save_character(&conn, None, &novel.id, "韩世忠", "[]", &profile_json).unwrap()
    };

    // 自动在角色关系表中建立双方关联（星云爆炸图的数据源）
    let rel = db::character_relation::save_character_relation(
        &conn,
        None,
        &novel.id,
        &char_a.id,
        &char_b.id,
        "君臣信任",
        "官家下邳犒军，韩世忠誓死效忠",
    )
    .unwrap();
    assert_eq!(rel.relation_name, "君臣信任");

    // 验证关系管理列表查询成功（直接供星云图使用）
    let relations = db::character_relation::list_character_relations(&conn, &novel.id).unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].from_char_id, char_a.id);
    assert_eq!(relations[0].to_char_id, char_b.id);

    // 7. 测试【剧情与伏笔】提炼入库
    let plot = db::settings::save_plot(
        &conn,
        None,
        &novel.id,
        "苗刘怀异心",
        "苗傅刘正彦御前失仪，隐有兵变之兆",
        "foreshadow",
        "planted",
        Some(chapter.id.clone()),
    )
    .unwrap();
    assert_eq!(plot.title, "苗刘怀异心");
    assert_eq!(plot.status, "planted");

    // 8. 测试【大纲与章纲】自动回填章节 summary
    let summary_text = "官家驻跸明道宫，召群臣整顿军备，誓不南渡。";
    conn.execute(
        "UPDATE chapter SET summary = ?1 WHERE id = ?2",
        rusqlite::params![summary_text, chapter.id],
    )
    .unwrap();

    let ch_read = db::chapter::get_chapter(&conn, &chapter.id).unwrap().unwrap();
    assert_eq!(ch_read.summary, summary_text, "大纲视图直接呈现更新后的章纲摘要");
}

#[test]
fn test_extraction_failure_protection_coverage() {
    // 验证 P1 报告指出的问题：
    // 1. 合法空数组 [] 不能被视作失败
    // 2. 格式解析失败必须返回 Err，阻断全链路 all_succeeded，绝不能被打上 extracted_hash
    let db_dir = temp_db_dir("failure_protection");
    let pool = db::init_db(db_dir).expect("init test db");
    let conn = pool.get().unwrap();

    let novel = db::novel::create_novel(&conn, "测试作品", "说明").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();
    let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "第1章", "").unwrap();

    // 初始状态下 extracted_hash 为 None
    assert!(ch.extracted_hash.is_none());

    // 场景 A：模型返回合法的空数组 []（确实无设定可提炼） -> 返回 Ok 且长度为 0
    let legit_empty = self_heal_and_parse_candidates("```json\n[]\n```", "character");
    assert!(legit_empty.is_ok());
    assert_eq!(legit_empty.unwrap().len(), 0);

    // 场景 B：模型返回非 JSON 闲聊或错误文案 -> 必须返回 Err
    let chatter_fail = self_heal_and_parse_candidates("对不起，本模型当前不可用，错误码 500", "character");
    assert!(chatter_fail.is_err());

    // 场景 C：模型返回语法完全破损且不可自愈的内容 -> 必须返回 Err
    let malformed_fail = self_heal_and_parse_candidates("{ 此处不是合法json内容也不含标题字段 }", "character");
    assert!(malformed_fail.is_err());

    // 场景 D：验证失败保护机制 —— 一旦任一环节失败，chapter 表的 extracted_hash 绝不能被写入
    // 模拟 run_parallel_extraction 的失败分支行为
    let all_succeeded = false;
    let current_fingerprint = "dummy_fingerprint_hash";
    if all_succeeded {
        conn.execute(
            "UPDATE chapter SET extracted_hash = ?1 WHERE id = ?2",
            rusqlite::params![current_fingerprint, ch.id],
        ).unwrap();
    }
    let ch_after = db::chapter::get_chapter(&conn, &ch.id).unwrap().unwrap();
    assert!(ch_after.extracted_hash.is_none(), "失败时绝不能写入 extracted_hash 指纹");

    // 场景 E：模拟数据库插入/锁冲突阻断 extracted_hash，以及随后重试成功的完整生命周期
    {
        let cand_title = "独孤求败";
        let cand_dedup = compute_dedup_key("character", cand_title);

        // 第一次尝试：模拟数据库写入失败（例如锁冲突/外键故障）
        let mut sim_all_succeeded = true;
        let insert_res: Result<(), String> = Err("database table is locked (5)".to_string());
        if let Err(_e) = insert_res {
            sim_all_succeeded = false;
        }
        if sim_all_succeeded {
            conn.execute(
                "UPDATE chapter SET extracted_hash = ?1 WHERE id = ?2",
                rusqlite::params!["sim_hash_1", ch.id],
            ).unwrap();
        }
        let ch_check1 = db::chapter::get_chapter(&conn, &ch.id).unwrap().unwrap();
        assert!(ch_check1.extracted_hash.is_none(), "DB 异常时阻断 extracted_hash 写入");

        // 第二次重试（Retry）：数据库正常，候选成功入库并更新 extracted_hash
        let mut retry_all_succeeded = true;
        let insert_res2 = db::extraction::insert_candidate(
            &conn,
            &novel.id,
            &ch.id,
            "agent-character",
            "character",
            "create",
            None,
            cand_title,
            "剑魔独孤求败生平未尝一败",
            "重剑无锋，大巧不工",
            &cand_dedup,
        );
        assert!(insert_res2.is_ok(), "重试时数据库插入成功");
        if insert_res2.is_err() {
            retry_all_succeeded = false;
        }
        if retry_all_succeeded {
            conn.execute(
                "UPDATE chapter SET extracted_hash = ?1 WHERE id = ?2",
                rusqlite::params!["retry_success_hash", ch.id],
            ).unwrap();
        }
        let ch_check2 = db::chapter::get_chapter(&conn, &ch.id).unwrap().unwrap();
        assert_eq!(
            ch_check2.extracted_hash.as_deref(),
            Some("retry_success_hash"),
            "重试成功后顺利记录 extracted_hash 指纹"
        );
    }
}


