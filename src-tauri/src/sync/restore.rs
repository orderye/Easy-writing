//! WebDAV 整库恢复。
//!
//! ## 两阶段结构
//!
//! 恢复被拆成**预检**与**执行**两个独立的 Tauri command：
//!
//! - [`run_restore_preflight`]：下载备份包与清单 → 解压 → 完整性/schema/计数预检
//!   → 清单交叉校验 → 返回结构化结论。此阶段**不触碰现有数据库**。
//! - [`run_consistency_restore`]：凭预检返回的 `preflight_token` 找回同一份候选备份，
//!   **重新跑一遍全部校验**，才允许进入覆盖阶段。
//!
//! 这样做的直接理由是 P0-3：旧实现在覆盖完成之后才把「这是空库」写进结果，
//! 作者根本没有机会在覆盖前拒绝。拆分后，「用空库替换现有作品」必须由前端
//! 在看过预检结论后显式确认。
//!
//! ## token 是哈希，不是路径
//!
//! `preflight_token` 是候选备份压缩包的 SHA-256，候选文件按
//! `<token>.db.gz` 存放在系统临时目录下的应用专属子目录中。
//! 执行阶段**只接受 token、不接受路径**，并会重新计算文件哈希与 token 比对。
//!
//! 两点收益：一是「确认 A 却恢复 B」在结构上不可能发生（内容一变哈希就不符）；
//! 二是前端无法通过传路径让后端去恢复任意本地文件。
//!
//! ## 回滚
//!
//! 覆盖前用 SQLite Backup API 落一份 `pre_restore_*.db` 安全快照。
//! 数据库覆盖失败、或本地配置写回失败，都会从快照回滚，并按
//! [`RestoreOutcome`] 明确区分「已回滚」与「回滚也失败」。
//! 快照只有在恢复达到完全成功时才清理。

