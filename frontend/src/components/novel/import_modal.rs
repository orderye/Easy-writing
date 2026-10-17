use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{ChapterHeader, ImportPreview, Novel, Volume};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Event, FileReader, HtmlInputElement};

#[component]
pub fn ImportModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    let active_input_tab = RwSignal::new("file".to_string()); // "file" | "paste"
    let file_name = RwSignal::new(String::new());
    let raw_text = RwSignal::new(String::new());
    let is_parsing = RwSignal::new(false);
    let is_committing = RwSignal::new(false);
    let import_target_mode = RwSignal::new("new".to_string()); // "new" | "current"
    let preview = RwSignal::new(Option::<ImportPreview>::None);
    let error_msg = RwSignal::new(Option::<String>::None);
    let epub_base64 = RwSignal::new(Option::<String>::None);
    let expanded_volumes = RwSignal::new(std::collections::HashSet::<usize>::new());

    let on_file_selected = move |ev: Event| {
        error_msg.set(None);
        epub_base64.set(None);
        let target = ev
            .target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
        if let Some(input) = target {
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let fname = file.name();
                    file_name.set(fname.clone());

                    if fname.to_lowercase().ends_with(".epub") {
                        if let Ok(reader) = FileReader::new() {
                            let reader_clone = reader.clone();
                            let text_sig = raw_text;
                            let err_sig = error_msg;
                            let epub_sig = epub_base64;
                            let on_load = Closure::wrap(Box::new(move |_: Event| {
                                if let Ok(val) = reader_clone.result() {
                                    if let Some(data_url) = val.as_string() {
                                        let b64 = data_url.split(',').nth(1).unwrap_or("").to_string();
                                        epub_sig.set(Some(b64));
                                        text_sig.set("[EPUB 电子书已就绪，点击下方开始智能解析]".to_string());
                                    } else {
                                        err_sig.set(Some("读取 EPUB 电子书文件失败".to_string()));
                                    }
                                }
                            }) as Box<dyn FnMut(_)>);

                            reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
                            on_load.forget();
                            let _ = reader.read_as_data_url(&file);
                        }
                    } else if let Ok(reader) = FileReader::new() {
                        let reader_clone = reader.clone();
                        let text_sig = raw_text;
                        let err_sig = error_msg;
                        let on_load = Closure::wrap(Box::new(move |_: Event| {
                            if let Ok(val) = reader_clone.result() {
                                if let Some(text) = val.as_string() {
                                    text_sig.set(text);
                                } else {
                                    err_sig.set(Some("无法以 UTF-8 编码读取文本文件".to_string()));
                                }
                            }
                        })
                            as Box<dyn FnMut(_)>);

                        reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
                        on_load.forget();
                        let _ = reader.read_as_text(&file);
                    }
                }
            }
        }
    };

    let on_start_parse = move |_| {
        let is_epub = file_name.get().to_lowercase().ends_with(".epub") && epub_base64.get().is_some();
        if is_epub {
            let b64 = match epub_base64.get() {
                Some(b) => b,
                None => {
                    error_msg.set(Some("EPUB 数据未就绪，请重新选择文件".to_string()));
                    return;
                }
            };
            let fname = file_name.get();
            is_parsing.set(true);
            error_msg.set(None);

            leptos::task::spawn_local(async move {
                let res = invoke_cmd("parse_import_epub")
                    .arg("file_name", fname)
                    .arg("epub_base64", b64)
                    .send::<ImportPreview>()
                    .await;

                is_parsing.set(false);
                match res {
                    Ok(p) => {
                        let mut set = std::collections::HashSet::new();
                        if p.volumes.len() <= 2 {
                            for i in 0..p.volumes.len() {
                                set.insert(i);
                            }
                        } else {
                            set.insert(0);
                        }
                        expanded_volumes.set(set);
                        preview.set(Some(p));
                    }
                    Err(e) => error_msg.set(Some(format!("EPUB 电子书解析失败: {}", e))),
                }
            });
            return;
        }

        let text = raw_text.get();
        if text.trim().is_empty() {
            error_msg.set(Some("文稿内容为空，请先选择文件或粘贴文本".to_string()));
            return;
        }

        let fname = if file_name.get().is_empty() {
            "未命名作品.txt".to_string()
        } else {
            file_name.get()
        };

        is_parsing.set(true);
        error_msg.set(None);

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("parse_import_document")
                .arg("file_name", fname)
                .arg("raw_text", text)
                .send::<ImportPreview>()
                .await;

            is_parsing.set(false);
            match res {
                Ok(p) => {
                    let mut set = std::collections::HashSet::new();
                    if p.volumes.len() <= 2 {
                        for i in 0..p.volumes.len() {
                            set.insert(i);
                        }
                    } else {
                        set.insert(0);
                    }
                    expanded_volumes.set(set);
                    preview.set(Some(p));
                }
                Err(e) => error_msg.set(Some(format!("智能解析拆解失败: {}", e))),
            }
        });
    };

    let on_confirm_commit = move |_| {
        let prev = match preview.get() {
            Some(p) => p,
            None => return,
        };

        let target_novel_id = if import_target_mode.get() == "current" {
            app_ctx.current_novel.get().map(|n| n.id)
        } else {
            None
        };

        is_committing.set(true);
        error_msg.set(None);

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("commit_import_novel")
                .opt("target_novel_id", target_novel_id)
                .arg("preview", prev)
                .send::<Novel>()
                .await;

            is_committing.set(false);
            match res {
                Ok(novel) => {
                    // 1. Reload novel list
                    if let Ok(novels) = invoke_cmd("list_novels").send::<Vec<Novel>>().await {
                        app_ctx.novels_list.set(novels);
                    }

                    // 2. Set current novel
                    app_ctx.current_novel.set(Some(novel.clone()));

                    // 3. Reload volumes and chapters
                    let nid = novel.id.clone();
                    if let Ok(vols) = invoke_cmd("list_volumes")
                        .arg("novel_id", nid.clone())
                        .send::<Vec<Volume>>()
                        .await
                    {
                        if let Some(first_vol) = vols.first() {
                            app_ctx.current_volume.set(Some(first_vol.clone()));
                        }
                        app_ctx.volumes_list.set(vols);
                    }

                    if let Ok(chaps) = invoke_cmd("list_all_chapter_headers_for_novel")
                        .arg("novel_id", nid)
                        .send::<Vec<ChapterHeader>>()
                        .await
                    {
                        app_ctx.chapters_map.set(chaps.clone());
                        if let Some(first) = chaps.first() {
                            crate::components::structure_panel::chapter_list::do_switch_chapter(
                                app_ctx,
                                first.id.clone(),
                            );
                        }
                    }

                    // 4. Reset & Close
                    preview.set(None);
                    raw_text.set(String::new());
                    file_name.set(String::new());
                    epub_base64.set(None);
                    is_open.set(false);
                }
                Err(e) => error_msg.set(Some(format!("入库提交失败: {}", e))),
            }
        });
    };

    view! {
        {move || {
            if is_open.get() {
                let prev_opt = preview.get();
                let parsing = is_parsing.get();
                let committing = is_committing.get();
                let text_len = raw_text.get().chars().count();
                let has_current_novel = app_ctx.current_novel.get().is_some();

                view! {
                    <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center z-50 p-6 select-none">
                        <div class="bg-white dark:bg-slate-900 rounded-xl shadow-2xl max-w-3xl w-full border border-slate-200 dark:border-slate-800 overflow-hidden flex flex-col max-h-[88vh] animate-in fade-in zoom-in-95">
                            // Header
                            <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-blue-50/50 dark:bg-blue-950/20 shrink-0">
                                <div class="flex items-center gap-2">
                                    <span class="text-blue-600 dark:text-blue-400 font-bold text-base">"📥"</span>
                                    <div>
                                        <h3 class="font-bold text-sm text-slate-800 dark:text-slate-100">"批量导入长篇文稿（智能拆卷拆章）"</h3>
                                        <p class="text-[11px] text-slate-500 dark:text-slate-400">"按正则识别「第X卷」、「第X章/节/回」等层级，无缝导入作品结构库"</p>
                                    </div>
                                </div>
                                <button
                                    class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 font-mono text-lg cursor-pointer px-1"
                                    on:click=move |_| is_open.set(false)
                                >
                                    "✕"
                                </button>
                            </div>

                            // Error Alert
                            {move || {
                                if let Some(err) = error_msg.get() {
                                    view! {
                                        <div class="mx-5 mt-3 p-2.5 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 text-rose-700 dark:text-rose-300 text-xs rounded-lg flex items-center gap-2">
                                            <span>"⚠️"</span>
                                            <span>{err}</span>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <div></div> }.into_any()
                                }
                            }}

                            // Body Content
                            <div class="p-5 flex-1 overflow-y-auto space-y-4">
                                {match prev_opt {
                                    None => {
                                        // Step 1: Input & File Selection
                                        view! {
                                            <div class="space-y-4">
                                                // Tab Switcher
                                                <div class="flex border-b border-slate-200 dark:border-slate-800 text-xs font-semibold">
                                                    <button
                                                        class=move || format!(
                                                            "px-4 py-2 border-b-2 cursor-pointer transition-colors {}",
                                                            if active_input_tab.get() == "file" {
                                                                "border-blue-600 text-blue-600 dark:text-blue-400"
                                                            } else {
                                                                "border-transparent text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200"
                                                            }
                                                        )
                                                        on:click=move |_| active_input_tab.set("file".to_string())
                                                    >
                                                        "📄 选择本地文件 (.txt / .md / .epub)"
                                                    </button>
                                                    <button
                                                        class=move || format!(
                                                            "px-4 py-2 border-b-2 cursor-pointer transition-colors {}",
                                                            if active_input_tab.get() == "paste" {
                                                                "border-blue-600 text-blue-600 dark:text-blue-400"
                                                            } else {
                                                                "border-transparent text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200"
                                                            }
                                                        )
                                                        on:click=move |_| active_input_tab.set("paste".to_string())
                                                    >
                                                        "📋 直接粘贴全文"
                                                    </button>
                                                </div>

                                                // Tab Content
                                                {move || if active_input_tab.get() == "file" {
                                                    view! {
                                                        <div class="space-y-3">
                                                            <label class="border-2 border-dashed border-slate-300 dark:border-slate-700 rounded-xl p-8 flex flex-col items-center justify-center gap-2 hover:border-blue-500 dark:hover:border-blue-500 bg-slate-50/50 dark:bg-slate-850/50 cursor-pointer transition-colors">
                                                                <input
                                                                    type="file"
                                                                    accept=".txt,.md,.epub"
                                                                    class="hidden"
                                                                    on:change=on_file_selected
                                                                />
                                                                <span class="text-3xl">"📁"</span>
                                                                <span class="text-xs font-semibold text-slate-700 dark:text-slate-200">
                                                                    {move || {
                                                                        let f = file_name.get();
                                                                        if f.is_empty() {
                                                                            "点击选择本地 .txt、.md 或 .epub 电子书文件".to_string()
                                                                        } else {
                                                                            format!("已选中文件: {}", f)
                                                                        }
                                                                    }}
                                                                </span>
                                                                <span class="text-[11px] text-slate-400">"支持 TXT / Markdown 纯文本及标准 EPUB 电子书，自动识别目录分卷分章并提取作品名"</span>
                                                            </label>

                                                            {move || {
                                                                let is_ep = file_name.get().to_lowercase().ends_with(".epub");
                                                                if is_ep && epub_base64.get().is_some() {
                                                                    view! {
                                                                        <div class="text-xs text-blue-600 dark:text-blue-400 flex items-center justify-between px-1">
                                                                            <span>"✓ EPUB 电子书已就绪，点击下方「开始智能解析」即可提取卷章与目录树"</span>
                                                                        </div>
                                                                    }.into_any()
                                                                } else if text_len > 0 {
                                                                    view! {
                                                                        <div class="text-xs text-slate-600 dark:text-slate-400 flex items-center justify-between px-1">
                                                                            <span>"已载入文稿字数：" <strong class="text-blue-600 dark:text-blue-400">{text_len}</strong> " 字符"</span>
                                                                        </div>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <div></div> }.into_any()
                                                                }
                                                            }}
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <div class="space-y-2">
                                                            <div class="flex items-center gap-2">
                                                                <label class="text-xs font-semibold text-slate-700 dark:text-slate-300 shrink-0">"作品名称:"</label>
                                                                <input
                                                                    type="text"
                                                                    class="flex-1 text-xs px-2.5 py-1 border border-slate-200 dark:border-slate-700 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                                                    placeholder="输入作品标题..."
                                                                    prop:value=move || file_name.get()
                                                                    on:input=move |ev| file_name.set(event_target_value(&ev))
                                                                />
                                                            </div>
                                                            <textarea
                                                                class="w-full text-xs text-slate-700 dark:text-slate-200 bg-slate-50 dark:bg-slate-800/60 border border-slate-200 dark:border-slate-700 rounded-lg p-3 min-h-[220px] font-serif leading-relaxed focus:bg-white dark:focus:bg-slate-900 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                                                placeholder="在此直接粘贴长篇正文内容（需包含「第X卷」、「第X章」等标题标记）……"
                                                                prop:value=move || raw_text.get()
                                                                on:input=move |ev| raw_text.set(event_target_value(&ev))
                                                            ></textarea>
                                                            <div class="text-right text-[11px] text-slate-400">
                                                                "已输入 " {move || raw_text.get().chars().count()} " 字符"
                                                            </div>
                                                        </div>
                                                    }.into_any()
                                                }}
                                            </div>
                                        }.into_any()
                                    }
                                    Some(ref p) => {
                                        // Step 2: Split Preview & Confirm
                                        let total_chapters: usize = p.volumes.iter().map(|v| v.chapters.len()).sum();
                                        let vols_clone = p.volumes.clone();

                                        view! {
                                            <div class="space-y-4">
                                                 // Summary Banner
                                                <div class="grid grid-cols-3 gap-3 p-3 bg-blue-50/70 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-800/80 rounded-lg text-center text-xs">
                                                    <div>
                                                        <div class="text-slate-400 dark:text-slate-400 text-[10px]">"识别分卷"</div>
                                                        <div class="text-base font-bold text-blue-700 dark:text-blue-300">{p.volumes.len()} " 卷"</div>
                                                    </div>
                                                    <div>
                                                        <div class="text-slate-400 dark:text-slate-400 text-[10px]">"识别章节"</div>
                                                        <div class="text-base font-bold text-blue-700 dark:text-blue-300">{total_chapters} " 章"</div>
                                                    </div>
                                                    <div>
                                                        <div class="text-slate-400 dark:text-slate-400 text-[10px]">"正文总字数"</div>
                                                        <div class="text-base font-bold text-blue-700 dark:text-blue-300">{p.total_word_count} " 字"</div>
                                                    </div>
                                                </div>

                                                // Novel Title Editor
                                                <div class="flex items-center gap-2.5 p-3 bg-white dark:bg-slate-800/80 border border-slate-200 dark:border-slate-700/80 rounded-lg">
                                                    <label class="text-xs font-semibold text-slate-700 dark:text-slate-200 shrink-0 flex items-center gap-1.5">
                                                        <span>"📖"</span>
                                                        <span>"作品名称:"</span>
                                                    </label>
                                                    <input
                                                        type="text"
                                                        class="flex-1 text-xs px-2.5 py-1.5 border border-slate-300 dark:border-slate-600 rounded bg-slate-50 dark:bg-slate-900 text-slate-800 dark:text-slate-100 font-semibold focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                                        placeholder="输入作品标题..."
                                                        prop:value=move || preview.get().map(|p| p.novel_title).unwrap_or_default()
                                                        on:input=move |ev| {
                                                            let val = event_target_value(&ev);
                                                            preview.update(|opt| {
                                                                if let Some(p) = opt {
                                                                    p.novel_title = val;
                                                                }
                                                            });
                                                        }
                                                    />
                                                </div>

                                                // Destination Selection
                                                <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-3 space-y-2 bg-slate-50/50 dark:bg-slate-850/50">
                                                    <span class="text-xs font-bold text-slate-700 dark:text-slate-200">"导入目标位置:"</span>
                                                    <div class="flex items-center gap-4 text-xs">
                                                        <label class="flex items-center gap-1.5 cursor-pointer">
                                                            <input
                                                                type="radio"
                                                                name="target_mode"
                                                                value="new"
                                                                prop:checked=move || import_target_mode.get() == "new"
                                                                on:change=move |_| import_target_mode.set("new".to_string())
                                                            />
                                                            <span class="text-slate-700 dark:text-slate-200">"新建为独立作品"</span>
                                                        </label>
                                                        {if has_current_novel {
                                                            view! {
                                                                <label class="flex items-center gap-1.5 cursor-pointer">
                                                                    <input
                                                                        type="radio"
                                                                        name="target_mode"
                                                                        value="current"
                                                                        prop:checked=move || import_target_mode.get() == "current"
                                                                        on:change=move |_| import_target_mode.set("current".to_string())
                                                                    />
                                                                    <span class="text-slate-700 dark:text-slate-200">"追加合并至当前作品"</span>
                                                                </label>
                                                            }.into_any()
                                                        } else {
                                                            view! { <div></div> }.into_any()
                                                        }}
                                                    </div>
                                                </div>

                                                // Hierarchy Tree Preview
                                                <div class="border border-slate-200 dark:border-slate-800 rounded-lg overflow-hidden max-h-[320px] overflow-y-auto">
                                                    <div class="px-3 py-2 bg-slate-100 dark:bg-slate-800 border-b border-slate-200 dark:border-slate-700 text-xs font-bold text-slate-700 dark:text-slate-200 flex items-center justify-between sticky top-0 z-10">
                                                        <span>"拆解结果目录层级预览"</span>
                                                        <div class="flex items-center gap-2 font-normal text-[11px]">
                                                            <button
                                                                type="button"
                                                                class="text-blue-600 dark:text-blue-400 hover:underline cursor-pointer"
                                                                on:click=move |_| {
                                                                    if let Some(p) = preview.get() {
                                                                        let all: std::collections::HashSet<usize> = (0..p.volumes.len()).collect();
                                                                        expanded_volumes.set(all);
                                                                    }
                                                                }
                                                            >
                                                                "展开全部"
                                                            </button>
                                                            <span class="text-slate-300 dark:text-slate-600">"|"</span>
                                                            <button
                                                                type="button"
                                                                class="text-slate-500 dark:text-slate-400 hover:underline cursor-pointer"
                                                                on:click=move |_| {
                                                                    expanded_volumes.set(std::collections::HashSet::new());
                                                                }
                                                            >
                                                                "收起全部"
                                                            </button>
                                                        </div>
                                                    </div>
                                                    <div class="p-2 space-y-2">
                                                        {vols_clone.into_iter().enumerate().map(|(vol_idx, vol)| {
                                                            let is_expanded = move || expanded_volumes.get().contains(&vol_idx);
                                                            let vol_words: i32 = vol.chapters.iter().map(|c| c.word_count).sum();
                                                            let vol_chapters = vol.chapters.clone();
                                                            view! {
                                                                <div class="border border-slate-200 dark:border-slate-700/60 rounded p-2 bg-white dark:bg-slate-900 space-y-1">
                                                                    <div
                                                                        class="flex items-center justify-between text-xs font-bold text-slate-800 dark:text-slate-200 cursor-pointer hover:bg-slate-50 dark:hover:bg-slate-850/50 p-1 rounded select-none transition-colors"
                                                                        on:click=move |_| {
                                                                            expanded_volumes.update(|set| {
                                                                                if set.contains(&vol_idx) {
                                                                                    set.remove(&vol_idx);
                                                                                } else {
                                                                                    set.insert(vol_idx);
                                                                                }
                                                                            });
                                                                        }
                                                                    >
                                                                        <div class="flex items-center gap-1.5">
                                                                            <span class="text-[10px] text-slate-400 font-mono">
                                                                                {move || if is_expanded() { "▼" } else { "▶" }}
                                                                            </span>
                                                                            <span>"📁"</span>
                                                                            <span>{vol.title.clone()}</span>
                                                                        </div>
                                                                        <div class="flex items-center gap-2 text-[10px] text-slate-400 font-normal">
                                                                            <span>{vol_words} " 字"</span>
                                                                            <span>"•"</span>
                                                                            <span>{vol.chapters.len()} " 章"</span>
                                                                        </div>
                                                                    </div>
                                                                    {move || if is_expanded() {
                                                                        view! {
                                                                            <div class="pl-5 space-y-0.5 pt-1 border-t border-slate-100 dark:border-slate-800">
                                                                                {vol_chapters.clone().into_iter().map(|ch| {
                                                                                    view! {
                                                                                        <div class="flex items-center justify-between text-[11px] text-slate-600 dark:text-slate-400 py-0.5">
                                                                                            <span class="truncate max-w-[400px]">"📄 " {ch.title}</span>
                                                                                            <span class="text-[10px] text-slate-400">{ch.word_count} " 字"</span>
                                                                                        </div>
                                                                                    }
                                                                                }).collect::<Vec<_>>()}
                                                                            </div>
                                                                        }.into_any()
                                                                    } else {
                                                                        view! { <div></div> }.into_any()
                                                                    }}
                                                                </div>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                </div>
                                            </div>
                                        }.into_any()
                                    }
                                }}
                            </div>

                            // Footer Buttons
                            <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-850 flex items-center justify-between shrink-0">
                                {move || if preview.get().is_none() {
                                    view! {
                                        <div class="flex items-center justify-between w-full">
                                            <span class="text-xs text-slate-400">"单章超长时系统将自动保留段落格式"</span>
                                            <div class="flex items-center gap-2">
                                                <button
                                                    class="px-3.5 py-1.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-750 rounded-md font-medium transition-colors cursor-pointer"
                                                    on:click=move |_| is_open.set(false)
                                                >
                                                    "取消"
                                                </button>
                                                <button
                                                    class="px-4 py-1.5 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded-md font-semibold shadow-xs transition-colors cursor-pointer flex items-center gap-1.5 disabled:opacity-50"
                                                    disabled=move || text_len == 0 || parsing
                                                    on:click=on_start_parse
                                                >
                                                    {if parsing {
                                                        view! { <span>"⚡ 正在分析拆解..."</span> }.into_any()
                                                    } else {
                                                        view! { <span>"⚡ 开始智能分析拆解"</span> }.into_any()
                                                    }}
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="flex items-center justify-between w-full">
                                            <button
                                                class="px-3 py-1.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-750 rounded-md transition-colors cursor-pointer"
                                                on:click=move |_| preview.set(None)
                                            >
                                                "← 重新上传/调整"
                                            </button>
                                            <div class="flex items-center gap-2">
                                                <button
                                                    class="px-3.5 py-1.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-750 rounded-md font-medium transition-colors cursor-pointer"
                                                    on:click=move |_| is_open.set(false)
                                                >
                                                    "取消"
                                                </button>
                                                <button
                                                    class="px-4 py-1.5 text-xs bg-emerald-600 hover:bg-emerald-700 text-white rounded-md font-semibold shadow-xs transition-colors cursor-pointer flex items-center gap-1.5 disabled:opacity-50"
                                                    disabled=committing
                                                    on:click=on_confirm_commit
                                                >
                                                    {if committing {
                                                        view! { <span>"⏳ 正在事务入库..."</span> }.into_any()
                                                    } else {
                                                        view! { <span>"✓ 确认导入并加载"</span> }.into_any()
                                                    }}
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }
        }}
    }
}
