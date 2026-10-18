Easy-writing 审校报告

审校范围：我读了仓库首页 README 和《实施计划》（含 DDL、时序图、阶段计划）。没有逐个读 Rust/JS 源码，也没读 PRD 与两份流程文档。所以下面评的是文档与设计，不是代码实现。另外，抓取到的页面里 Releases 一栏是空的，不确定是页面渲染问题还是真没发布，建议你自己确认。

整体上，设计很完整：Tiptap 不销毁实例的切章策略、40ms 流式合并、key_ref 钥匙串方案、先传图再传库的备份顺序，都是对的决定。问题主要在下面几类。

一、文档之间的不一致
README 比实施计划新，计划已过期。 README 里的 EPUB 导入、图库、词库、角色关系图谱、多作品看板，在计划里都找不到，DDL 里也没有词库、关系、章节历史版本对应的表。计划里"外部稿件导入"只写了 txt/md。
计划自己前后矛盾。 开头写"MVP 暂不含导入"，第八节写"全部已交付"，结尾却写"具备直接开展编码实现的标准"。第八节标题是"P1 功能与交付闭环"，目录里却写"P1 功能与后续排期"。建议把计划冻结为设计文档，另建 CHANGELOG 或状态页记录交付情况。
目录结构与仓库实际不符。 计划写的是 src/ 加 Cargo.workspace.toml，仓库里是 frontend/、shared-types/，根目录只有 Cargo.toml。另外 Cargo 没有 Cargo.workspace.toml 这个文件，workspace 是写在根 Cargo.toml 的 [workspace] 里的。
平台声明有出入。 README 写了 Linux Secret Service，但你定的首发平台是 macOS 和 Windows，计划里的钥匙串也只列了这两个。macOS 方面，计划写 Universal，README 只写 Apple Silicon。包体积也有 ~15MB 和 ~18MB 两个数字。
setting_change.target_type 含 'outline'，但库里没有大纲表。 大纲实际是 novel.synopsis、volume.summary、chapter.summary 三处，target_id 指向谁没有定义。
二、设计风险（按严重程度）

1. WebDAV 恢复会带来三个问题

备份的是整个 DB，其中的 llm_profile.key_ref 指向本机钥匙串条目。在新设备恢复后，这些引用会悬空，API Key 全部失效。恢复后应检测缺失并引导重新填写。
恢复会连同 app_setting（面板显隐、主题等）和 webdav_config 一起覆盖本地设置，需要决定哪些表不参与备份和恢复。
"整体替换本地 DB 文件"时，连接池和 -wal、-shm 文件必须先关闭或清理，否则会出现新旧文件混用。另外，备份版本和当前应用的迁移版本不一致时，需要先做版本检查。

2. 退出时异步备份可能没传完就退出。 进程关闭后上传可能被中断。WebDAV 的 PUT 本身也不是原子的，计划里的"原子上传"需要先传 .tmp、再 MOVE 重命名，并校验大小或哈希，否则远端可能留下半截备份。

3. 提炼去重与 action='update' 互相冲突。 dedup_key = hash(kind + norm(title))，且"自动过滤已在库中的条目"。但"更新已有角色"的候选，标题必然已在库中，会被过滤掉。此外，被忽略过的条目会被永久屏蔽，哪怕后文出现了新信息。建议 update 类候选绕过标题去重，改用内容差异判断。角色别名也建议纳入去重。

4. 切章时的保存竞态。 计划写的是"抓取草稿 → 后台保存 → set_content"。如果保存是异步的，回调里用的是"当前章节 ID"而不是调用时捕获的 ID，章 A 的内容可能写到章 B 上。要确保保存时绑定章节 ID，且 await 完成后才切换。

5. SQLite 配置。

没有设置 busy_timeout。自动保存、静默提炼、备份会并发写，会遇到 database is locked。
synchronous=NORMAL 加 WAL 保证不损坏，但断电时最近提交的事务可能丢失。README 写了"断电防崩溃"，对写作软件来说，正文保存最好用 FULL，或者在文档里说明这个取舍。

6. 版本历史会撑大数据库和备份。 每次手动保存和切章都生成快照，而整库备份会带上全部历史。计划里没写保留策略。建议：内容未变不生成快照、限制每章保留数量，并给 WebDAV 备份加保留策略。

7. 冲突检测的实现没说清。

"前置快速检测"是规则匹配还是 LLM 调用？规则抓不到语义冲突，LLM 则多一次延迟和成本。
后置复核是第二次 LLM 调用，会有误报和漏报，需要"跳过检查"的出口。
"按本章相关性选取设定"没说用什么算法（名称和别名匹配？向量？）。设定一多，"严格遵循设定"就可能因为没选进上下文而失效。

8. Asset 协议的细节。

scope: $APPDATA/images/* 只匹配一层，如果图片按作品分子目录存放，需要递归通配。
正文 JSON 里存相对路径、渲染时转换成 asset:// 的映射层，计划里没写。Windows 上的协议地址形式和 CSP 配置也要验证。
三、README 的表述
"数据完全自主"”数据绝对自主"与实际功能有张力。 提炼和对话会把章节正文、设定发送给第三方 LLM，备份会上传到 WebDAV。建议加一段数据流说明：哪些数据在什么时候发给谁，以及自动提炼默认关闭。
"完美兼容"”60fps””零虚拟 DOM 开销"这类措辞，没有基准数据支撑，建议改成可验证的描述，或附上测试条件。
如果 macOS 包没有签名和公证，建议在下载说明里写明 Gatekeeper 的放行方法。
双远程流程文档提到身份隔离的 worktree。我没读内容，建议自查一下，确保其中没有 token、邮箱等私人信息。
四、次要问题
extracted_hash 用 MD5，asset.hash 用 SHA-256，建议统一。同时要明确哈希的对象是 content 还是 content_plain。
软删除加 ON DELETE CASCADE 的组合：软删卷时，章节的 deleted_at 要手动级联，恢复时也要对称处理。README 提到的"作品级联删除"测试是否覆盖了这点，值得确认。
asset 缺少 (novel_id, hash) 索引，plot_item(chapter_id) 和 chat_session 也缺索引。
备份文件名里的时间戳建议用 UTC，避免跨时区排序混乱。
全局 Semaphore(2) 对 4 个可能绑定不同服务商的 Agent 并不精确，按 llm_profile 限流更合理。