use crate::db::DbPool;
use crate::sync::manifest::{self, BackupManifest};
use crate::sync::webdav::{RemoteFetch, WebDavClient};
use chrono::Local;
use flate2::read::GzDecoder;
pub use shared_types::{
    BackupFileInfo, DanglingKeyInfo, ManifestCheck, RestoreIssue, RestoreOutcome, RestorePreflight,
    RestoreReport, RestoreSummary,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 预检候选文件的存放目录名（位于系统临时目录下，仅本应用使用）。
const CANDIDATE_DIR_NAME: &str = "ew_restore_candidates";

/// 候选文件保留时长。超过即视为上一次恢复流程的残留。
const CANDIDATE_TTL_SECS: u64 = 6 * 60 * 60;

/// 本机需要保留、不被远端全库覆盖的界面设置键。
const PRESERVED_SETTING_KEYS: &str = "'theme', 'left_panel_visible', 'right_panel_visible'";

pub async fn list_remote_backups(
    client: &WebDavClient,
    remote_dir: &str,
) -> Result<Vec<BackupFileInfo>, String> {
    let clean_dir = remote_dir.trim_matches('/').to_string();
    client.list_files(&clean_dir).await
}

// ---------------------------------------------------------------------------
// 候选备份的存放与定位
// ---------------------------------------------------------------------------

fn candidate_dir() -> PathBuf {
    std::env::temp_dir().join(CANDIDATE_DIR_NAME)
}

/// token 必须是 64 位十六进制（SHA-256）。
///
/// 这条校验是路径安全的一部分：token 会被拼进临时目录下的文件名，
/// 放宽它等于允许调用方指定任意路径。
pub fn is_preflight_token(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// 预检候选备份包的落盘位置（`<临时目录>/ew_restore_candidates/<token>.db.gz`）。
///
/// 公开的原因：token 会被拼进文件路径，因此「token 必须是 64 位十六进制」
/// 是一条**安全约束**而不只是格式检查。测试需要能独立复算这个路径，
/// 才能证明非法 token 无法被用来读写应用目录之外的文件。
pub fn candidate_gz_path(token: &str) -> Result<PathBuf, String> {
    if !is_preflight_token(token) {
        return Err("预检凭据格式非法（应为 64 位十六进制 SHA-256）".to_string());
    }
    Ok(candidate_dir().join(format!("{token}.db.gz")))
}

/// 清理过期的候选备份残留。
///
/// **只删除严格符合 `<64hex>.db.gz[.json]` 命名的文件**，且必须已超过 TTL。
/// 临时目录里可能有别的东西，一律不碰。清理失败只记录，不影响恢复。
fn sweep_stale_candidates() {
    let dir = candidate_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_candidate_name(name) {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        let Ok(age) = now.duration_since(mtime) else { continue };
        if age > Duration::from_secs(CANDIDATE_TTL_SECS) {
            let _ = fs::remove_file(&path);
        }
    }
}

fn is_candidate_name(name: &str) -> bool {
    let stem = name
        .strip_suffix(".db.gz.json")
        .or_else(|| name.strip_suffix(".db.gz"));
    match stem {
        Some(s) => is_preflight_token(s),
        None => false,
    }
}

fn remove_candidate(token: &str) {
    if let Ok(gz) = candidate_gz_path(token) {
        let _ = fs::remove_file(&gz);
        let _ = fs::remove_file(&manifest::sidecar_path_for(&gz));
    }
}

/// 解压 `.db.gz`，并在超过上限时立即中止。
///
/// 上限保护是必需的：一个 `.gz` 的解压体积可以远超其自身大小（压缩炸弹），
/// 而备份来自远端，其内容不受本机控制。分块读取，不把整包读进内存后再判断。
fn gunzip_capped(gz_bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut decoder = GzDecoder::new(gz_bytes);
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0usize;
    loop {
        let n = decoder
            .read(&mut buf)
            .map_err(|e| format!("解压 WebDAV 备份包失败: {e}"))?;
        if n == 0 {
            break;
        }
        total += n;
        if total > crate::sync::backup::MAX_RESTORE_DECOMPRESSED_BYTES {
            return Err(format!(
                "备份解压后超过 {} MiB 上限，已中止恢复（可能是损坏文件或压缩炸弹）。",
                crate::sync::backup::MAX_RESTORE_DECOMPRESSED_BYTES / (1024 * 1024)
            ));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

/// 解压候选包到一个独立的临时数据库文件。
fn decompress_candidate_to_db(gz_bytes: &[u8]) -> Result<(PathBuf, Vec<u8>), String> {
    let decompressed = gunzip_capped(gz_bytes)?;
    let path = std::env::temp_dir().join(format!(
        "easy_writing_candidate_{}.db",
        uuid::Uuid::new_v4()
    ));
    fs::write(&path, &decompressed).map_err(|e| format!("写入解压临时文件失败: {e}"))?;
    Ok((path, decompressed))
}

// ---------------------------------------------------------------------------
// 阶段一：预检（不触碰现有数据库）
// ---------------------------------------------------------------------------

/// 下载并校验候选备份，返回覆盖数据库之前作者需要看到的全部事实。
///
/// 任何一步失败都返回 `Err`，且**现有数据库不会被改动**。
pub async fn run_restore_preflight(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    backup_filename: &str,
    app_data_dir: PathBuf,
) -> Result<RestorePreflight, String> {
    crate::sync::backup::validate_backup_filename(backup_filename)?;

    let clean_dir = remote_dir.trim_matches('/').to_string();
    let remote_backup_path = format!("{}/{}", clean_dir, backup_filename);
    // 清单路径由已校验的备份文件名拼接而来，不接受任何外部输入。
    let remote_manifest_path = format!("{}.json", remote_backup_path);

    sweep_stale_candidates();

    // 1. 下载备份包
    let gz_bytes = client.download_file(&remote_backup_path).await?;
    let gz_len = gz_bytes.len() as u64;
    let token = manifest::sha256_hex(&gz_bytes);

    // 2. 下载清单：必须区分「确实不存在」与「没问到」
    let mut remote_manifest: Option<BackupManifest> = None;
    match client.download_file_optional(&remote_manifest_path).await? {
        RemoteFetch::NotFound => {}
        RemoteFetch::Found(bytes) => {
            // 清单存在但解析不了 ≠ 清单不存在。
            // 折叠这两者等于让「损坏/被篡改的清单」把恢复降级到只校验结构的兼容分支。
            remote_manifest = Some(serde_json::from_slice(&bytes).map_err(|e| {
                format!(
                    "远端备份清单存在但无法解析，已中止恢复（不会被当成历史无清单备份）：{e}"
                )
            })?);
        }
    }

    // 3. 解压 + 落候选文件（按 token 命名，供执行阶段凭哈希找回）
    let cand_dir = candidate_dir();
    fs::create_dir_all(&cand_dir).map_err(|e| format!("创建预检候选目录失败: {e}"))?;
    let cand_gz = candidate_gz_path(&token)?;
    crate::sync::backup::atomic_write(&cand_gz, &gz_bytes)?;
    if let Some(m) = &remote_manifest {
        // 本地留存清单副本，执行阶段凭它重新校验，无需二次访问远端。
        manifest::write_sidecar(&cand_gz, m).map_err(|e| format!("保存清单副本失败: {e}"))?;
    }

    let (cand_db, _) = match decompress_candidate_to_db(&gz_bytes) {
        Ok(r) => r,
        Err(e) => {
            // 解压失败（典型是上传/下载被截断）同样要告诉作者：现有数据没被动过。
            remove_candidate(&token);
            return Err(format!("恢复中止：备份包无法解压，原数据未被改动。{e}"));
        }
    };

    // 4. 结构预检：文件头 / integrity_check / schema 版本 / 实体计数
    let preflight = match crate::sync::backup::preflight_backup(&cand_db) {
        Ok(r) => r,
        Err(e) => {
            let _ = fs::remove_file(&cand_db);
            remove_candidate(&token);
            return Err(format!("恢复中止：备份未通过完整性检查，原数据未被改动。{e}"));
        }
    };

    // 5. 清单交叉校验（有清单时）
    let mut asset_problems: Vec<String> = Vec::new();
    let manifest_check = match &remote_manifest {
        None => ManifestCheck::Absent,
        Some(m) => {
            let verification = match manifest::verify_manifest(
                m,
                &cand_db,
                Some(gz_len),
                Some(&token),
                Some(&app_data_dir),
            ) {
                Ok(v) => v,
                Err(e) => {
                    let _ = fs::remove_file(&cand_db);
                    remove_candidate(&token);
                    return Err(format!("恢复中止：无法完成清单校验，原数据未被改动。{e}"));
                }
            };
            if !verification.is_ok() {
                let _ = fs::remove_file(&cand_db);
                remove_candidate(&token);
                return Err(format!(
                    "恢复中止：备份清单与备份内容不一致，原数据未被改动。\n- {}",
                    verification.fatal.join("\n- ")
                ));
            }
            asset_problems.extend(verification.notes);
            ManifestCheck::Verified {
                asset_count: m.assets.len() as u64,
            }
        }
    };

    // 6. 资产盘点（只读本地，不联网；缺失的在执行阶段回填）
    let assets = match preflight_assets(&cand_db, &app_data_dir) {
        Ok(a) => a,
        Err(e) => {
            let _ = fs::remove_file(&cand_db);
            remove_candidate(&token);
            return Err(format!("恢复中止：无法盘点备份中的图片素材，原数据未被改动。{e}"));
        }
    };
    asset_problems.extend(assets.problems);

    // 7. 本机当前库状态：决定是否需要「空库覆盖」二次确认
    let local_novel_count = match local_novel_count(&pool) {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_file(&cand_db);
            remove_candidate(&token);
            return Err(e);
        }
    };

    // 8. 来源身份（meta sidecar，可选）。
    //
    //    它回答的是「这份备份是哪台设备写的」，供跨设备保护使用。
    //    没有 meta 的历史备份照常可恢复（兼容路径），只是作者看不到来源。
    let mut notes: Vec<String> = Vec::new();
    let remote_meta_path = format!("{}.meta.json", remote_backup_path);
    let mut source_meta: Option<crate::sync::device::BackupMeta> = None;
    match client.download_file_optional(&remote_meta_path).await {
        Ok(RemoteFetch::NotFound) => {}
        Ok(RemoteFetch::Found(bytes)) => match serde_json::from_slice::<crate::sync::device::BackupMeta>(
            &bytes,
        ) {
            Ok(m) => {
                // meta 记录的哈希必须与实际下载到的包一致。
                // 不一致说明这份来源信息不属于这个备份包 —— 可能是文件被替换，
                // 也可能是 meta 被错配到了别的备份上。两种情况都必须拦下。
                if !m.sha256.is_empty() && !m.sha256.eq_ignore_ascii_case(&token) {
                    let _ = fs::remove_file(&cand_db);
                    remove_candidate(&token);
                    return Err(format!(
                        "恢复中止：备份的来源信息（meta）记录的哈希与实际下载内容不符，\
                         这份来源信息不属于该备份包。原数据未被改动。"
                    ));
                }
                source_meta = Some(m);
            }
            Err(e) => {
                // meta 解析失败不阻断恢复：清单的 sha256 才是完整性闸门。
                // 但必须让作者知道「来源信息读不出来」。
                notes.push(format!("备份的来源信息（meta）无法解析，已忽略：{e}"));
            }
        },
        Err(e) => {
            // 网络错误不是「文件不存在」，但同样不该阻断恢复。
            notes.push(format!("无法读取备份的来源信息（meta），已忽略：{e}"));
        }
    }

    let local_device_id = crate::sync::device::load(&app_data_dir).device_id;
    let source_device_id = source_meta.as_ref().map(|m| m.device_id.clone());
    let source_device_name = source_meta.as_ref().map(|m| m.device_name.clone());
    let is_from_other_device = source_device_id
        .as_ref()
        .map(|id| *id != local_device_id)
        .unwrap_or(false);

    let _ = fs::remove_file(&cand_db);

    Ok(RestorePreflight {
        filename: backup_filename.to_string(),
        preflight_token: token,
        schema_version: preflight.schema_version,
        novel_count: preflight.novel_count,
        volume_count: preflight.volume_count,
        chapter_count: preflight.chapter_count,
        word_count: preflight.word_count,
        is_empty_library: preflight.is_empty_library,
        local_novel_count,
        local_is_empty: local_novel_count == 0,
        requires_empty_overwrite_confirm: local_novel_count > 0 && preflight.is_empty_library,
        manifest: manifest_check,
        integrity_ok: preflight.is_valid,
        compressed_size_bytes: gz_len,
        asset_total: assets.total,
        asset_present_locally: assets.present_locally,
        asset_missing_locally: assets.missing_locally,
        asset_problems,
        source_device_id,
        source_device_name,
        is_from_other_device,
        notes,
    })
}

fn local_novel_count(pool: &DbPool) -> Result<i64, String> {
    let conn = pool.get().map_err(|e| format!("获取当前数据库连接失败: {e}"))?;
    conn.query_row("SELECT COUNT(*) FROM novel", [], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("统计当前作品数失败，无法判断是否会用空库覆盖现有作品：{e}"))
}

// ---------------------------------------------------------------------------
// 资产盘点与回填
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetPreflight {
    pub total: u64,
    pub present_locally: u64,
    pub missing_locally: u64,
    pub problems: Vec<String>,
}

/// 盘点候选备份里的图片资产：本机已有哪些、缺哪些、路径是否合法。
///
/// 只读本地文件，不发网络请求 —— 远端图片是否可下载由执行阶段的回填负责，
/// 那时才知道真实结果。
fn preflight_assets(db_path: &Path, app_data_dir: &Path) -> Result<AssetPreflight, String> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("打开候选备份失败: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT rel_path, hash FROM asset")
        .map_err(|e| format!("读取资产表失败: {e}"))?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("查询资产表失败: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("读取资产记录失败: {e}"))?;
    drop(stmt);

    let images_dir = app_data_dir.join("images");
    let mut out = AssetPreflight {
        total: rows.len() as u64,
        ..Default::default()
    };
    let mut seen = std::collections::BTreeSet::new();

    for (rel_path, hash) in &rows {
        if !seen.insert(rel_path.clone()) {
            out.problems.push(format!("资产表中 rel_path 重复：{rel_path}"));
        }
        let Some(filename) = Path::new(rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            out.problems
                .push(format!("资产 {rel_path} 没有文件名，无法定位本地文件"));
            out.missing_locally += 1;
            continue;
        };
        if !manifest::is_sha256_hex(hash) {
            out.problems
                .push(format!("资产 {rel_path} 的哈希不是合法的 SHA-256 值"));
        }
        match fs::read(images_dir.join(&filename)) {
            Ok(bytes) if manifest::sha256_hex(&bytes) == *hash => out.present_locally += 1,
            Ok(_) => {
                out.problems.push(format!(
                    "本机已有同名文件 {filename}，但内容与备份记录不符，恢复时会用备份覆盖"
                ));
                out.missing_locally += 1;
            }
            Err(_) => out.missing_locally += 1,
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetBackfill {
    pub total: u64,
    pub restored: u64,
    pub failed: u64,
    pub issues: Vec<RestoreIssue>,
}

/// 从远端回填缺失图片，并**逐个校验内容哈希**。
///
/// 下载器以闭包注入，生产环境传 [`WebDavClient::download_file`]，
/// 测试传受控实现。这样测试跑的是真实的哈希比对、原子写与问题上报控制流，
/// 而不是另写一套 mock 逻辑来自我说服。
///
/// 旧实现把 SELECT 出来的 hash 直接丢掉，只按文件名判断存在性，
/// 下载失败也用 `let _ =` 吞掉 —— 结果是「恢复完成」可能意味着素材其实
/// 缺失或已被篡改，而作者完全看不出来。
pub async fn backfill_assets<F, Fut>(
    pool: &DbPool,
    download: F,
    images_remote_dir: &str,
    local_images_dir: &Path,
) -> Result<AssetBackfill, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    fs::create_dir_all(local_images_dir)
        .map_err(|e| format!("创建本地图片目录失败: {e}"))?;

    let asset_rows: Vec<(String, String)> = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT rel_path, hash FROM asset")
            .map_err(|e| e.to_string())?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect();
        rows.map_err(|e| e.to_string())?
    };

    let mut out = AssetBackfill {
        total: asset_rows.len() as u64,
        ..Default::default()
    };

    for (rel_path, hash) in asset_rows {
        let Some(filename) = Path::new(&rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            out.failed += 1;
            out.issues.push(RestoreIssue {
                filename: rel_path.clone(),
                reason: "资产 rel_path 没有文件名，无法定位本地文件".to_string(),
            });
            continue;
        };

        let target = local_images_dir.join(&filename);
        let remote_path = format!("{}/{}", images_remote_dir, filename);

        // 本地已存在也要校验：备份可能来自另一台机器，同名不同内容会静默错配。
        if target.exists() {
            match fs::read(&target) {
                Ok(local) if manifest::sha256_hex(&local) == hash => {
                    out.restored += 1;
                    continue;
                }
                Ok(_) => out.issues.push(RestoreIssue {
                    filename: filename.clone(),
                    reason: "本地已有同名文件但内容与备份记录不符，已用备份覆盖".to_string(),
                }),
                Err(e) => out.issues.push(RestoreIssue {
                    filename: filename.clone(),
                    reason: format!("读取本地文件失败，已用备份覆盖：{e}"),
                }),
            }
        }

        match download(remote_path).await {
            Ok(data) => {
                let actual = manifest::sha256_hex(&data);
                if actual != hash {
                    out.failed += 1;
                    out.issues.push(RestoreIssue {
                        filename: filename.clone(),
                        reason: format!(
                            "远端图片哈希不匹配（期望 {}，实际 {}），已拒绝写入",
                            short_hash(&hash),
                            short_hash(&actual)
                        ),
                    });
                    continue;
                }
                if let Err(e) = crate::sync::backup::atomic_write(&target, &data) {
                    out.failed += 1;
                    out.issues.push(RestoreIssue {
                        filename: filename.clone(),
                        reason: format!("写入本地失败：{e}"),
                    });
                } else {
                    out.restored += 1;
                }
            }
            Err(e) => {
                out.failed += 1;
                out.issues.push(RestoreIssue {
                    filename: filename.clone(),
                    reason: format!("远端下载失败：{e}"),
                });
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 阶段二：执行恢复
// ---------------------------------------------------------------------------

/// 故障注入点。**生产代码恒为 `FaultPoint::None`**，仅测试用于驱动真实控制流。
///
/// 存在的理由：恢复的失败分支（快照失败、写入失败、回滚失败）在正常环境下
/// 无法触发，而不测就等于把最关键的数据安全逻辑交给运气。
/// 注入点刻意放在真实函数内部的真实位置，测试验证的是真实的回滚与上报路径，
/// 不是一个独立的 mock。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaultPoint {
    #[default]
    None,
    /// 安全快照创建失败 —— 此时现有库尚未被触碰。
    SnapshotCreate,
    /// 获取目标数据库连接失败。
    TargetOpen,
    /// SQLite Backup API 初始化失败。
    BackupInit,
    /// 目标库写入过程中失败（注入时先真实破坏目标库，再报错）。
    TargetWrite,
    /// 本机 WebDAV 连接配置写回失败。
    ConfigWrite,
    /// 本机界面设置写回失败。
    SettingWrite,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RestoreHooks {
    pub fault: FaultPoint,
    /// 回滚阶段是否也失败。
    ///
    /// 与 `fault` 正交而不是新增一档枚举，因为「回滚失败」只在
    /// 已经发生覆盖失败之后才有意义 —— 需要两者同时成立才能触达
    /// `RestoreOutcome::RollbackFailed` 这条最关键的数据安全分支。
    ///
    /// 为 true 时，回滚前会破坏安全快照，验证的是：
    /// 回滚失败绝不删除唯一可用的快照，且原始错误与回滚错误都如实上报。
    pub rollback_fails: bool,
}

/// 恢复前从本机当前库暂存、不能被远端全库覆盖的本地状态。
///
/// **刻意不派生 `Debug`**：这个结构里带着 `pwd_ref`（钥匙串引用键名）。
/// 少了 `Debug`，任何 `{:?}` 打印都会在编译期失败，
/// 而不是等到凭据引用出现在用户日志里才被发现。
#[derive(Clone, Default)]
pub struct PreservedLocalConfig {
    /// (id, url, username, pwd_ref, remote_dir)
    pub webdav: Option<(String, String, String, String, String)>,
    pub settings: Vec<(String, String)>,
}

/// 提交阶段的失败。携带回滚结果与安全快照位置，供上层如实呈现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFailure {
    /// 失败发生的阶段。
    pub stage: &'static str,
    pub cause: String,
    /// 现有库是否曾被改动。为 false 时不需要回滚。
    pub touched_database: bool,
    pub rolled_back: bool,
    pub rollback_error: Option<String>,
    pub snapshot_path: Option<PathBuf>,
}

impl CommitFailure {
    fn before_change(stage: &'static str, cause: String, snapshot: Option<PathBuf>) -> Self {
        CommitFailure {
            stage,
            cause,
            touched_database: false,
            rolled_back: false,
            rollback_error: None,
            snapshot_path: snapshot,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSuccess {
    /// 安全快照路径。调用方只有在整个恢复判定为完全成功后才允许清理。
    pub snapshot_path: PathBuf,
}

/// 读取本机需要保留的本地配置。
///
/// 查询失败必须冒泡：旧实现用 `.ok()` 把「查询失败」和「本机没有配置」
/// 压成同一个 `None`，前者会导致恢复后 WebDAV 凭据凭空消失且无人知晓。
pub fn read_preserved_local_config(pool: &DbPool) -> Result<PreservedLocalConfig, String> {
    let conn = pool.get().map_err(|e| format!("获取当前数据库连接失败: {e}"))?;

    let webdav = match conn.query_row(
        "SELECT id, url, username, pwd_ref, remote_dir FROM webdav_config LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        // 错误信息只含 SQL 层描述，不含 pwd_ref 的值。
        Err(e) => return Err(format!("读取本机 WebDAV 连接配置失败: {e}")),
    };

    let mut stmt = conn
        .prepare(&format!(
            "SELECT key, value FROM app_setting WHERE key IN ({PRESERVED_SETTING_KEYS})"
        ))
        .map_err(|e| format!("准备界面设置查询失败: {e}"))?;
    let settings = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("查询本机界面设置失败: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取本机界面设置失败: {e}"))?;

    Ok(PreservedLocalConfig { webdav, settings })
}

/// 把安全快照的内容写回目标库（回滚）。
///
/// 用 SQLite Backup API 而不是重命名文件：目标库处于 WAL 模式、
/// 连接池里有活跃连接、应用正在运行，直接换文件会让已有连接继续
/// 操作一个已被替换掉的 inode。
///
/// ## 回滚前必须先验证快照
///
/// `Connection::open` 对不存在的文件会**新建一个空库**。少了下面这道校验，
/// 「快照丢失」就会变成「用一个空库覆盖目标库」—— 回滚反而成了数据销毁。
/// 这条路径是被测试真实踩出来的，不是理论担忧。
pub fn restore_from_snapshot(pool: &DbPool, snapshot: &Path) -> Result<(), String> {
    if !snapshot.exists() {
        return Err(format!(
            "安全快照不存在（{}），无法回滚；目标数据库可能处于不完整状态，需要人工检查",
            snapshot.display()
        ));
    }
    // 复用备份预检：只读打开、integrity_check、schema 版本三项都过才允许作为回滚源。
    crate::sync::backup::preflight_backup(snapshot).map_err(|e| {
        format!(
            "安全快照未通过完整性校验，拒绝用它回滚（否则会把损坏内容写进目标库）：{e}"
        )
    })?;

    let src = rusqlite::Connection::open(snapshot)
        .map_err(|e| format!("打开安全快照失败: {e}"))?;
    let mut dst = pool
        .get()
        .map_err(|e| format!("回滚时获取目标数据库连接失败: {e}"))?;
    let backup = rusqlite::backup::Backup::new(&src, &mut dst)
        .map_err(|e| format!("初始化回滚管道失败: {e}"))?;
    backup
        .run_to_completion(20, Duration::from_millis(20), None)
        .map_err(|e| format!("从安全快照回滚失败: {e}"))?;
    Ok(())
}

/// 覆盖目标数据库并在失败时回滚。
///
/// 提交边界：安全快照 → 暂存本地配置 → 覆盖数据库 → 写回本地配置。
/// 覆盖之后发生的任何关键失败都会触发回滚；回滚本身失败时
/// **保留快照**，并同时上报原始错误与回滚错误。
pub fn commit_restored_database(
    pool: &DbPool,
    candidate_db: &Path,
    app_data_dir: &Path,
    hooks: &RestoreHooks,
) -> Result<CommitSuccess, CommitFailure> {
    // ---- 阶段 1：安全快照 ----
    let backups_dir = app_data_dir.join("backups");
    fs::create_dir_all(&backups_dir).map_err(|e| {
        CommitFailure::before_change(
            "snapshot",
            format!("创建本地快照目录失败: {e}"),
            None,
        )
    })?;

    let snapshot_path = backups_dir.join(format!(
        "pre_restore_{}_{}.db",
        Local::now().format("%Y%m%d_%H%M%S"),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ));

    if hooks.fault == FaultPoint::SnapshotCreate {
        return Err(CommitFailure::before_change(
            "snapshot",
            "创建防覆写快照失败（故障注入）".to_string(),
            None,
        ));
    }
    if let Err(e) = crate::sync::backup::snapshot_to_file(pool, &snapshot_path) {
        // 快照没建成就不许动现有库：没有退路的覆盖不是可接受的恢复。
        return Err(CommitFailure::before_change(
            "snapshot",
            format!("创建防覆写快照失败: {e}"),
            None,
        ));
    }

    // ---- 阶段 2：暂存本地独有配置 ----
    let preserved = match read_preserved_local_config(pool) {
        Ok(p) => p,
        Err(e) => {
            return Err(CommitFailure::before_change(
                "preserve_config",
                e,
                Some(snapshot_path),
            ))
        }
    };

    // ---- 阶段 3：覆盖目标库 ----
    if let Err(cause) = overwrite_target_database(pool, candidate_db, hooks) {
        return Err(rollback_after_change(pool, &snapshot_path, "database_overwrite", cause, hooks));
    }

    // ---- 阶段 4：写回本地配置 ----
    if let Err(cause) = restore_local_config(pool, &preserved, hooks) {
        return Err(rollback_after_change(pool, &snapshot_path, "local_config", cause, hooks));
    }

    Ok(CommitSuccess { snapshot_path })
}

fn overwrite_target_database(
    pool: &DbPool,
    candidate_db: &Path,
    hooks: &RestoreHooks,
) -> Result<(), String> {
    if hooks.fault == FaultPoint::TargetOpen {
        return Err("获取目标数据库连接失败（故障注入）".to_string());
    }
    let src_conn = rusqlite::Connection::open(candidate_db)
        .map_err(|e| format!("打开候选备份数据库失败: {e}"))?;
    let mut target_conn = pool
        .get()
        .map_err(|e| format!("获取目标数据库连接失败: {e}"))?;

    if hooks.fault == FaultPoint::TargetWrite {
        // 模拟「写到一半失败」：先真实破坏目标库再报错。
        // 这样回滚面对的是一个确实已经损坏的库，测试才能证明
        // 回滚真的把数据救了回来，而不是恰好什么都没发生。
        let _ = target_conn.execute("DELETE FROM novel", []);
        return Err("写入目标数据库过程中失败（故障注入）".to_string());
    }

    if hooks.fault == FaultPoint::BackupInit {
        return Err("初始化还原管道失败（故障注入）".to_string());
    }
    let backup = rusqlite::backup::Backup::new(&src_conn, &mut target_conn)
        .map_err(|e| format!("初始化还原管道失败: {e}"))?;
    backup
        .run_to_completion(20, Duration::from_millis(20), None)
        .map_err(|e| format!("还原数据至目标数据库失败: {e}"))?;
    Ok(())
}

/// 把本机独有的连接配置与界面设置写回已恢复的库。
///
/// 单事务：要么全部写回，要么一条都不写。半写状态意味着作者的
/// WebDAV 凭据可能只剩一半，比整体失败更难排查。
///
/// 错误信息只含阶段名与 SQL 层描述，**不含 pwd_ref 或密码内容**。
fn restore_local_config(
    pool: &DbPool,
    preserved: &PreservedLocalConfig,
    hooks: &RestoreHooks,
) -> Result<(), String> {
    let mut conn = pool
        .get()
        .map_err(|e| format!("获取数据库连接失败: {e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("开启本地配置恢复事务失败: {e}"))?;

    if hooks.fault == FaultPoint::ConfigWrite {
        // tx 在此处 drop → 自动回滚，不留半写状态。
        return Err("写回本机 WebDAV 连接配置失败（故障注入）".to_string());
    }
    if let Some((id, url, username, pwd_ref, remote_dir)) = &preserved.webdav {
        tx.execute(
            "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               url = excluded.url,
               username = excluded.username,
               pwd_ref = excluded.pwd_ref,
               remote_dir = excluded.remote_dir,
               updated_at = excluded.updated_at",
            rusqlite::params![id, url, username, pwd_ref, remote_dir],
        )
        .map_err(|e| format!("写回本机 WebDAV 连接配置失败: {e}"))?;
    }

    if hooks.fault == FaultPoint::SettingWrite {
        return Err("写回本机界面设置失败（故障注入）".to_string());
    }
    for (k, v) in &preserved.settings {
        tx.execute(
            "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![k, v],
        )
        .map_err(|e| format!("写回本机界面设置失败: {e}"))?;
    }

    tx.commit()
        .map_err(|e| format!("提交本地配置恢复事务失败: {e}"))
}

fn rollback_after_change(
    pool: &DbPool,
    snapshot: &Path,
    stage: &'static str,
    cause: String,
    hooks: &RestoreHooks,
) -> CommitFailure {
    if hooks.rollback_fails {
        // 受控失败点：破坏快照内容，让紧随其后的回滚真的失败。
        // 注意这里**只写坏内容、不删除文件** —— 删除会让
        // `Connection::open` 建出一个空库并把目标库清空，
        // 那是数据损坏，不是我们要测的失败。
        let _ = fs::write(snapshot, vec![0xA5u8; 8192]);
    }
    match restore_from_snapshot(pool, snapshot) {
        Ok(()) => CommitFailure {
            stage,
            cause,
            touched_database: true,
            rolled_back: true,
            rollback_error: None,
            // 快照保留：作者需要它来复核回滚结果。
            snapshot_path: Some(snapshot.to_path_buf()),
        },
        // 回滚失败绝不能删掉唯一可用的安全快照。
        Err(re) => CommitFailure {
            stage,
            cause,
            touched_database: true,
            rolled_back: false,
            rollback_error: Some(re),
            snapshot_path: Some(snapshot.to_path_buf()),
        },
    }
}

/// 凭预检 token 执行恢复。
///
/// 会**重新下载候选内容并重新跑完整校验**（哈希绑定 + 结构预检 + 清单交叉校验），
/// 因此「确认 A 却恢复 B」不可能发生：候选内容一旦被改动，哈希就对不上。
pub async fn run_consistency_restore(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    backup_filename: &str,
    app_data_dir: PathBuf,
    preflight_token: String,
    confirm_empty_overwrite: bool,
    hooks: RestoreHooks,
) -> Result<RestoreReport, String> {
    crate::sync::backup::validate_backup_filename(backup_filename)?;
    let clean_dir = remote_dir.trim_matches('/').to_string();

    // 1. 凭 token 找回候选备份，并证明它还是预检时那一份
    let cand_gz = candidate_gz_path(&preflight_token)?;
    let gz_bytes = fs::read(&cand_gz).map_err(|_| {
        "预检候选备份已失效或被清理，请重新执行预检后再恢复。当前数据库未被改动。".to_string()
    })?;
    let actual_token = manifest::sha256_hex(&gz_bytes);
    if actual_token != preflight_token {
        remove_candidate(&preflight_token);
        return Err(
            "候选备份内容与预检时不一致（可能已被另一次预检替换）。已中止，当前数据库未被改动。"
                .to_string(),
        );
    }

    // 2. 清单副本：损坏必须报错，不得当成「无清单」
    let remote_manifest = match manifest::read_sidecar(&cand_gz) {
        Ok(m) => m,
        Err(e) => {
            return Err(format!(
                "恢复中止：备份清单副本无法读取，原数据未被改动。{e}"
            ))
        }
    };

    // 3. 解压到独立临时文件
    let (cand_db, _) = decompress_candidate_to_db(&gz_bytes).map_err(|e| {
        format!("恢复中止：候选备份无法解压，原数据未被改动。{e}")
    })?;

    // 4. 重新执行结构预检
    let preflight = match crate::sync::backup::preflight_backup(&cand_db) {
        Ok(r) => r,
        Err(e) => {
            let _ = fs::remove_file(&cand_db);
            return Err(format!("恢复中止：备份未通过完整性检查，原数据未被改动。{e}"));
        }
    };

    // 5. 重新执行清单交叉校验
    let mut notes: Vec<String> = Vec::new();
    let manifest_check = match &remote_manifest {
        None => ManifestCheck::Absent,
        Some(m) => {
            let verification = match manifest::verify_manifest(
                m,
                &cand_db,
                Some(gz_bytes.len() as u64),
                Some(&actual_token),
                Some(&app_data_dir),
            ) {
                Ok(v) => v,
                Err(e) => {
                    let _ = fs::remove_file(&cand_db);
                    return Err(format!("恢复中止：无法完成清单校验，原数据未被改动。{e}"));
                }
            };
            if !verification.is_ok() {
                let _ = fs::remove_file(&cand_db);
                return Err(format!(
                    "恢复中止：备份清单与备份内容不一致，原数据未被改动。\n- {}",
                    verification.fatal.join("\n- ")
                ));
            }
            notes.extend(verification.notes);
            ManifestCheck::Verified {
                asset_count: m.assets.len() as u64,
            }
        }
    };

    // 6. 空库覆盖闸门：**必须在覆盖之前**
    let local_novels = local_novel_count(&pool)?;
    if local_novels > 0 && preflight.is_empty_library && !confirm_empty_overwrite {
        let _ = fs::remove_file(&cand_db);
        // 候选文件保留：作者确认后要用同一个 token 再来一次。
        return Err(format!(
            "已中止：这份备份是空库，而本机当前有 {local_novels} 部作品。恢复将用空库替换现有全部作品数据，需要显式确认。当前数据库未被改动。"
        ));
    }

    // 7. 提交（含失败回滚）
    let commit = commit_restored_database(&pool, &cand_db, &app_data_dir, &hooks);
    let _ = fs::remove_file(&cand_db);

    let commit = match commit {
        Ok(s) => s,
        Err(f) => {
            // 覆盖之后失败并已回滚 / 回滚也失败 —— 都不是「普通成功」。
            let outcome = if f.touched_database && !f.rolled_back {
                RestoreOutcome::RollbackFailed
            } else {
                RestoreOutcome::RolledBack
            };
            remove_candidate(&preflight_token);
            // 快照一律保留：无论回滚成功与否，作者都需要它来复核结果。
            let snapshot_path = f.snapshot_path.clone().unwrap_or_default();
            let mut issues = Vec::new();
            for n in &notes {
                issues.push(RestoreIssue {
                    filename: "（清单校验）".to_string(),
                    reason: n.clone(),
                });
            }
            issues.push(RestoreIssue {
                filename: format!("（恢复失败·{}）", f.stage),
                reason: match &f.rollback_error {
                    None if f.touched_database => format!(
                        "恢复在「{}」阶段失败，已从安全快照回滚，原数据完好。原因：{}",
                        f.stage, f.cause
                    ),
                    None => format!(
                        "恢复在「{}」阶段失败，当前数据库未被改动。原因：{}",
                        f.stage, f.cause
                    ),
                    Some(re) => format!(
                        "恢复在「{}」阶段失败，且回滚也失败。原因：{}；回滚错误：{}。安全快照已保留，请勿删除，需要人工介入。",
                        f.stage, f.cause, re
                    ),
                },
            });
            return Ok(RestoreReport {
                outcome,
                summary: RestoreSummary {
                    schema_version: preflight.schema_version,
                    novel_count: preflight.novel_count,
                    volume_count: preflight.volume_count,
                    chapter_count: preflight.chapter_count,
                    word_count: preflight.word_count,
                    is_empty_library: preflight.is_empty_library,
                    manifest: manifest_check,
                    config_restored: false,
                    config_error: f.cause.clone(),
                    database_restored: false,
                    asset_total: 0,
                    asset_restored: 0,
                    asset_failed: 0,
                    snapshot_path: snapshot_path.display().to_string(),
                    snapshot_retained: true,
                    rollback_error: f.rollback_error.clone().unwrap_or_default(),
                },
                issues,
                dangling_keys: Vec::new(),
            });
        }
    };

    // 8. 图片回填（提交之后；失败只降级为「部分成功」，不影响已恢复的数据库）
    let images_remote_dir = format!("{}/images", clean_dir);
    let local_images_dir = app_data_dir.join("images");
    let download_client = client.clone();
    let backfill = backfill_assets(
        &pool,
        move |remote_path: String| {
            let c = download_client.clone();
            async move { c.download_file(&remote_path).await }
        },
        &images_remote_dir,
        &local_images_dir,
    )
    .await;

    let mut issues: Vec<RestoreIssue> = Vec::new();
    for n in &notes {
        issues.push(RestoreIssue {
            filename: "（清单校验）".to_string(),
            reason: n.clone(),
        });
    }

    let (asset_total, asset_restored, asset_failed) = match backfill {
        Ok(b) => {
            issues.extend(b.issues);
            (b.total, b.restored, b.failed)
        }
        // 盘点/回填本身失败：数据库已恢复，但素材状态未知 —— 属于部分成功。
        Err(e) => {
            issues.push(RestoreIssue {
                filename: "（素材回填）".to_string(),
                reason: format!("无法完成图片素材回填：{e}"),
            });
            (0, 0, 1)
        }
    };

    // 9. 钥匙串 key_ref 有效性体检
    let dangling_keys = collect_dangling_keys(&pool)?;

    // 9.5 恢复成功 → 本机内容与远端在「这份备份的创建时刻」是对齐的。
    //
    //     记下这个时间点，别的设备才能判断「远端比本机新」到底是
    //     「刚被别人更新了」还是「本机自己刚写了这��」。
    //     用清单的 created_at（= 备份创建时间）而不是「现在」：
    //     远端可能还有更新的备份，写「现在」会假装本机已经包含了它们。
    {
        let mut state = crate::sync::device::load(&app_data_dir);
        if let Some(m) = &remote_manifest {
            let should_update = match &state.last_sync_at {
                Some(prev) => crate::sync::device::is_newer(&m.created_at, prev),
                None => true,
            };
            if should_update {
                state.last_sync_at = Some(m.created_at.clone());
                if let Err(e) = crate::sync::device::save(&app_data_dir, &state) {
                    eprintln!("[restore] 写入同步状态失败（不影响恢复结果）: {e}");
                }
            }
        }
    }

    // 10. 判定结局
    let outcome = if asset_failed == 0 && asset_restored == asset_total {
        RestoreOutcome::Success
    } else {
        RestoreOutcome::PartialAssets
    };

    // 11. 快照与候选文件生命周期：只有完全成功才清理快照。
    //     部分成功意味着素材仍不完整，作者可能还要再恢复一次，
    //     此时快照是唯一的退路，不能删。
    let snapshot_retained = !outcome.allows_snapshot_cleanup();
    if !snapshot_retained {
        if let Err(e) = fs::remove_file(&commit.snapshot_path) {
            // 清理失败只记警告，绝不覆盖恢复结果本身。
            eprintln!("[restore] 安全快照清理失败（可手动删除）：{e}");
        }
    }
    remove_candidate(&preflight_token);

    Ok(RestoreReport {
        outcome,
        summary: RestoreSummary {
            schema_version: preflight.schema_version,
            novel_count: preflight.novel_count,
            volume_count: preflight.volume_count,
            chapter_count: preflight.chapter_count,
            word_count: preflight.word_count,
            is_empty_library: preflight.is_empty_library,
            manifest: manifest_check,
            config_restored: true,
            config_error: String::new(),
            database_restored: true,
            asset_total,
            asset_restored,
            asset_failed,
            snapshot_path: if snapshot_retained {
                commit.snapshot_path.display().to_string()
            } else {
                String::new()
            },
            snapshot_retained,
            rollback_error: String::new(),
        },
        issues,
        dangling_keys,
    })
}

fn collect_dangling_keys(pool: &DbPool) -> Result<Vec<DanglingKeyInfo>, String> {
    let profiles: Vec<(String, String, String)> = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, name, key_ref FROM llm_profile")
            .map_err(|e| e.to_string())?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| e.to_string())?
            .collect();
        rows.map_err(|e| e.to_string())?
    };

    let mut dangling = Vec::new();
    for (profile_id, profile_name, key_ref) in profiles {
        if !key_ref.is_empty() && crate::keyring::get_secret(&key_ref).is_err() {
            dangling.push(DanglingKeyInfo {
                profile_id,
                profile_name,
                key_ref,
            });
        }
    }
    Ok(dangling)
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}
