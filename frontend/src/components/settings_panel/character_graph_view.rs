use crate::bridge::graph;
use leptos::prelude::*;
use shared_types::{Character, CharacterRelation, ExplodedProfile, GraphLinkDto, GraphNodeDto};
use wasm_bindgen::prelude::*;

#[component]
pub fn CharacterGraphView(
    characters: RwSignal<Vec<Character>>,
    relations: RwSignal<Vec<CharacterRelation>>,
    on_inspect_character: Callback<Character>,
) -> impl IntoView {
    let canvas_id = "character-nebula-canvas";
    let is_fullscreen = RwSignal::new(false);
    let active_node_id = RwSignal::new(String::new());
    let search_query = RwSignal::new(String::new());

    // Mount canvas once DOM is ready
    Effect::new(move |_| {
        let mounted = graph::mount(canvas_id, "{}");
        if mounted {
            // Register event listener for node clicks
            let window = web_sys::window().unwrap();
            let closure = Closure::wrap(Box::new(move |event: web_sys::CustomEvent| {
                if let Ok(detail) = js_sys::Reflect::get(&event.detail(), &JsValue::from_str("nodeId")) {
                    if let Some(id_str) = detail.as_string() {
                        active_node_id.set(id_str);
                    }
                }
            }) as Box<dyn FnMut(_)>);

            let _ = window.add_event_listener_with_callback(
                "character-graph-node-click",
                closure.as_ref().unchecked_ref(),
            );
            closure.forget();
        }
    });

    // Sync data into Canvas Bridge when characters or relations change
    Effect::new(move |_| {
        let chars = characters.get();
        let rels = relations.get();
        let cur_active = active_node_id.get();

        if chars.is_empty() {
            return;
        }

        let nodes: Vec<GraphNodeDto> = chars
            .iter()
            .map(|c| {
                let ep = ExplodedProfile::parse_from_str(&c.profile);
                GraphNodeDto {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    faction: if ep.faction.is_empty() { "默认阵营".to_string() } else { ep.faction },
                    importance: ep.importance,
                    summary: if !ep.core.desire.is_empty() {
                        ep.core.desire
                    } else {
                        ep.raw_notes.chars().take(40).collect()
                    },
                }
            })
            .collect();

        let links: Vec<GraphLinkDto> = rels
            .iter()
            .map(|r| GraphLinkDto {
                id: r.id.clone(),
                source: r.from_char_id.clone(),
                target: r.to_char_id.clone(),
                relation_name: r.relation_name.clone(),
                description: r.description.clone(),
            })
            .collect();

        let nodes_json = serde_json::to_string(&nodes).unwrap_or_default();
        let links_json = serde_json::to_string(&links).unwrap_or_default();
        let active_id = if cur_active.is_empty() {
            chars.first().map(|c| c.id.clone()).unwrap_or_default()
        } else {
            cur_active
        };

        let _ = graph::setData(canvas_id, &nodes_json, &links_json, &active_id);
    });

    let on_zoom_in = move |_| graph::zoomIn(canvas_id);
    let on_zoom_out = move |_| graph::zoomOut(canvas_id);
    let on_reset_view = move |_| graph::resetView(canvas_id);

    let on_select_character = move |cid: String| {
        active_node_id.set(cid.clone());
        graph::setActiveNode(canvas_id, &cid);
    };

    let on_open_active_character_modal = move |_| {
        let aid = active_node_id.get();
        if let Some(found) = characters.get().iter().find(|c| c.id == aid).cloned() {
            on_inspect_character.run(found);
        }
    };

    let active_character = move || {
        let aid = active_node_id.get();
        characters.get().into_iter().find(|c| c.id == aid)
    };

    view! {
        <div class=move || {
            format!(
                "flex flex-col bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-xl transition-all relative {}",
                if is_fullscreen.get() { "fixed inset-3 z-50 shadow-2xl" } else { "w-full h-[520px]" }
            )
        }>
            // Canvas Toolbar
            <div class="px-4 py-2.5 bg-slate-950/80 border-b border-slate-800/80 flex items-center justify-between text-xs backdrop-blur-xs z-10">
                <div class="flex items-center gap-3">
                    <div class="flex items-center gap-1.5 font-bold text-slate-100">
                        <span class="text-base text-amber-500 animate-pulse">"🌟"</span>
                        <span>"星云拓扑爆炸图"</span>
                    </div>

                    // Quick Search
                    <div class="relative w-44">
                        <input
                            type="text"
                            placeholder="定位角色并引爆..."
                            class="w-full bg-slate-900 border border-slate-700/80 rounded px-2 py-0.5 text-[11px] text-slate-200 placeholder-slate-500 focus:outline-hidden focus:border-blue-500"
                            prop:value=move || search_query.get()
                            on:input=move |ev| search_query.set(event_target_value(&ev))
                        />
                        {move || {
                            let q = search_query.get().trim().to_lowercase();
                            if q.is_empty() { return view! { <div></div> }.into_any(); }
                            let matches: Vec<_> = characters.get().into_iter().filter(|c| c.name.to_lowercase().contains(&q)).collect();
                            if matches.is_empty() { return view! { <div></div> }.into_any(); }
                            view! {
                                <div class="absolute left-0 top-full mt-1 w-full bg-slate-800 border border-slate-700 rounded-md shadow-lg py-1 z-20 max-h-36 overflow-y-auto">
                                    {matches.into_iter().map(|ch| {
                                        let cid = ch.id.clone();
                                        let cname = ch.name.clone();
                                        view! {
                                            <div
                                                class="px-2.5 py-1 text-[11px] text-slate-200 hover:bg-slate-700 cursor-pointer flex items-center justify-between"
                                                on:click=move |_| {
                                                    on_select_character(cid.clone());
                                                    search_query.set(String::new());
                                                }
                                            >
                                                <span>{cname}</span>
                                                <span class="text-[10px] text-amber-400">"聚焦 ➔"</span>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }}
                    </div>
                </div>

                // Legend & Controls
                <div class="flex items-center gap-2">
                    <div class="hidden md:flex items-center gap-2 text-[10px] text-slate-400 border-r border-slate-800 pr-3">
                        <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-amber-500"></span>"主角团"</span>
                        <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-emerald-500"></span>"宗门同盟"</span>
                        <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-sky-500"></span>"宿敌对立"</span>
                        <span class="flex items-center gap-1"><span class="w-2 h-2 rounded-full bg-purple-500"></span>"中立神秘"</span>
                    </div>

                    <div class="flex items-center bg-slate-900 border border-slate-800 rounded p-0.5">
                        <button
                            class="px-2 py-0.5 text-slate-300 hover:text-white hover:bg-slate-800 rounded cursor-pointer"
                            title="放大"
                            on:click=on_zoom_in
                        >
                            "➕"
                        </button>
                        <button
                            class="px-2 py-0.5 text-slate-300 hover:text-white hover:bg-slate-800 rounded cursor-pointer"
                            title="缩小"
                            on:click=on_zoom_out
                        >
                            "➖"
                        </button>
                        <button
                            class="px-2 py-0.5 text-slate-300 hover:text-white hover:bg-slate-800 rounded cursor-pointer"
                            title="居中重置"
                            on:click=on_reset_view
                        >
                            "🎯"
                        </button>
                    </div>

                    <button
                        class="px-2.5 py-1 text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 rounded text-[11px] font-medium cursor-pointer transition-colors"
                        on:click=move |_| is_fullscreen.set(!is_fullscreen.get())
                    >
                        {move || if is_fullscreen.get() { "⛶ 退出全屏" } else { "⛶ 全屏星图" }}
                    </button>
                </div>
            </div>

            // Main Canvas Rendering Area
            <div class="flex-1 w-full h-full relative cursor-grab active:cursor-grabbing">
                <canvas id=canvas_id class="w-full h-full block"></canvas>

                // Bottom Overlay: Current Active Character Inspector Card
                {move || if let Some(ch) = active_character() {
                    let ep = ExplodedProfile::parse_from_str(&ch.profile);
                    let ch_name = ch.name.clone();
                    let desire = if !ep.core.desire.is_empty() { ep.core.desire } else { "尚未设定渴望".to_string() };
                    let flaw = if !ep.core.flaw.is_empty() { ep.core.flaw } else { "尚未设定心魔".to_string() };

                    view! {
                        <div class="absolute left-4 bottom-4 max-w-sm bg-slate-950/85 border border-slate-700/80 rounded-xl p-3 shadow-2xl backdrop-blur-md text-xs space-y-1.5 z-20 animate-in fade-in slide-in-from-bottom-2">
                            <div class="flex items-center justify-between">
                                <div class="flex items-center gap-1.5">
                                    <span class="text-amber-500 font-extrabold text-sm">"★"</span>
                                    <span class="font-bold text-slate-100 text-sm">{ch_name}</span>
                                    <span class="text-[10px] px-1.5 py-0.5 rounded bg-blue-900/60 text-blue-300 border border-blue-800/80">
                                        {ep.faction}
                                    </span>
                                </div>
                                <button
                                    class="text-[11px] text-blue-400 hover:text-blue-300 font-semibold cursor-pointer underline flex items-center gap-0.5"
                                    on:click=on_open_active_character_modal
                                >
                                    "💥 6维全息拆解 ➔"
                                </button>
                            </div>
                            <div class="text-[11px] text-slate-300 space-y-0.5 leading-relaxed">
                                <p class="truncate"><span class="text-purple-400 font-medium">"欲求："</span> {desire}</p>
                                <p class="truncate"><span class="text-rose-400 font-medium">"心魔："</span> {flaw}</p>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }}
            </div>
        </div>
    }
}
