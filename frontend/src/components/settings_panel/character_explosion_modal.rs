use crate::components::ai::polish_button::PolishButton;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{
    Character, CharacterAppearance, CharacterAssetItem, CharacterCore, CharacterSecret, CharacterVoice,
    ExplodedProfile,
};

#[derive(Serialize)]
struct SaveCharacterArgs {
    id: Option<String>,
    novel_id: String,
    name: String,
    aliases: String,
    profile: String,
}

#[component]
pub fn CharacterExplosionModal(
    app_ctx: AppContext,
    character: RwSignal<Option<Character>>,
    on_saved: Callback<()>,
) -> impl IntoView {
    // Extracted 6-dimension state
    let core_desire = RwSignal::new(String::new());
    let core_need = RwSignal::new(String::new());
    let core_flaw = RwSignal::new(String::new());
    let core_principle = RwSignal::new(String::new());

    let app_features = RwSignal::new(String::new());
    let app_clothing = RwSignal::new(String::new());
    let app_gestures = RwSignal::new(String::new());

    let voice_tone = RwSignal::new(String::new());
    let voice_catchphrase = RwSignal::new(String::new());
    let voice_triggers = RwSignal::new(String::new());

    let sec_identity = RwSignal::new(String::new());
    let sec_weakness = RwSignal::new(String::new());
    let sec_secret = RwSignal::new(String::new());

    let faction = RwSignal::new("默认阵营".to_string());
    let importance = RwSignal::new(2i32);
    let assets = RwSignal::new(Vec::<CharacterAssetItem>::new());
    let raw_notes = RwSignal::new(String::new());

    let active_dimension = RwSignal::new("core".to_string()); // "core" | "appearance" | "voice" | "assets" | "secret"

    // Sync when modal opens
    Effect::new(move |_| {
        if let Some(ch) = character.get() {
            let ep = ExplodedProfile::parse_from_str(&ch.profile);
            core_desire.set(ep.core.desire);
            core_need.set(ep.core.inner_need);
            core_flaw.set(ep.core.flaw);
            core_principle.set(ep.core.principle);

            app_features.set(ep.appearance.features);
            app_clothing.set(ep.appearance.clothing);
            app_gestures.set(ep.appearance.gestures);

            voice_tone.set(ep.voice.tone);
            voice_catchphrase.set(ep.voice.catchphrase);
            voice_triggers.set(ep.voice.triggers);

            sec_identity.set(ep.secret.identity);
            sec_weakness.set(ep.secret.weakness);
            sec_secret.set(ep.secret.secret);

            faction.set(if ep.faction.is_empty() { "默认阵营".to_string() } else { ep.faction });
            importance.set(if ep.importance == 0 { 2 } else { ep.importance });
            assets.set(ep.assets);
            raw_notes.set(ep.raw_notes);
        }
    });

    let on_save = move |_| {
        if let (Some(ch), Some(novel)) = (character.get(), app_ctx.current_novel.get()) {
            let ep = ExplodedProfile {
                core: CharacterCore {
                    desire: core_desire.get(),
                    inner_need: core_need.get(),
                    flaw: core_flaw.get(),
                    principle: core_principle.get(),
                },
                appearance: CharacterAppearance {
                    features: app_features.get(),
                    clothing: app_clothing.get(),
                    gestures: app_gestures.get(),
                },
                voice: CharacterVoice {
                    tone: voice_tone.get(),
                    catchphrase: voice_catchphrase.get(),
                    triggers: voice_triggers.get(),
                },
                assets: assets.get(),
                secret: CharacterSecret {
                    identity: sec_identity.get(),
                    weakness: sec_weakness.get(),
                    secret: sec_secret.get(),
                },
                faction: faction.get(),
                importance: importance.get(),
                raw_notes: raw_notes.get(),
            };

            let json_str = ep.to_json_string();
            let cid = ch.id.clone();
            let cname = ch.name.clone();
            let caliases = ch.aliases.clone();
            let nid = novel.id;

            leptos::task::spawn_local(async move {
                let _ = invoke::<Character, _>(
                    "save_character",
                    &SaveCharacterArgs {
                        id: Some(cid),
                        novel_id: nid,
                        name: cname,
                        aliases: caliases,
                        profile: json_str,
                    },
                )
                .await;
                character.set(None); // close modal
                on_saved.run(());
            });
        }
    };

    let on_add_asset = move |_| {
        let mut list = assets.get();
        list.push(CharacterAssetItem {
            name: "新器物/道具".to_string(),
            kind: "武器".to_string(),
            note: "".to_string(),
        });
        assets.set(list);
    };

    let on_remove_asset = move |idx: usize| {
        let mut list = assets.get();
        if idx < list.len() {
            list.remove(idx);
            assets.set(list);
        }
    };

    view! {
        {move || if let Some(ch) = character.get() {
            let ch_name = ch.name.clone();
            view! {
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4">
                    <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl shadow-2xl w-full max-w-3xl max-h-[90vh] flex flex-col overflow-hidden">
                        // Modal Header
                        <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50 dark:bg-slate-850">
                            <div class="flex items-center gap-2">
                                <span class="text-xl">"💥"</span>
                                <div>
                                    <h3 class="text-sm font-bold text-slate-800 dark:text-slate-100 flex items-center gap-2">
                                        <span>"角色 6 维全息解构："</span>
                                        <span class="text-blue-600 dark:text-blue-400 font-extrabold">{ch_name}</span>
                                    </h3>
                                    <p class="text-[11px] text-slate-500 dark:text-slate-400">
                                        "机械爆炸图式立体拆解：灵魂核、外貌皮囊、言行性格、器物资产与隐秘死穴"
                                    </p>
                                </div>
                            </div>
                            <div class="flex items-center gap-2">
                                <button
                                    class="text-xs px-2.5 py-1 text-slate-500 hover:text-slate-800 dark:hover:text-slate-200 rounded hover:bg-slate-200 dark:hover:bg-slate-800 cursor-pointer transition-colors"
                                    on:click=move |_| character.set(None)
                                >
                                    "取消"
                                </button>
                                <button
                                    class="text-xs px-3.5 py-1 bg-blue-600 hover:bg-blue-700 text-white font-semibold rounded shadow-xs cursor-pointer transition-colors"
                                    on:click=on_save
                                >
                                    "保存解构数据"
                                </button>
                            </div>
                        </div>

                        // Top Dimension Navigation Pills
                        <div class="px-5 py-2 border-b border-slate-200 dark:border-slate-800 bg-slate-100/60 dark:bg-slate-800/40 flex items-center justify-between gap-2 overflow-x-auto">
                            <div class="flex items-center gap-1.5">
                                <button
                                    class=move || format!(
                                        "px-2.5 py-1 rounded text-xs font-medium cursor-pointer transition-colors {}",
                                        if active_dimension.get() == "core" { "bg-purple-600 text-white shadow-2xs" } else { "bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200" }
                                    )
                                    on:click=move |_| active_dimension.set("core".to_string())
                                >
                                    "🟣 灵魂核 (欲求/心魔)"
                                </button>
                                <button
                                    class=move || format!(
                                        "px-2.5 py-1 rounded text-xs font-medium cursor-pointer transition-colors {}",
                                        if active_dimension.get() == "appearance" { "bg-emerald-600 text-white shadow-2xs" } else { "bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200" }
                                    )
                                    on:click=move |_| active_dimension.set("appearance".to_string())
                                >
                                    "🟢 外貌与体态"
                                </button>
                                <button
                                    class=move || format!(
                                        "px-2.5 py-1 rounded text-xs font-medium cursor-pointer transition-colors {}",
                                        if active_dimension.get() == "voice" { "bg-amber-600 text-white shadow-2xs" } else { "bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200" }
                                    )
                                    on:click=move |_| active_dimension.set("voice".to_string())
                                >
                                    "🟡 性格与言行口癖"
                                </button>
                                <button
                                    class=move || format!(
                                        "px-2.5 py-1 rounded text-xs font-medium cursor-pointer transition-colors {}",
                                        if active_dimension.get() == "assets" { "bg-rose-600 text-white shadow-2xs" } else { "bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200" }
                                    )
                                    on:click=move |_| active_dimension.set("assets".to_string())
                                >
                                    "🔴 专属器物与资产 (" {move || assets.get().len()} ")"
                                </button>
                                <button
                                    class=move || format!(
                                        "px-2.5 py-1 rounded text-xs font-medium cursor-pointer transition-colors {}",
                                        if active_dimension.get() == "secret" { "bg-sky-600 text-white shadow-2xs" } else { "bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200" }
                                    )
                                    on:click=move |_| active_dimension.set("secret".to_string())
                                >
                                    "🔵 身份秘密与死穴"
                                </button>
                            </div>

                            // Faction & Importance Selector
                            <div class="flex items-center gap-2 text-xs shrink-0">
                                <label class="text-slate-500 font-medium">"阵营:"</label>
                                <select
                                    class="bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded px-2 py-0.5 text-xs text-slate-800 dark:text-slate-200"
                                    prop:value=move || faction.get()
                                    on:change=move |ev| faction.set(event_target_value(&ev))
                                >
                                    <option value="默认阵营">"默认阵营"</option>
                                    <option value="主角团">"主角团 (橙色)"</option>
                                    <option value="宗门">"宗门/同盟 (绿色)"</option>
                                    <option value="宿敌">"宿敌/对立 (蓝色)"</option>
                                    <option value="中立">"中立/神秘 (紫色)"</option>
                                    <option value="羁绊">"特殊羁绊 (玫瑰红)"</option>
                                </select>
                            </div>
                        </div>

                        // Modal Body Content
                        <div class="flex-1 overflow-y-auto p-5 text-xs space-y-4">
                            {move || match active_dimension.get().as_str() {
                                "core" => view! {
                                    <div class="space-y-3.5">
                                        <div class="bg-purple-50/60 dark:bg-purple-950/30 p-3 rounded-lg border border-purple-200 dark:border-purple-900/50">
                                            <span class="font-bold text-purple-700 dark:text-purple-300 block mb-1">"🟣 灵魂核心剖析 (人物动机与成长弧光驱动器)"</span>
                                            <p class="text-[11px] text-purple-600/80 dark:text-purple-400/80 leading-relaxed">
                                                "戏剧张力源自「渴望」与「心魔」的拉扯：人物以为自己想要 Want，但拯救他灵魂的往往是 Need。"
                                            </p>
                                        </div>

                                        <div class="grid grid-cols-2 gap-4">
                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"表面渴望 (Want - 外在追逐的目标):"</label>
                                                <textarea
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:ring-1 focus:ring-purple-500 focus:outline-hidden min-h-[70px]"
                                                    placeholder="如：夺回天下第一剑，为家族复仇，登临皇位……"
                                                    prop:value=move || core_desire.get()
                                                    on:input=move |ev| core_desire.set(event_target_value(&ev))
                                                ></textarea>
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"内在所需 (Need - 真正能救赎内心的东西):"</label>
                                                <textarea
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:ring-1 focus:ring-purple-500 focus:outline-hidden min-h-[70px]"
                                                    placeholder="如：学会相信同伴，放下对过去的执念，接纳不完美的自己……"
                                                    prop:value=move || core_need.get()
                                                    on:input=move |ev| core_need.set(event_target_value(&ev))
                                                ></textarea>
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"致命心魔/执念 (Flaw / The Lie):"</label>
                                                <textarea
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:ring-1 focus:ring-purple-500 focus:outline-hidden min-h-[70px]"
                                                    placeholder="如：曾遭恩师背叛剖心，深信世人皆不可信，随时准备先下手为强……"
                                                    prop:value=move || core_flaw.get()
                                                    on:input=move |ev| core_flaw.set(event_target_value(&ev))
                                                ></textarea>
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"行事底线/不可逾越之信条 (Principle):"</label>
                                                <textarea
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:ring-1 focus:ring-purple-500 focus:outline-hidden min-h-[70px]"
                                                    placeholder="如：决不伤及无辜妇孺，诺言一旦立下纵死不弃……"
                                                    prop:value=move || core_principle.get()
                                                    on:input=move |ev| core_principle.set(event_target_value(&ev))
                                                ></textarea>
                                            </div>
                                        </div>
                                    </div>
                                }.into_any(),

                                "appearance" => view! {
                                    <div class="space-y-3.5">
                                        <div class="bg-emerald-50/60 dark:bg-emerald-950/30 p-3 rounded-lg border border-emerald-200 dark:border-emerald-900/50">
                                            <span class="font-bold text-emerald-700 dark:text-emerald-300 block mb-1">"🟢 外貌体态解构 (画面感与标志性动作记忆点)"</span>
                                            <p class="text-[11px] text-emerald-600/80 dark:text-emerald-400/80">
                                                "读者记住一个角色，往往只需要 1 个视觉锚点（如异瞳/断眉）和 1 个标志性动作。"
                                            </p>
                                        </div>

                                        <div class="space-y-3">
                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"视觉锚点与核心体貌特征:"</label>
                                                <input
                                                    type="text"
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded px-2.5 py-1.5 focus:ring-1 focus:ring-emerald-500 focus:outline-hidden"
                                                    placeholder="如：满头如霜白发、盲目覆玄色锦绫、左颊有道浅浅剑痕"
                                                    prop:value=move || app_features.get()
                                                    on:input=move |ev| app_features.set(event_target_value(&ev))
                                                />
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"经典穿搭与外显气场风格:"</label>
                                                <input
                                                    type="text"
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded px-2.5 py-1.5 focus:ring-1 focus:ring-emerald-500 focus:outline-hidden"
                                                    placeholder="如：玄青色鹤氅宽袍、袖口用暗金线绣着残月，行动间带淡淡冷香"
                                                    prop:value=move || app_clothing.get()
                                                    on:input=move |ev| app_clothing.set(event_target_value(&ev))
                                                />
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"习惯性肢体动作/微表情:"</label>
                                                <textarea
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:ring-1 focus:ring-emerald-500 focus:outline-hidden min-h-[60px]"
                                                    placeholder="如：沉思时会下意识用食指轻扣剑柄；说谎时眼神会微微下垂……"
                                                    prop:value=move || app_gestures.get()
                                                    on:input=move |ev| app_gestures.set(event_target_value(&ev))
                                                ></textarea>
                                            </div>
                                        </div>
                                    </div>
                                }.into_any(),

                                "voice" => view! {
                                    <div class="space-y-3.5">
                                        <div class="bg-amber-50/60 dark:bg-amber-950/30 p-3 rounded-lg border border-amber-200 dark:border-amber-900/50">
                                            <span class="font-bold text-amber-700 dark:text-amber-300 block mb-1">"🟡 性格与言行台词 (听觉特征与情绪爆发点)"</span>
                                        </div>

                                        <div class="space-y-3">
                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"说话语调与口吻风格:"</label>
                                                <input
                                                    type="text"
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded px-2.5 py-1.5 focus:ring-1 focus:ring-amber-500 focus:outline-hidden"
                                                    placeholder="如：低沉清冷、带有一丝漫不经心的嘲弄感、从不轻易提高音量"
                                                    prop:value=move || voice_tone.get()
                                                    on:input=move |ev| voice_tone.set(event_target_value(&ev))
                                                />
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"经典口头禅 / 口癖:"</label>
                                                <input
                                                    type="text"
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded px-2.5 py-1.5 focus:ring-1 focus:ring-amber-500 focus:outline-hidden"
                                                    placeholder="如：「世人皆苦，本座也不例外」、「有趣，但还不够」"
                                                    prop:value=move || voice_catchphrase.get()
                                                    on:input=move |ev| voice_catchphrase.set(event_target_value(&ev))
                                                />
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"情绪失控雷区 (一旦被触及必定爆发剧情高潮):"</label>
                                                <textarea
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:ring-1 focus:ring-amber-500 focus:outline-hidden min-h-[60px]"
                                                    placeholder="如：提及当年弑师惨案，或有人试图触碰其龙渊剑鞘时，瞬间杀机毕露"
                                                    prop:value=move || voice_triggers.get()
                                                    on:input=move |ev| voice_triggers.set(event_target_value(&ev))
                                                ></textarea>
                                            </div>
                                        </div>
                                    </div>
                                }.into_any(),

                                "assets" => view! {
                                    <div class="space-y-3.5">
                                        <div class="flex items-center justify-between bg-rose-50/60 dark:bg-rose-950/30 p-3 rounded-lg border border-rose-200 dark:border-rose-900/50">
                                            <div>
                                                <span class="font-bold text-rose-700 dark:text-rose-300 block mb-0.5">"🔴 随身专属器物、信物与底牌"</span>
                                                <p class="text-[11px] text-rose-600/80 dark:text-rose-400/80">"器物往往是剧情反转与伏笔信物的核心载体。"</p>
                                            </div>
                                            <button
                                                class="px-2.5 py-1 bg-rose-600 hover:bg-rose-700 text-white rounded font-semibold text-xs cursor-pointer shadow-2xs"
                                                on:click=on_add_asset
                                            >
                                                "+ 新增器物"
                                            </button>
                                        </div>

                                        <div class="space-y-2">
                                            {move || {
                                                let list = assets.get();
                                                if list.is_empty() {
                                                    return view! {
                                                        <div class="text-center py-6 text-slate-400 text-xs">
                                                            "暂无专属器物，点击右上角「+ 新增器物」"
                                                        </div>
                                                    }.into_any();
                                                }

                                                view! {
                                                    <div class="space-y-2">
                                                        {list.into_iter().enumerate().map(|(idx, item)| {
                                                            let item_name = RwSignal::new(item.name);
                                                            let item_kind = RwSignal::new(item.kind);
                                                            let item_note = RwSignal::new(item.note);

                                                            Effect::new(move |_| {
                                                                let n = item_name.get();
                                                                let k = item_kind.get();
                                                                let nt = item_note.get();
                                                                let mut cur = assets.get();
                                                                if idx < cur.len() {
                                                                    cur[idx] = CharacterAssetItem { name: n, kind: k, note: nt };
                                                                    assets.set(cur);
                                                                }
                                                            });

                                                            view! {
                                                                <div class="border border-slate-200 dark:border-slate-700 rounded-lg p-2.5 bg-slate-50 dark:bg-slate-800 flex items-center gap-2">
                                                                    <input
                                                                        type="text"
                                                                        class="w-32 bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded px-2 py-1 font-semibold text-slate-800 dark:text-slate-100"
                                                                        placeholder="道具名称"
                                                                        prop:value=move || item_name.get()
                                                                        on:input=move |ev| item_name.set(event_target_value(&ev))
                                                                    />
                                                                    <select
                                                                        class="w-24 bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded px-1.5 py-1 text-slate-700 dark:text-slate-300"
                                                                        prop:value=move || item_kind.get()
                                                                        on:change=move |ev| item_kind.set(event_target_value(&ev))
                                                                    >
                                                                        <option value="武器">"武器"</option>
                                                                        <option value="信物">"信物"</option>
                                                                        <option value="功法">"功法秘籍"</option>
                                                                        <option value="坐骑/灵宠">"坐骑/灵宠"</option>
                                                                        <option value="资产/底牌">"资产/底牌"</option>
                                                                    </select>
                                                                    <input
                                                                        type="text"
                                                                        class="flex-1 bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded px-2 py-1 text-slate-600 dark:text-slate-300 placeholder-slate-400"
                                                                        placeholder="来历、特殊效果或象征意义……"
                                                                        prop:value=move || item_note.get()
                                                                        on:input=move |ev| item_note.set(event_target_value(&ev))
                                                                    />
                                                                    <button
                                                                        class="text-slate-400 hover:text-rose-500 p-1 rounded hover:bg-slate-200 dark:hover:bg-slate-700 cursor-pointer"
                                                                        on:click=move |_| on_remove_asset(idx)
                                                                    >
                                                                        "✕"
                                                                    </button>
                                                                </div>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                }.into_any()
                                            }}
                                        </div>
                                    </div>
                                }.into_any(),

                                "secret" => view! {
                                    <div class="space-y-3.5">
                                        <div class="bg-sky-50/60 dark:bg-sky-950/30 p-3 rounded-lg border border-sky-200 dark:border-sky-900/50">
                                            <span class="font-bold text-sky-700 dark:text-sky-300 block mb-1">"🔵 身世隐秘与致命弱点死穴"</span>
                                            <p class="text-[11px] text-sky-600/80 dark:text-sky-400/80">"用来制造反转、逼入绝境以及盟友间的信任危机。"</p>
                                        </div>

                                        <div class="space-y-3">
                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"真实身份 / 隐藏身世 (Identity):"</label>
                                                <input
                                                    type="text"
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded px-2.5 py-1.5 focus:ring-1 focus:ring-sky-500 focus:outline-hidden"
                                                    placeholder="如：前朝大炎覆灭皇室留下的唯一嫡脉遗孤、魔宗圣子"
                                                    prop:value=move || sec_identity.get()
                                                    on:input=move |ev| sec_identity.set(event_target_value(&ev))
                                                />
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"致命死穴 / 功法弱点 (Fatal Weakness):"</label>
                                                <input
                                                    type="text"
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded px-2.5 py-1.5 focus:ring-1 focus:ring-sky-500 focus:outline-hidden"
                                                    placeholder="如：每月朔夜旧疾复发灵力尽失、极度畏火、不能闻到安息香气味"
                                                    prop:value=move || sec_weakness.get()
                                                    on:input=move |ev| sec_weakness.set(event_target_value(&ev))
                                                />
                                            </div>

                                            <div class="space-y-1">
                                                <label class="font-semibold text-slate-700 dark:text-slate-300">"不可告人的核心秘密 (Hidden Secret):"</label>
                                                <textarea
                                                    class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded p-2 focus:ring-1 focus:ring-sky-500 focus:outline-hidden min-h-[60px]"
                                                    placeholder="如：他一直在暗中利用女主解开封印；当年救过他的恩人其实是被他亲手错杀……"
                                                    prop:value=move || sec_secret.get()
                                                    on:input=move |ev| sec_secret.set(event_target_value(&ev))
                                                ></textarea>
                                            </div>
                                        </div>
                                    </div>
                                }.into_any(),

                                _ => view! { <div></div> }.into_any(),
                            }}
                        </div>

                        // Footer with AI Polish and Raw Note backup
                        <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-850 flex items-center justify-between text-xs">
                            <div class="flex items-center gap-2">
                                <span class="text-slate-500 font-medium">"AI 解构/润色:"</span>
                                <PolishButton module_key="character" content=raw_notes />
                            </div>
                            <span class="text-[11px] text-slate-400">"数据已自动采用结构化 ExplodedProfile JSON 保存，兼容全系统"</span>
                        </div>
                    </div>
                </div>
            }.into_any()
        } else {
            view! { <div></div> }.into_any()
        }}
    }
}
