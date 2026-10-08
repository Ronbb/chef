# Chef 协作约定

2026-10-08 CI 实际日志修复：管理员分页使用数据库动态未来边界，不写死日期；SSR 人工试听必须从可选入口进行，默认 primary 保持直接发布。Brioche maintenance 框架提交仅测试补丁，不部署多产品迁移。Hargow 已部署独立品牌准备页，serve-product-launch.mjs 无身份/DB/课程接口，不能视作正式 H serve 或完整双产品上线；真实粤语运行门槛保持。见 docs/ci-repair-20261008.md。

课程2.0公开模型：neutral::NeutralLesson及独立TS/Schema导出只Chef；结构/媒体复用旧内核，私有字段位置视图不返回或保存，角色使用真实目标locale。旧1.0仍固定法语；2.0词音cue精确对应作者Unicode范围，不用法语空格词界。当前公开模型已实现，但作者/判分、旧适配、导入/API/UI/配音仍待贯通，生产/pin与H运行门槛保持；见 docs/neutral-course-contract.md。

语言中立正文原语：ReadingText/TargetLanguage 的 Rust 真源与 TS/Schema 生成只 Chef；作者显式 Unicode scalar 词段，原文不归一化，粤拼独立注音且精确附着。词音区间不能越过作者范围，语言格式检查不是粤语/TTS质量证明。当前尚未贯通2.0课程/适配/API/UI，保持1.0及H运行关闭与生产/pin门槛；见 docs/language-neutral-reading.md。

共享 Web 产品范围：product-runtime 从可信构建配置统一 Cookie/草稿/身份通知 namespace，Hargow 必填 hargow且与产品ID一致；账号清理只删除本产品，Brioche旧键兼容。所有生产恢复/退出调用显式传该范围，不从 URL/Host/storage 选产品。实现、实际组件/SSR与协议回归只 Chef，产品不复制业务；详见 docs/product-web-isolation.md。真实粤语/完整装配待齐，生产/pin保持。

分离配音交付 CLI：alignment import、package import、lesson direct publication 先完整 author_scope 与真实 maintenance_auth，Some(product) 复用报告导入/打包快照与登记/授权事务，身份停止拒绝。package import 新 CLI 仅完整 split；原 HTTP 抽取同一内核，不复制逻辑。固定归档/哈希/实文件、不可变重试与产品范围保持，登记或授权不激活目录、不伪造 heard/timingsChecked。仅 Chef 合成隔离验证，生产/pin 不动。

分离角色声音/评价 CLI：character-voice-import、audition/clip review 先 author_scope 与 maintenance_auth 真实本产品管理员核验，再复用 append_profile_authorized_for_product / review_for_actor。声音追加、参考录音边界、试听采纳同事务、heard 显式声明、试听重复冲突/片段精确重试保持；不自动发布或伪造审听。实现和隔离合成测试只 Chef，身份停止拒绝，生产和产品 pin 保持。

分离语音生成 CLI：clip/audition generate 使用 author_scope 和真实 maintenance_auth，Some(product) Store 复用创建内核及同产品完成轮询，不构建身份 Backend 或按邮箱/全局 role 授权。local-cli 标记、成本确认、未知重试、精确重放与缓存音频验证保留；不发布或声明审听。测试只用合成供应商及无付费配置的实际 CLI 缓存/重放，生产和产品 pin 保持。

分离配音计划 CLI：preview/save 使用完整 author_scope 与 maintenance_auth 真实本产品管理员会话，不读取身份表或以邮箱授权。Some(product) Store 复用 HTTP preview_authorized/save_for_actor，固定哈希、事务权限复核与不可变重试保留；save 理由标记 local-cli，preview 只写 create_new 私有输出。实现仅 Chef，产品不增加副本；旧组合兼容、H 写入关闭、生产与产品 pin 保持。

分离本机自动打包补充：speech-package-automatic先完整author_scope，再maintenance_auth真实本产品会话核验/邮箱一致，以Some(product) Store复用assemble_authorized；不构建身份Backend或查全局role，不改旧combined/H关闭门槛。固定报告/归档SHA/计划/片段/课源全产品范围，交付前同一身份与来源事务复核；共用save_private_archive/create_new私有输出，不覆盖。实际split受限作者CLI与HTTP自动包一致，外计划/外片段/错误账号/普通成员/CSRF/identity停止无输出，六类导入/审听/直接发布表零增量及供应商计数不增加；不声明真人审听/真实对齐或粤语质量。输出/权限实现与测试只Chef，其他配音CLI/语言中立/双产品装配与最终部署继续待完成，生产/pin保持。

分离本机配音导出补充：speech-plan-export/export-direct先author_scope完整账本，再以私有CHEF_OPERATOR_SESSION_FILE的cookie/csrf和可信PUBLIC_APP_URL向独立identity核验POST，实际产品operator+邮箱一致；邮箱不授权限，无identity Backend/身份SQL/角色缓存/失败回退。私有JSON有界/严格字段，凭据不日志，内部密钥不替代会话。export_author复用HTTP export_policy的本产品快照与交付事务权限复核；完成归档后create_new私有输出，既有文件不覆盖。实际split受限角色CLI与两种HTTP归档一致，非管理员/错误邮箱/CSRF/外计划无输出、identity停止拒绝；模拟供应商计数不增加。H运维gate、其他配音CLI与完整装配仍待迁移，旧combined兼容但不用于split，生产/pin保持，实现/测试只Chef。

已有账号跨产品加入补充：独立identity account_login在真实密码认证后，以固定服务器product调用enroll_authenticated；account-admin锁→共享用户行核对认证密码摘要→缺失产品learner/version1与本人加入成员审计同事务→再建立会话。重复/并发加入不改现有role/version、不重复审计，不复制全局/别产品operator；旧密码/不存在账号/错误密码/审计错误失败关闭。身份不创建学习设置/数据，兼容组合登录保持。实际非所有者HTTP与PG核心验证仅Chef；成员与会话保存不是原子提交，不声明会话失败一定无成员。新增H成员审计使legacy29降级明确拒绝保留历史，原测试改为验证此真实门槛；普通成员加入身份环节已完成，H内容serve/完整装配、配音CLI/语言中立/生产继续待完成，pin保持。

首位产品管理员维护补充：chef-identity bootstrap-operator <existing-email> <reason> 仅独立身份schema、四张身份表实际所有者可执行，运行非所有者明确拒绝；不需要服务密钥/公开URL，不创建账号/密码。account-admin锁内检查本产品无现任/历史管理员，再锁共享账号，成员版本+成员审计+账号role历史同事务；history标记bootstrap/identity-table-owner，不冒充浏览器actor证明。并发至多一次、审计末步失败全部回滚、历史管理员不允许重复bootstrap，users与B成员指纹保持。真实CLI隔离验证属于Chef，普通已有账号加入产品仍待完成，H内容serve关闭；生产初始化/产品pin不推进。

独立身份维护 CLI 补充：chef-identity invite/reset-password 使用必填独立身份 schema 与专用非所有者身份连接，PUBLIC_APP_URL/产品只取可信配置；不得借用全局role、学习连接或HTTP证明。发行复用后台issue_operator_token事务的本产品管理员复核/邮箱锁/审计；链接只写create_new私有文件，Unix0600，文件与DB不能宣称原子，失败须检查待用令牌而不自动重发。隔离实际CLI/HTTP回归覆盖组合拒绝、最小权限、同邮箱双产品发行/审计、输出不覆盖、跨入口拒绝、H邀请仅H权限及共享重置全产品失效。首位管理员初始化/已有账号加入产品仍待完成；不更新生产或产品pin，实现与测试只Chef。

可信产品启动配置补充：共享ProductId::configured统一读取应用CHEF_PRODUCT与身份进程IDENTITY_PRODUCT，仅精确brioche/hargow，缺省B兼容既有部署；空值/空白/别名/未知/非Unicode失败且不回显内容。command在任何课源/私有文件/数据库工作前校验产品与命令，H只允许离线只读作者检查，未迁移写命令拒绝；H serve在连接DB之前明确拒绝，保留内容/身份边界原关闭门槛，不能把配置贯通当H上线。应用远端identity Client、公共课程router和图片/音频router都接同一可信product，删除启动装配固定B常量。身份进程复用解析器，非Unicode不再默默回退B。实际server/identity子进程验证非法配置不回显marker、不连接不可用DB；H serve/生成/邀请提前拒绝。26作者CLI、工作区lib20契约+64server通过（2原ignored保留）、workspace check/tests/clippy通过。本轮无PG布局/真实TTS/产品业务副本/生产改动；布局CLI、产品局部编号、真正H入口及语言中立仍待完成，产品pin和生产48课保持。

独立自动配音打包产品范围补充：speech_automatic::Store由可信后台传产品，核对Operator产品，直接输入快照、固定课源/最新版本与打包后交付复核使用同一范围，交付锁对应product状态；snapshot_direct旧包装替换为snapshot_direct_for_product。本机assemble_for_actor仍None兼容legacy32，布局CLI继续待适配。actual split加入较新H ready片段共享B生成键后，以真实当前直接输入归档和原有效合成预测生成B自动包，选择B缓存回执而无H片段，课源ID/新版本/结构及humanListeningAsserted=false/approvalRequired=false保持；有效外计划404、外片段409，六类导入/录音/审听/直接发布记录零增量，模拟供应商三计数不增加。完整split模拟克隆试听配音发布链、独立身份两项、原legacy配音计划/归档/对齐/打包回归、workspace check/tests/clippy通过。新增用例初始变量与数据库schema名冲突已改唯一名称，未放宽检查；测试预测不证明真实对齐准确度或粤语质量，没有真实付费调用。实现和详细回归只Chef，产品仅短链接；局部编号、CLI/实际产品装配、语言中立/真实H及最终部署继续，产品pin和生产48课保持。

独立审听打包产品范围补充：speech_package::Store由可信后台传产品，导出/导入核对Operator产品，前后快照读取同产品对齐、决定、固定计划/片段和课源/最新版本，提交或交付锁对应product状态。列表/游标/精确重试限制产品，临时全局包ID冲突只检查外归属并404，不读取外产品载荷；列表拒绝product参数。登记录音、课程导入和package回执同事务显式产品，复用recording::register_product_transaction与author_import::import_product_transaction，调用方持授权/状态锁；移除无调用的register_transaction和package_snapshot旧包装，legacy router None保持原32布局。actual split加入H包图前后B全列表完全一致，外对齐打包导出/导入、外包ID重试/列表404，五张录音/课程/审计表零增量、三供应商计数不增；本产品已完成导入精确重试保持，原成功包/新课源/课程审计同产品。完整split、独立身份两项、原legacy配音计划/归档/对齐/打包及模拟发布链、workspace check/tests/clippy通过，没有真实付费调用。实现/详细测试仅Chef，产品仅短链接；自动报告打包、布局CLI、局部编号、真实H及最终部署继续，产品pin/生产48课不动。

独立对齐报告产品范围补充：speech_alignments::Store由可信后台传产品，固定报告读取、计划列表/游标与审核计数、报告来源计划和最新片段、审核读取及精确重试都限定产品；空读取查询与列表拒绝客户端product参数。导入/审核核对Operator产品，锁对应产品状态，新报告/审核显式写归属；临时全局报告ID冲突仅检查外产品存在并404，不读取其重试载荷。导入归档hash使用同产品speech_export::snapshot_for_product；移除无调用的旧plan/latest/snapshot包装。package_snapshot仍仅以None适配待迁移打包内核，本机导入CLI仍None，直接自动报告导出适配继续待迁移。actual split真实隔离DB和模拟语音流水线验证H报告/计划列表/有效外计划导入/外ID重试/审核404、共享生成键外片段报告409、两表零增量和供应商三计数零增加；H图加入前后B列表完全一致，原成功报告和全部审核明确B归属，两个product查询400。完整split、独立身份两项、原legacy配音计划/归档/对齐/打包回归、workspace check/tests/clippy通过。首轮新增用例误用同名校验结果及初始片段ID，已改为原请求有效报告与生成键定位后全量复验；不放宽格式或范围检查。没有真实付费调用；详细实现与测试仅Chef、产品仅短链接，pin/生产48课保持，打包/自动流程、布局CLI、局部编号及真实H和最终部署继续。

独立配音导出产品范围补充：speech_export::Store由可信后台装配传产品；已审听与直接交付两种HTTP导出读取同产品固定计划及各生成键的最新片段，关联/撤回/评价复用已限定范围的片段投影。打包前和交付前核对Operator产品、当前权限及同产品快照，交付复核锁对应product状态；空查询契约拒绝客户端product参数。原归档上限、真实原始/修复WAV哈希校验、private/no-store及直接交付不声明人工审听保持。actual split存在较新H ready片段共享B生成键，两个B导出均选择B缓存回执且全文件哈希正确，外计划导出404、product查询400。独立身份两项、完整split模拟流水线、原legacy配音计划/导出对齐打包回归与workspace check/tests/clippy通过，没有真实付费调用。旧snapshot/snapshot_direct与本机CLI明确None，仅供待迁移legacy对齐/打包内核；完整多产品上线尚未完成。实现与详细证据仅Chef，产品仅链接记录；产品pin、生产48课及私有配置保持。

独立课程配音片段产品范围补充：speech_clips::Store由可信后台装配传产品，固定片段/文件、计划请求键列表及按生成键取最新片段限制产品；计划/复用片段/原计划、最新事件、直接与继承评价、当前/原课撤回关联同产品。生成/评价核对Operator，锁对应产品状态，精确重试只读本产品载荷；临时全局片段ID冲突只查外归属并404。新片段、submitted/缓存ready/异步供应商回执事件与评价显式产品，工作线程保留提交产品。plan_for_product/latest_for_product供新入口；旧plan/latest包装None仅供待迁移导出/对齐/打包，local CLI仍None兼容legacy32。actual split新增较新H片段共享B生成键且有真实合成测试WAV及评价，B完整最新列表不变；外片段/文件/计划片段列表/计划生成/编号重试/评价404，外previous409，三表零增量且三种product查询400。同产品无费用确认缓存复用ready/原B来源及继承评价、精确提交/评价重试、1片段/1缓存事件/1评价明确B归属，原生成两事件B归属保持，模拟供应商create/query/synthesis均不增加。独立身份两项、完整split模拟克隆试听配音发布链、原legacy配音计划/片段/导出对齐打包回归、workspace check/tests/clippy通过；没有真实付费调用。实现和详细回归只Chef，产品仅短记录；导出/对齐/打包、布局CLI、局部编号、语言中立/实际H与最终部署继续，产品pin/生产48课发布保持。

独立课程配音计划产品范围补充：admin_speech_plans::Store由可信后台传产品，课源/撤回、选项角色及最新声音、编译固定角色/声音关联、计划列表/游标/固定读取同产品。预览/保存核对Operator，保存锁对应产品状态，精确重试只读本产品载荷，新计划显式归属；临时全局计划编号冲突只检查外产品存在并404。空查询契约拒绝options/read的product参数，列表仍deny未知字段；旧本机preview/save CLI明确None兼容legacy32，待布局适配。actual split有效H结构计划固定读取与H课声音选项404、H课列表空；H课预览/保存及外计划编号重试404、无效外声音选择预览400，计划总数和模拟供应商create/query/synthesis均零增量，三类product查询400。原B编译/固定读取/单审计/精确重试/改参冲突/等待撤权及完整模拟语音发布链保持，保存计划归属B；独立身份两项、完整split、原legacy固定计划/私有/幂等/保留回归与workspace check/tests/clippy通过。实现和回归仅Chef，没有真实模型调用，产品无业务副本；片段/导出/对齐/打包接口、布局CLI、局部编号、语言中立/实际H与最终部署继续，产品pin和生产48课发布保持。

课程配音产品关联准备补充：learning_000015_product_speech_work为计划、片段、片段事件/评价、对齐、对齐评价、打包导入七表增加默认B固定产品与不可变归属；计划/片段/对齐产品候选键、计划到课程、片段到计划及复用片段、事件/评价到片段、对齐到计划、对齐评价到对齐/片段、打包到对齐/新课源均立即验证同产品外键。旧全局编号/唯一课版本/不可变审计保留，JSON声音选择及运行生成/导出/打包内核仍待范围迁移。actual split维护账本16步，迁移前七张非空表旧字段指纹保持，末尾打包约束冲突整批DDL/账本回滚，跨产品课程/计划/复用/事件/评价/对齐/打包边拒绝且H合成同产品含片段复用图正例回滚；所有归属更新和未知产品拒绝。原撤权计数仍验证全表，期望改为已有样本基数加本次合法写入；完整模拟克隆试听/课程配音发布链路与独立身份两项、workspace check/tests/clippy通过。后台历史最后六类计划/片段/对齐/打包分支使用可信产品；永久H合成图加试听/查询记录后B全历史分页完全不变。SQL、接口与回归仅Chef，结构样本不表示真实粤语/TTS质量或完整运行隔离；H业务/JSON固定声音/局部编号/运行配音接口及CLI继续，产品pin与生产48课发布保持，没有付费调用。

独立角色试听产品范围补充：voice_auditions::Store由可信后台装配传产品，列表/游标/固定读取/私有WAV与任务、授权、最新事件和评价关联同产品，product查询拒绝。创建核对Operator产品，同产品系统角色/当前声音与固定克隆任务/声音版本；临时全局试听ID冲突只查外产品存在并404，不读取外产品重试载荷。新试听、submitted/异步回执事件和评价显式归属，采纳调用同产品声音追加内核；删除无调用的旧load与authorized声音包装，local CLI仍明确None待布局适配。后台历史对已产品化的角色声音/素材录音审计/参考授权撤销/任务查询/试听决定九类加固定产品条件，课程配音计划/片段/对齐/打包历史仍待对应图迁移。actual split25条H ready试听携带真实合成测试WAV收据及评价，B列表分页和全部历史分页保持；外试听读取/文件/有效外克隆/系统角色生成/外ID重试/采纳404，四表计数零增加、模拟供应商三计数不增，产品参数400。B系统候选异步ready、精确重试不再合成、退回及明确1试听/2事件/1评价归属通过；原B克隆采纳与角色撤权全链路保持。独立身份两项、完整split、原legacy参考录音/克隆/系统试听回归及workspace clippy通过。首轮列表SQL条件位置错误已修复并完整复验；没有真实付费调用。产品无业务副本，生产pin与48课发布不动；课程配音图、局部编号、真实H业务和最终部署继续。

独立音色任务接口补充：voice_jobs::Store由可信后台装配接收产品，列表/游标、固定任务及授权/最新事件关联同产品；客户端product查询拒绝。提交和查询恢复核对Operator产品，授权/撤销/访问次数及重复任务查询同产品，新任务/事件显式归属；异步供应商回执使用提交时同一产品锁与版本CAS。旧load包装None供待迁移试听调用，legacy32保持。actual split插入25个排序靠前H任务后B整列表/分页不变，外任务读取/有效外授权提交/查询恢复404且任务事件零增量、模拟供应商create/query/synthesis均零额外调用，产品参数400；原身份与后台克隆试听配音全链路、workspace check/tests/clippy通过。仅隔离PG和模拟供应商，未付费调用；试听/课程配音及历史范围、局部编号、真实H业务继续，产品pin/生产保持，业务与回归仅Chef。

音色任务与试听关联准备补充：learning_000014_product_voice_work为clone job/event与audition/event/review五表添加默认B固定产品与归属不可变保护；任务到同产品授权、事件到任务、克隆试听到固定任务事件版本、系统试听到角色及保存声音版本、试听JSON参考录音及事件/评价到试听/角色/声音均立即验证。生成base_profile_revision保留系统版本0，可选参考录音保持；旧全局编号/不可变审计保留。actual split账本15步，五表迁移前非空旧字段指纹保持，末尾评价约束冲突整批DDL/账本/生成列回滚；双向外产品任务事件、错误克隆版本/系统声音版本、跨录音和评价边拒绝，H合成克隆/系统0/接受评价正向链回滚。独立身份原审计降级保护单独事务验证，新布局阻止旧试听列降级；原身份/后台克隆试听配音全链路与workspace clippy通过。运行任务/试听/课程配音及历史范围、局部编号、真实H业务继续，生产pin保持，SQL/回归仅Chef。

独立参考录音接口补充：voice_references::Store由可信后台产品装配，临时delivery产品取固定identity Client；列表/游标及撤销/访问统计同产品，签发读取同产品声音与录音、有效授权冲突同产品；签发/撤销核对Operator，新grant/revocation/read显式归属。delivery token/expiry预检先限产品再取得account-admin锁，事务内撤销/32次上限/到期复检同产品，原真实hash/解码/PCM/Range与角色复核保持；legacy None仍原布局。actual split25条排序靠前H授权不改变B整列表/分页，H声音签发/授权撤销404且三审计表零增量；已知有效H bearer在持有账号管理锁时及时404，产品查询400，原B授权/撤销/到期/角色撤权/克隆试听配音与身份全链路、workspace clippy通过。任务/试听/课程配音及审计历史范围、局部编号、真实H业务继续，生产pin保持，实现仅Chef。

独立角色库接口补充：character_voices::Store由可信装配接收产品，列表/游标、角色固定版本、头像与声音固定版本限制归属且关联同产品声音/素材，查询product拒绝。HTTP声音写入核对Operator产品，同产品角色/声音CAS/参考录音查询，新声音显式product；角色新版本复用media内核传相同产品、状态锁/头像/审计归属，临时全局角色编号冲突仅检查外产品存在并404。旧CLI/试听调用authorized_in包装None仍待迁移。actual split25条靠前无效H角色/声音不改变B整列表/分页，三种外产品读取404、四种product查询400、外产品角色与声音写入404且记录/素材审计零增量、有效H参考录音不能追加B声音；原版本/CAS/权限/头像/克隆试听配音与独立身份全链路、workspace clippy通过。参考录音接口/任务/试听/配音与历史范围、局部编号、实际H业务继续，生产pin保持，产品无角色库业务副本。

角色声音关联准备补充：learning_000013_product_voices给character_voice_profiles与参考录音grant/revocation/read四表添加默认B的固定产品枚举和不可变归属；声音到同产品角色、声音JSON referenceAudio经生成列到同产品录音、授权到同产品声音/录音及撤销/访问到同产品授权均立即验证外键。可选referenceAudio=null保持，半个引用拒绝；旧全局编号/内容与审计不可变保护保留。actual split维护账本14步，迁移前四张非空表旧字段指纹保持；最后访问约束冲突整批DDL/账本与生成列回滚，跨产品角色/参考音频/授权/撤销/读取拒绝，H数据库合成正向链及无参考声音事务回滚。身份与原后台克隆/试听/配音全链路、workspace clippy通过。运行角色/参考录音接口、任务/试听/课程配音与审计历史范围、局部编号等待继续，H后台/生产pin保持，SQL与回归仅Chef。

公开媒体产品范围补充：media/recording新增可信product_router，注册对象与授权published课源同产品，撤回记录也匹配归属；图片同时补齐撤回保护，历史已发布且未撤回版本保持可访问。空查询契约拒绝product参数，header不选择产品。独立command仍固定B，并装配带范围媒体；legacy None保持原布局。学习角色audio_assets仅SELECT且明确撤销写入。actual split受限连接与真实SVG/解码WAV验证未发布404、外产品课源不能授予文件访问、双向跨产品404、自有200/no-store/same-origin/精确字节、音频206精确Range、撤回404且B图片200、产品参数400及录音写权限拒绝；学习/录音/身份后台全链路与workspace clippy通过。角色声音/参考录音/配音图及审计历史、全局编号、实际H入口等继续，生产pin保持，产品无媒体实现副本。

课程录音引用补充：hydrate_source_for_product与发布/试听/直接授权录音校验使用可信产品；课程导入另核对嵌入audio描述归属，防止绕过audioRefs。旧CLI包装None保持兼容。actual split以文件、解码及来源都有效的H录音验证跨产品引用/嵌入预检分别定位audioRefs/audio revision，导入400且课源/审计零新增；原不可变登记、独立身份与后台完整回归通过，workspace clippy无警告。公开音频、声音/配音关联、审计历史及局部编号继续；H后台/生产pin保持，实现只Chef，产品无录音业务副本。

独立录音登记接口补充：admin_recordings接收可信产品，列表/搜索/分页及文件读取固定产品；上传核对Operator产品、锁对应状态，register_product_transaction显式写录音与导入审计归属。旧全局编号冲突仅查存在，不加载外产品descriptor/provenance；本机CLI/配音交付register_transaction仍None待迁移。actual split25个靠前且描述无效H录音不改变B整列表/分页，搜索空、文件404、product查询400；外产品冲突409且原记录不变/审计零增加，B上传归属与单审计/重复冲突/文件/撤权及原不可变登记回归保持。课程语音hydrate/validate、公开音频与声音/配音关联、审计历史及局部编号继续，H后台/生产pin保持，实现只Chef。

录音归属准备补充：learning_000012_product_recordings为audio_assets/audio_import_audit添加默认B的固定产品枚举与归属不可变触发器，录音增加产品复合候选键；旧全局编号及不可变内容/审计保护保持。actual split维护账本13步，真实解码合成音频在迁移前登记，两张已填充旧字段指纹保持；末尾索引冲突整批DDL/账本回滚，改归属与未知产品拒绝、H数据库合成登记正例回滚。独立身份与后台全链路回归保持；运行录音API/hydrate/validate、声音/配音关联和全局ID仍待迁移，H后台与生产pin保持，SQL/回归只Chef，产品无迁移副本。

课程图片与角色引用补充：hydrate_source_for_product和validate_lesson_detailed使用可信产品限制素材描述及角色快照；课程导入/上传预检和目录检查/暂存/激活传相同上下文。导入另检查projected media/cast归属，防止嵌入描述绕过assetRefs，保持草稿与完整发布检查分离。public legacy hydrate/validate包装None用于旧CLI兼容。actual split存在且文件有效的H图片引用及嵌入描述预检分别定位assetRefs/media revision，导入400且课源/审计零新增；H角色引用预检定位cast revision、导入400且零新增，原B导入发布保持。录音引用与校验、公共媒体、角色语音入口/关联、审计历史与局部编号继续，H后台/生产pin保持，实现仅Chef。

独立图片素材接口补充：admin_assets::Store接收可信产品，列表/搜索/游标分页及文件读取固定产品；上传核对Operator产品，复用media导入内核锁对应状态，新图片/角色/导入审计显式归属，头像查询同产品。全局素材/角色编号冲突规则暂保留；本机CLI及角色语音入口仍None待迁移。actual split插入25个排序靠前且descriptor无效的H素材，B列表整JSON不变、搜索为空、文件404、客户端product查询400；原B上传归属/单审计及重复409/文件/撤权回归保持。素材hydrate/validate、公开文件/角色语音关联、审计历史与产品局部编号继续，H后台/生产pin保持，产品无素材实现副本。

图片与角色归属准备补充：learning_000011_product_visuals为media_assets/character_revisions/asset_import_audit添加默认B的固定产品枚举与不可变归属触发器；图片/角色复合候选键和角色到同产品头像外键立即验证。全局编号及原不可变内容保护暂保留，运行登记/查询/hydrate/validate和声音关联仍待迁移，不能开放H后台或切生产。actual split维护账本12步，三张已填充旧字段指纹不变；最后角色约束冲突整批DDL/账本回滚、双向跨产品头像具体外键拒绝、H合成登记正例事务回滚及改归属拒绝。身份/独立后台全链路保持；SQL与回归只在Chef，产品无迁移副本。

独立课音频发布门槛补充：lesson_audio_reviews::Store由可信装配接收产品，课程/撤回读取、状态锁、试听决定、直接授权/相同重试及后续拒绝覆盖均固定产品，新增决定与授权显式product_id。admin::approved与音频accepted核对同产品记录，目录检查/暂存/激活、概览及编辑审核传同一上下文。actual split外产品音频状态/试听写入/直接授权404且两表计数不变；B直接授权不伪造试听、后续拒绝覆盖、正向试听/CAS/重试/撤权与发布保持。旧CLI authorize_local仍None待迁移；录音validate/素材归属及配音交付/产品局部ID尚待，H后台/生产pin保持，产品无门槛副本。

独立私有预览补充：preview::Store接收部署装配的固定产品；目录/条目/课源/撤回、课程详情与预览判分只读取同产品，图片与音频先走同产品课源门槛再访问文件。actual split跨产品私有目录、课源、真实图片引用、音频路径及判分404，原B草稿预览/私有字段移除/文件与Range/no-store/撤权回归保持。legacy None不访问新列；仍待媒体录音注册归属、审批音频下层、CLI/配音交付和产品局部ID，H后台/生产pin保持，实现只Chef。

独立课程导入补充：HTTP与lesson/check传可信Store.product，import_operator核对Operator；导入与重试查询同产品，新课源和导入审计显式产品。全局课ID仍保留，check_owner只读归属并在全局导入锁前后检查，禁止把外产品私有课源作为重试；上传外产品编号返回valid=false，导入404且外课源/导入审计不变。原B导入及相同重试仅一个审计，legacy CLI/配音交付仍None待迁移。素材录音内核/私有预览/产品局部ID/H业务及生产尚未完成，产品无导入副本。

独立暂存补充：stage_operator核对可信产品与Operator，状态锁及checked_entries课源/撤回固定产品；release/check使用同一上下文只读。目录/条目/审计显式产品，hash/原子写入和全局目录编号冲突保持。actual split外课源暂存404、上传valid=false、目录/条目/审计零增量，B正向发布与权限保持。审批/音频/素材下层、CLI/导入/预览及媒体仍待产品化，H后台/生产保持。

独立目录激活补充：activate_operator接收可信Store.product并核对Operator产品，内核状态锁、目录存在、条目/撤回/课源读取、发布标志/状态/代数及审计同产品；保留审批/课音频/素材门槛。actual split跨产品目录404且状态/课源/撤回/审计指纹不变，B原子切换回滚硬撤回及失效管理员保持。下层审批音频/素材、暂存/导入/CLI/私有预览和媒体尚待产品化，H后台/生产pin保持，实现只Chef。

独立撤回补充：HTTP传可信Store.product，withdraw_operator核对Operator产品；共用kernel锁对应状态、过滤课源更新、撤回/审计显式产品、只增该状态generation。actual split跨产品撤回404且双状态/课源/撤回及审计指纹不变，B硬撤回学习保护回归保持。旧本机CLI/组合入口仍None，固定产品CLI/旧选择器移除、导入目录写入/媒体/私有预览待继续，H后台/生产pin保持，产品无内核副本。

独立编辑审核补充：Store.state_selector共用概览与审核，远端锁产品状态；lesson/withdrawal/latest editorial decision/retry actor均固定产品，新审计显式product_id，None兼容legacy32。actual split已/未发布H课程在B审核入口404且无审计增量，原B正向/CAS/重试/撤权保持；audio accepted、导入/目录写入/撤回/私有预览及媒体仍待产品化，H后台保持拒绝，生产pin不变，产品无审核实现副本。

独立管理员课程读取补充：Store.product取可信Client，legacy传None；概览按固定产品读状态/课程/目录，撤回与编辑审核同产品，条目计数同产品。history仅已具product_id的五类课程记录加范围；媒体/音色/素材等历史仍待迁移。actual split加入H合成课程/目录/发布审计，B概览与history JSON不变、H独特搜索为空；空H私有document不进入B解析。写入/预览/audio accepted及媒体仍待产品化，H后台保持拒绝，生产pin不变，实现只Chef，产品无后台副本。

产品课程锁补充：learning_000010_product_locks安装固定schema/search_path=pg_catalog SECURITY DEFINER chef_lock_product_lesson(TEXT,TEXT,INTEGER)，仅FOR SHARE返回是否存在，不返回课源；PUBLIC EXECUTE撤销，split附加授权显式授学习角色。lock_lesson使用可信LearningStore.product，Some缺课404，None仅legacy旧函数；所有学习/收藏/复习写入传产品。独立ready要求新课程/状态锁。actual split受限学习角色验证外产品在持有课行写锁时false且不阻塞、同产品触发lock timeout、释放后true，身份角色拒执行。全局课ID、旧函数移除、媒体后台及真实粤语仍待迁移，H入口/生产pin保持，产品无锁实现副本。

课源数据库约束补充：learning_000009_product_sources为learning_sessions/review_cards/saved_items及lesson_progress.latest_completed_revision建立同产品课源复合外键，安装时立即验证已有行，不使用NOT VALID或自动改归属。固定维护命令整批原子DDL/账本；冲突回滚和旧九表字段指纹由actual split验证。H合成事实必须引用独立H课源，三类跨产品引用和不存在的已完成版本由具体外键拒绝，正向H九事实在事务中验证后回滚。产品局部课ID、课程锁、媒体后台及真实粤语仍待完成，H入口/生产pin保持，SQL仅Chef。

学习课源范围补充：远端学习load/start/history、收藏source/detail/list/history和复习load/queue/count/cards同时限制事实与关联lesson_revisions的固定产品，legacy None保留原布局。收藏创建/复习enroll显式传LearningStore.product到共用source；外产品公开已发布课源不能生成当前产品事实。受限split HTTP验证B收藏/加入复习/开始课程对真实存在的H合成课源404且三类事实零增量；完整课源复合外键、有限课程锁/global编号及媒体后台范围仍待迁移，H认证业务关闭，生产pin保持，实现仅Chef。

公共课程API产品范围补充：independent_product_router由可信部署装配传ProductId，Extension固定目录与当前/指定版本详情范围，客户端query/header不能选产品。受限actual split HTTP覆盖跨产品ID两路径404、自有H公开详情200且无私有字段、伪造header目录保持、product查询400；H禁止回退法语演示判分/fixture。independent_learning_router仍固定B，H认证业务关闭；合成法语夹具不代表粤语能力。后台/媒体、学习事实到课源约束及全局编号仍待迁移，生产pin保持，产品不复制API。

目录读取产品范围补充：catalog_matching_for_product共用查询按可信Option<ProductId>选状态、同产品目录/条目/课程/撤回，legacy catalog_matching传None仅原B查询；首页目录/推荐使用固定产品并过滤撤回，无目录返回空不回退。actual split受限学习连接读取双发布合成法语夹具，H返回独立summary、独特搜索只H、H撤回事务不影响B、B整目录指纹及HTTP首页目录/推荐保持。H夹具不是真实粤语课程。公共catalog/lesson API上下文、后台/global课ID/媒体及legacy选择器移除仍待实施，H业务保持关闭，生产pin不变；实现仅Chef。

发布状态产品范围补充：learning_000008_product_release_state将content_state主键改为product_id；singleton暂为legacy Brioche选择器，约束为等于(product_id=brioche)，H插入false，后续全部调用迁移后移除旧选择器。新chef_lock_product_release_state(TEXT)固定schema/pg_catalog SECURITY DEFINER只FOR SHARE，PUBLIC EXECUTE撤销，split-only learning-product-grants显式授运行角色；远端开始课程用可信产品调用，legacy旧函数保持。actual split验证双状态/读取H目录/旧B选择器单行、Hgeneration改17 B指纹不变、B引用H目录拒绝、身份role拒执行及双状态下B学习/内容后台回归。全局课ID/媒体及后台产品scope/目录查询仍待迁移，H入口和生产pin保持，不声明完整租户。

课程内容归属准备补充：learning_000007_product_content为课程版本/目录/目录条目/状态/撤回/发布审计及导入/编辑/课音频/直接发布审计十表添加默认brioche的product_id和归属不可变trigger。课程/目录增加产品复合候选键，条目/状态/撤回/相关审核引用同产品父记录；原全局ID/主键/singleton仍保留，运行查询及媒体/学习事实到课源外键尚未产品化，禁止开放H。actual split维护命令核对十表旧字段指纹，故意中途trigger冲突整批回滚、异产品条目引用拒绝、同产品H课/条目正例回滚和lesson改归属拒绝。准备不等于完整内容隔离，生产/pin保持，SQL/回归仅Chef。

首页学习统计产品范围补充：dashboard按固定LearningStore.product读取设置、日历步骤/答题/评分/首次完成事件、课程状态/resume、完成数、复习到期/下次时间及推荐课learned标记。关联步骤/进度要求同产品，legacy保留旧32 SQL。actual split受限HTTP放入同账号异产品全部事件/同课进度/到期和未来复习，B仅自身步骤与评分，0异产品答题完成、单独resume、待复习与next时间不串、推荐未完成。目录仍全局，H路由继续关闭；完整内容媒体/粤语/双产品生产待完成，生产pin不变，产品无实现副本。

复习产品范围补充：learning_000006_product_reviews将卡片知识去重和队列/历史索引加入product_id；队列计数/详情/评分/暂停/卡片列表/历史按可信LearningStore产品，历史同产品关联，评分显式登记产品并读取当前产品设置时区。手动enroll与课程完成共用review_conflict兼容legacy32。actual split受限HTTP异产品详情/评分/暂停404、异产品历史不返回、同知识双卡独立/登记重放、B队列和评分重放/暂停不改H版本stage与attempt数。完整Hargow路由仍关闭；dashboard/目录媒体范围、双产品生产仍待实施，生产pin不变；SQL与业务仅Chef。

收藏产品范围补充：learning_000005_product_saved将saved_items去重改为(product_id,user_id,knowledge_id)并增加产品近期索引；详情/列表/锁内读取/新建/取消及请求锁按固定LearningStore.product，旧组合布局保留原查询。actual split受限HTTP预放同账号同表达H收藏，B详情404、新建独立ID、精确重放、列表仅B、取消B版本递增而H仍saved/version1。旧九表指纹保留；复习参与/卡片/历史/完成去重、dashboard及内容媒体仍待隔离，H入口/生产pin保持；所有SQL和回归仅Chef。

学习会话产品范围补充：layout learning_000004_product_sessions为活动会话唯一索引、课程进度主键加入product_id；远端开始/读取/步骤/答题/提示/完成及历史按可信LearningStore产品读写，legacy保留旧32布局。实际split受限HTTP覆盖同账号同课双产品活动会话共存、B首次完成字段不继承H、H会话读/四类写404且版本/时间不变、B历史和成功步骤登记。原九表字段指纹保留。dashboard、收藏/复习查询与完成时review_cards全局去重、目录/媒体仍待隔离，Hargow业务保持关闭，生产/pin不变；所有实现测试属于Chef。

幂等产品范围补充：独立学习LearningStore由可信Client.product构造Some(product)，共用replay/record及全部学习/收藏/复习调用显式传入；legacy new只兼容未分离Brioche，不能在split布局启动。layout learning_000003_product_operations主键增加product_id，原scope/key/hash/result不变。实际受限split HTTP预放同账号同key异产品记录，B忽略异产品结果/hash、独立登记、精确重放及改参409通过。identity_service合成fixture明确安装同一SQL步骤，真实命令/rollback在split套件。其余事实和内容媒体仍未隔离，Hargow关闭，生产pin不变。

学习事实产品准备补充：layout learning_000002_product_facts为九张事实表添加默认brioche的product_id、同产品会话/卡片复合外键和禁止改归属trigger，保留全部原字段/全局唯一键。SQL文件定义仅CRLF→LF规范。非空九表指纹、同产品九表正例事务回滚、跨产品父引用/改归属拒绝及步骤中途失败整批回滚验证属于Chef。此阶段未完成查询/RLS/内容/媒体租户，Hargow路由仍关闭，禁止生产迁移或把准备当完整隔离；产品无迁移副本。

共享Web直接发布补充：LessonAudioReview默认direct-publication授权，人工heard仅可选试听动作写audio-review；固定Rust DTO，evidence仅admin-web来源及humanListeningAsserted=false。attempt锁定kind/body，未知结果保留、重试同一路径/载荷、离开拦截；明确校验拒绝才释放，已授权版本阻止再次表单授权。授权不会自动激活目录。受控浏览器验证直接请求/503精确重试/离开保留/成功恢复与独立人工试听/409释放；typecheck/unit/SSR/build通过。实现与回归只在Chef，产品pin与生产不变。

自动打包补充：speech_automatic的HTTP与本机CLI共用仅内容db/Operator kernel，本机入口才读取成员身份。新增automatic POST及export-direct GET，AdminAutomaticSpeechPackageRequest由Rust生成TS；报告JSON拒绝重复字段，4MiB报告/5MiB请求、两媒体槽与原128MiB归档限制保留。计算前/交付前lock_content复核请求，交付前快照比对，不写人工heard或激活目录。受限split-schema验证私有自动tar/固定报告/learner与CSRF403及二十五类等待后撤权，模拟协议不表示真实模型准确度。完整租户/语言中立/粤语、共享Web入口及生产装配仍待完成，生产pin不变。

分离布局升级补充：migrate-layout只接受已登记split布局和精确legacy32历史，由相关表所有者运行；身份schema从布局读取，固定步骤及独立chef_layout_migrations属于Chef。共用维护锁，整批DDL/账本原子提交，拒绝未知/漂移定义、同名未登记索引及运行角色，不自动逆向或服务启动执行。实际CLI/受限PG验证冲突整批回滚、正常/幂等、身份行指纹和漂移拒绝。生产pin不变；完整产品事实、语言中立/粤语和生产装配仍待实施。

独立私有预览补充：preview共享路由使用db-only Store/AdminAuth并入content_router，legacy移除重复装配。固定release/revision/私有media/audio/grade共用原投影/读文件/判分实现，URL仍改写operator路径，来源与私有答案不返回。POST判分增加Operator事务锁内复核，只计算结果，不写学习事实；保持CSRF与撤回410。实际分离schema/受限角色验证未发布新音频revision预览、私有SVG、WAV range206、learner403/匿名401、撤回410/目录标记、学习session/attempt零增量、二十三类等待后撤权403及identity停止预览/媒体503。完整租户/语言中立与粤语、布局感知后续迁移/生产装配仍未完成，不更新生产pin。

独立打包与课音频授权补充：speech_package和lesson_audio_reviews采用db-only Store/AdminAuth进入共享content_router；打包/导入前后Operator.lock_content复核原请求，最终content_state快照比对，保持原子录音+新draft课源+package审计、fixedhash/CAS/精确重试。课音频read/review事务授权迁出身份Backend；新增direct-publication POST复用原本机授权kernel，CLI只在边界取proof，AdminDirectPublication真源Rust生成TS，审核与直接授权分开，不伪造heard。内容角色仅新增package_imports/audio_reviews/direct_publications INSERT。实际受限schema验证完整模拟包导出/原子导入/列表/幂等/无公开新课、无heard的直接授权/后续拒绝失效旧授权/音频审核版本/二十二类等待后撤权403，无额外包/决定/调用。人工字段与时间均fixture，非真实生产审批。其余私有预览、布局感知迁移、完整租户/语言中立/粤语/生产装配仍待完成，不更新生产pin。

独立导出/对齐补充：speech_export/speech_alignments使用db-only Store/AdminAuth进入共享content_router，CLI仅在边界构造本地Operator。导出打包前/交付前均lock_content复核原请求，交付前content_state锁重新比对snapshot；保持私有no-store/128MiB/原始与规范化音频hash验证。对齐导入/审核事务先lock_content，固定引擎/报告hash/原课音频/words/sourceArchiveSHA/CAS/精确重试不变。内容角色仅新增speech_alignments/reviews INSERT，无身份表/UPDATEDELETE。实际受限schema模拟完整计划课音频后验证tar完整hash/对齐导入和审核/列表读取/幂等/协议字段及十八类等待撤权403零额外报告/决定/合成；合成时间/人工字段仅测试fixture，非真实对齐/审听。余下打包/发布/私有课程预览/完整租户/粤语仍待迁移，不更新生产pin。

独立课程语音片段补充：speech_clips共享路由采用db-only Store/AdminAuth；HTTP生成/审核传Operator到kernel，事务先lock_content再content_state。CLI只在边界构造本地proof，worker仅内容db。保留先登记submitted再调用、未知不自动重试、固定计划hash/previous CAS、精确重试、音频哈希验证和复用审计；审核仍为显式协议，不自动伪造heard。内容角色新增clips/events/reviews SELECT/INSERT，无身份权限或片段UPDATE/DELETE。真实分离schema模拟WAV验证生成/私有读取/列表/精确重试不重发/改参409/审核精确重试/复用ready与继承决定，十五类请求等待后撤权拒绝且零额外记录/调用。合成heard只是测试fixture；余下导出/对齐/打包/发布/私有预览、完整租户与粤语仍待迁移，保持生产pin。

独立课程配音计划补充：admin_speech_plans采用db-only Store/AdminAuth进入共享content_router；HTTP预览/保存向kernel传可信Operator，事务先lock_content再编译/内容锁。CLI只在边界从本地身份db构造proof，复用kernel。保持固定角色/声音/课源选择、plan hash、actor与reason、精确重试和撤回过滤；编译/保存不调用供应商。内容角色仅增加course_speech_plans INSERT，不授UPDATE/DELETE。真实分离schema验证选项/编译/保存/读取/列表/精确重试/hash冲突及十三类等待后撤权拒绝，原模拟供应商调用数不变。余下片段生成/对齐/打包/发布/私有预览与完整租户仍需迁移，不更新生产或产品pin。

独立试听补充：voice_auditions共享路由db-only Store/AdminAuth，HTTP创建/采纳传可信Operator，事务先lock_content；local CLI仅在入口从身份db取proof，共用内容kernel，不把Backend带入worker。采纳调用character_voices::append_profile_authorized_in同事务复核并追加配置，旧CLI append_profile_in仍保留本地复核。content-grants新增试听/事件/采纳SELECT/INSERT与仅audition UPDATE(id)锁权限，原不可变trigger保留。真实分离schema模拟合成WAV验证生成/私有读取/精确重试不重发/修改重试409/采纳新voice revision/重复采纳409与十一类等待撤权拒绝，无新试听/调用。合成人工字段仅协议测试，不声称生产人工审听；剩余课程语音流水线/私有预览及租户待迁移，不更新生产pin。

独立音色创建任务补充：voice_jobs的列表/读取/创建/状态查询进入共享content_router，db-only Store/AdminAuth；创建和查询先Operator.lock_content。创建不再JOIN身份成员表，远端通过原可信proof内的身份client检查参考授权人的固定产品权限；legacy复用本地产品成员读取。供应商调用仍在提交submitted/checking审计后触发，响应worker只持内容db，保留版本CAS/未知结果不自动重试。content-grants新增jobs/events INSERT与仅jobs UPDATE(id)行锁权限，原不可变trigger保留。真实分离schema模拟供应商回归验证提交→processing→ready、旧版本/重复409、九类等待撤权403无额外事件/调用、不同有效管理员消费已撤权授权人的参考404。其他试听/语音流水线/私有预览与租户仍待迁移，不更新生产或产品pin。

独立参考录音补充：voice_references管理员列表/签发/撤销使用db-only Store/AdminAuth，签发与撤销事务先Operator.lock_content；临时bearer下载单独delivery_router在浏览器gate外，仍用token hash/期限/撤销/32读上限和account-admin锁。远端下载通过凭证保护GET /internal/v1/operators/{stored_actor}检查固定产品成员，204允许/404拒绝/其他503，有界无缓存HTTP；不能要求供应商登录，也不能读取内容连接身份表。身份服务最终响应统一private,no-store含fallback；内部operator接口无浏览器会话或全局role授权。内容角色新增reference三表SELECT/INSERT、reads序列与仅grant UPDATE(id)用于FOR UPDATE，原不可变trigger保留，禁止实际更新/删除或扩大身份权限。角色产品仍仅Brioche；其他配音流水线及课程预览待迁移，不切生产。

独立角色库补充：character_voices共享路由采用db-only Store/AdminAuth；角色追加必须传Operator到media导入，声音HTTP写事务必须先lock_content再调用私有append_profile_body。公共CLI append_profile/试听采纳append_profile_in保留本地成员事务复核，共用私有写实现；不得直接暴露body或接受客户端actor。content-grants增加character_revisions/character_voice_profiles INSERT，不授UPDATE/DELETE。实际分离schema验证新角色、第二revision、旧revision/头像/声音读取、声音CAS冲突/actor审计、learner/CSRF拒绝、五类写入各自等待后撤权403零登记，身份停止503。语音任务/参考授权/试听/课程预览仍依赖legacy；不推进生产或Hargow开放。

独立素材/录音后台补充：admin_assets和admin_recordings使用仅db的Store与AdminAuth，legacy和远端共用content_router，不初始化身份Backend。上传必须传内部Operator证明至共享导入事务，先account-admin锁再原请求身份复核再content_state锁；保留CLI可信导入与不可变审计。content-grants新增media_assets/audio_assets及对应导入审计INSERT、审计序列USAGE，无身份或学习权限、无素材UPDATE/DELETE。实际分离schema回归覆盖两个上传、私有读取、真实actor审计、重复冲突、learner/CSRF拒绝、逐项撤权等待与身份不可用503。角色/音色流水线及课程预览仍需迁移，不能据此切生产或开放Hargow内容。

远端课程后台补充：admin核心课程路由使用无身份Backend的Store和AdminAuth，legacy复用；远端Operator仅由已验证请求生成，写事务先取得同一PostgreSQL数据库的account-admin锁，再向identity复核原method/Origin/CSRF/账号/产品成员，随后锁内容。禁止只用入口缓存权限或用globalrole。CONTENT_DATABASE_URL和必填CONTENT_DATABASE_SCHEMA可装配独立课程运行连接，必须与identity/learning同一实际数据库，缺少远端身份时拒绝；不配置则不开放该后台。content-grants只支持核心课程写入与内容历史读取，无身份/学习事实权限。录音/媒体/音色等后台仍需迁移，Hargow内容仍明确拒绝，不能切生产或宣称完整后台已独立。

实际schema分离补充：迁移32只创建不可变chef_schema_layout；split-identity-schema是迁移所有者维护命令，要求精确32版本、单schema连接与七表所有权，事务内创建全新身份schema并移动七表，保留ID/序列/外键/会话/审计。迁移完成后禁用旧combined serve/migrate，32回滚拒绝删除已分离布局。授权模板必须显式传入身份和学习两个schema；产品不得复制迁移或授权实现。隔离真实迁移/CLI/双服务回归已覆盖，生产仍不能切换：远端内容后台、后续schema感知迁移、完整产品事实与Hargow未完成。操作边界见docs/database-schema-split.md。

运行数据库配置补充：IDENTITY_DATABASE_SCHEMA只用于独立身份进程，DATABASE_SCHEMA用于学习/兼容CLI；仅允许单个小写ASCII标识符，不能由Host、请求或客户端产品选择。缺省沿用旧部署连接默认值。identity-grants.sql提供专用非所有者身份角色权限，审计仅SELECT/INSERT，禁止访问学习/内容；实际HTTP回归必须使用该模板和真实角色，不以所有者连接代替。配置与角色验证不是物理schema迁移，内容后台仍需迁出本地AuthSession后才能切生产。

学习持久化边界补充：远端学习进程只初始化LearningStore，不持有身份Backend、密码服务或会话存储。设置读取只查product_user_settings；账号有效性由身份内省负责，未知账号写入仍由外键拒绝。独立学习ready不读取身份表。迁移31提供固定schema、固定search_path、撤销PUBLIC EXECUTE的内容行锁函数，学习数据库角色无内容UPDATE权限；授权模板和实际受限角色回归属于Chef。此阶段仍未分离物理schema或完成Hargow事实隔离、远端内容后台及生产装配；禁止推进生产迁移或产品pin。

通用学习框架与独立身份服务真源。产品品牌与正式课程属于各自仓库，不把 Brioche 的法语绑定作为未来语言中立契约。

当前先抽取共享契约，暂保留 `brioche-course-contract` / `@brioche/contracts` 包名与 v1 法语字段以兼容既有不可变课程。这是明确的迁移阶段，不代表粤语支持已经完成。改变公共契约须通过版本化适配与 Rust 生成的 Schema/TS，不手改生成物。

tests 的法语样本仅兼容 fixture，不是 Chef 拥有的正式课程。所有产品依赖固定已验证提交；身份服务、学习 API、数据产品隔离和完整引擎抽取尚待实施，边界见 docs/architecture.md。秘密、生产数据与私有声音资料不得提交。

## 后端抽取阶段

学习/账号/管理/配音实现与 SeaORM 迁移已迁入 `crates/server` / `crates/migration`；`chef-engine` 提供通用入口，产品只调用入口。所有数据库回归属于框架，使用隔离 PostgreSQL，不允许读取生产配置。`curriculum` 子模块固定法语课源版本，仅用于兼容测试；图片和少量旧例子是明确的测试 fixture。保持原依赖锁定、迁移顺序与公开载荷，不在抽取时顺带升级。当前身份仍同进程，独立身份服务、产品隔离、共享 Web 与粤语适配需继续实施。

Web 真源补充：通用页面/播放器/管理员界面与完整 Web/SSR/浏览器回归在 packages/web；apps/web仅独立合成兼容测试壳。产品提供 Product品牌配置并直接引用固定框架 appDirectory，不创建页面包装副本或在产品重建业务逻辑。路由类型与构建产物忽略，不提交到框架或产品。法国v1、现有Cookie/草稿仍兼容待后续迁移，不能把前端抽取宣称完整多产品隔离。

运维/TTS/离线对齐与回归真源在scripts；产品仅保留兼容转发入口。当前保留法语样本和legacy序列化标识，不声称粤语能力。对齐私有模型/输出根是调用工作区（可CHEF_WORKSPACE_ROOT显式指定），固定模型/runtime JSON仍来自源码旁。测试Docker演练必须隔离随机资源，不读取生产配置。

独立账号迁移补充：chef-identity与产品会话范围校验已经落地，配置/限制见docs/identity-service.md。学习服务消费者、产品settings/schema/权限分离仍未完成；禁止把新二进制或账号DTO去settings当作完整迁移，不在未验证产品数据隔离前更新生产路由。账号实现继续只属于Chef，产品仅配置和部署装配。

产品学习设置补充：迁移28已移除users.settings，Brioche legacy组合API使用product_user_settings独立版本；账号Backend/独立identity不读取学习设置。新消费者必须区分account version与product preference version，不把全局role当产品管理员；迁移必须匹配新API，旧prod二进制不可先drop列。Hargow设置核心已有隔离回归，其余学习/目录/媒体/后台产品隔离与identity消费者未完成。

内省消费者补充：learning_identity按每个私有请求验证，严格Cookie/产品/ID/载荷与有界HTTP，无缓存/失败回退。写请求在独立identity核验原method+Origin+CSRF；远端模式仍Brioche learner-only，globalrole不作为其他产品管理员授权。原本地AuthSession仅legacy及未迁移后台。不得切生产或给Hargow开放未隔离的业务路由，余下角色/后台/账号编辑/schema隔离仍待完成。

产品成员授权补充：迁移29将既有全局role仅复制为Brioche product_memberships，Hargow不继承；独立identity内省必填membership，消费者profile取产品role。成员修改在account-admin锁内复核产品actor+CAS/lastoperator/audit；legacy后台已改为读取Brioche成员权限，仍依赖本地AuthSession，不代表独立后台服务迁移完成或可切生产。旧无scope邀请只在B入口建立B成员，不bootstrapHargow管理员，明确初始化流程仍待实施。

账号编辑契约补充：AccountProfile/AccountProfileUpdateRequest/AccountAuthResult真源在Rust契约，TS由export生成。独立GET/PATCH /account只改名称，expectedAccountVersion不能取UserProfile产品version。共享Web的昵称与学习日常分成独立编辑入口；账号读取/保存只采用AccountProfile版本，学习保存不接受displayName，账号role/version不替换产品role/version。登录不读取返回载荷中的学习资料，整页跳转后由根SSR重新GET /me；无需额外客户端合并账号与产品版本。不允许在learning新增账号数据库写入，或用隐式From转换混淆版本。编辑/版本恢复/跨账号取消的回归属于Chef，不向产品复制。

SSR会话边界：Product.sessionNamespace是可信构建配置（brioche/hargow），缺省仅兼容现有Brioche。SSR公共身份及私有读取都用相同精确Cookie过滤，拒绝重复或非法会话，不能依据浏览器参数、Host或header选择产品namespace。Hargow SSR测试使用隔离合成API，不代表其学习事实/目录/后台已产品隔离或生产已开放。登录返回账号全局operator/version也不能覆盖根SSR的产品learner/version。产品入口仍须匹配固定的产品学习服务/身份服务。

后台授权事务：legacy Brioche HTTP入口每次读取product_memberships；导入/暂存/激活/撤回在事务中先取得account-admin锁并复核真实actor，再取得内容锁。Operator是内部可信证明，不允许反序列化客户端产品或actor。未完成内容租户迁移前明确拒绝Hargow证明操作Brioche内容。录音/媒体/音色事务及参考录音授权也复核产品成员权限；旧角色接口更新成员版本与审计，不修改全局users.role。可信本机CLI保留独立入口，不得用它绕过HTTP权限。账号后台所有权、带产品范围的邀请、远端管理员消费者和最小数据库角色仍待实施。

账号后台归属补充：account_admin由独立identity装配，legacy组合路由复用同一实现；产品不复制账号管理。迁移30将旧identity_tokens/account_admin_audit仅归Brioche，新增产品字段与索引；令牌发行、接受、重置、列表、撤销及审计按可信配置范围执行，不能接受客户端product选择。密码仍全局共享，重置成功撤销全账号会话/重置令牌；普通会话管理只能读取/撤销当前产品（旧无范围记录仅Brioche）。Hargow邀请创建Hargow成员权限，不能赋予其他产品或全局operator。身份清理任务属于identity_cleanup，独立身份进程和legacy启用，远端学习进程不启动。Hargow首位管理员初始化、数据库schema/最小角色、独立内容后台消费者与生产装配仍未完成；不更新产品pin/生产迁移。

课程本地编号迁移补充：layout第17步 learning_000016_local_lesson_keys仅替换课程主表PK和已核对同产品替代的旧父引用；保持chef_lesson_product候选键、旧行/审计及事务账本，不CASCADE未知依赖。隔离PG同编号双产品/同产品重复/目录父引用/原指纹/未知外键整批回滚属于Chef。子表旧全局主键、author_import临时全局冲突保护、本机CLI仍待迁移，Hargow serve继续关闭，不更新生产/pin。产品库不复制SQL或通用测试。

课程关联本地编号补充：layout第18步learning_000017_local_lesson_records为撤回/导入审计/内容审核/音频审核/直接发布五表PK加入产品；保护课程快照函数的撤回查找也限定NEW.product_id，原不可变/同产品FK保留。双产品同编号五类结构记录、逐表重复拒绝、撤回互不影响/本产品不能恢复、原指纹及末步冲突整批回滚在隔离PG验证；不是人工审听或真实粤语课程证明。author_import可信产品模式只有从pg_catalog确认课程及导入审计的精确产品主键后才跳过全局编号保护；错误失败关闭，旧布局保留保护，固定素材校验/授权/不可变精确重试保持。实际B接口在H已有同编号下的自有导入验证属于Chef，不冒充H入口已开放；CLI、目录局部键与H入口继续待迁移，生产/pin保持。

目录本地编号补充：layout第19步learning_000018_local_release_keys在核对每条旧父引用的同产品替代后替换目录PK/条目PK/位置唯一键，不CASCADE；protect_release_insert限定NEW.product_id。预检/暂存从真实键元数据确认三类键完成后才解除全局ID保护；旧/部分布局保留拒绝，查询错误失败关闭。catalog聚合必须按实际复合主键分组，不能继续依赖全局id的函数依赖。结构图/未知引用和末尾碰撞整批回滚、同名目录实际B暂存/激活与H图不变的受限PG回归在Chef；H serve仍关闭，角色/媒体/CLI/语言中立待迁移，生产/pin不改。

视觉本地编号补充：layout第20步learning_000019_local_visual_keys替换图片/角色PK前核对所有旧父引用有同产品等价替代，未知依赖失败且不CASCADE，保留候选键、头像/声音/试听同产品FK及不可变审计。media导入从两表真实复合PK确认完成后才按产品检查重复ID和expectedRevision；旧/部分布局保留全局保护，错误失败关闭。图片与角色同名不允许跨产品头像借用；实际素材哈希/解码、授权和事务保持。独立身份fixture的unsupported legacy down会因已移除audition_character停止，账号审计保护仍用原migration15在独立rollback事务验证，不把该失败宣称可逆迁移。声音/录音/CLI/语言中立/H入口未齐，不切生产或产品pin。

声音档案本地编号补充：layout第21步learning_000020_local_voice_keys核对所有旧三列父引用有同产品等价替代后替换声音档案主键，未知依赖失败、不CASCADE，候选键与同产品角色/参考录音/授权/试听关联保持。受限B接口可在H同角色/声音版本存在时写入和读取自身声音，重复expectedVersion仍409、H行不变；整批rollback与旧非空指纹属于Chef回归。声音/角色逻辑不复制到产品；录音局部键、CLI、语言中立/H入口及双产品完整验收待齐，不改生产或pin。

录音本地编号补充：layout第22步learning_000021_local_recording_keys核对旧父引用有同产品等价替代后替换录音主键，未知依赖失败、不CASCADE，候选键与参考授权/角色参考/试听关联保留。录音登记只有从pg_catalog确认实际产品复合主键后才解除临时全局冲突保护；旧/部分布局仍拒绝，元数据失败关闭，来源权限/实际哈希解码/精确复用和事务审计保持。上传/配音打包共用Chef内核，不复制到产品；CLI/语言中立/实际H运行与完整双产品验收待齐，不改生产/pin。

配音工作本地编号补充：layout第23步learning_000022_local_voice_work_keys迁移参考授权/撤销、克隆任务/事件、试听/事件/评价七表产品主键和clone任务产品+grant唯一键；迁移前补同产品试听→克隆任务直接FK，再验证旧父引用替代，不CASCADE未知依赖，账本整批原子。下载token_hash与供应商prefix保留全局唯一，读取审计的自增ID不改。试听只有实际产品主键确认后解除临时外编号保护；旧/部分布局拒绝，元数据错误关闭，产品精确重试不重发供应商请求。产品不复制该逻辑，语言中立/CLI/完整H运行和双产品验收待齐，不改生产/pin。

课程配音工作本地编号补充：layout第24步learning_000023_local_speech_work_keys迁移计划/片段/片段事件及评价/对齐及评价/打包导入七表主键和打包product+lesson+revision唯一键；逐表验证旧父引用替代，不CASCADE未知依赖，账本原子。接口四类临时外编号保护尚未解除，不把数据层局部键当实际新提交/完整H入口已开放。后续须按真实键能力启用并验证精确重试、输出快照与异产品行不变；生产/pin/CLI/语言中立及H运行保持待齐，产品不复制SQL或回归。

计划/片段接口局部编号补充：product_keys只接受内部闭合表枚举，在授权事务内核对真实产品主键列。计划/片段在确认完成后才移除外编号临时保护，旧/部分布局拒绝、错误失败关闭；读/精确重试保持本产品快照和原请求条件，片段缓存复用不借其他产品来源。对齐/打包接口、本机CLI和实际H运行仍待迁移，不更新产品pin或生产。该内核和回归只在Chef。

对齐/打包接口局部编号补充：product_keys闭合枚举增加对齐和打包，授权事务内对齐核对实际产品主键，打包同时核对产品主键和product+lesson+revision唯一键；旧/部分布局保持外编号拒绝，元数据错误关闭。实际受限PG以当前本产品真实归档生成合成报告、显式合成时序决定和原子打包导入，双产品同编号读写/精确重试及改参冲突、H报告/评价/包/课源全指纹不变、供应商三计数不增通过。合成heard/timingsChecked只是测试协议，不证明人工审听或粤语质量。实现与测试只Chef，CLI/真实H/语言中立及生产验收继续，不更新产品pin或生产。

作者CLI布局补充：command对import/release-stage/release-activate/content-withdraw/release-status用固定CHEF_PRODUCT和author_scope，旧B组合布局None兼容；已登记split必须read-only verify_complete核对精确32边界、当前/登记schema和全部已知步骤定义/范围/数量，未完成/未知/漂移拒绝，不执行DDL或回退。产品author封装复用原导入与发布事务，保持重复导入拒绝、原校验与代数/撤回规则；其他未适配数据库CLI在split先require_combined（邀请拒绝先于输出文件创建），serve/显式维护保留各自门槛。author-grants复用content-grants，仅另加三张布局账本SELECT，专用非所有者维护登录无身份表/账本写权限；不是Web管理员证明。实际非所有者B同编号导入/暂存/激活/撤回与H完整哨兵指纹不变、外ID拒绝、缺失/未知/漂移零新增课源通过。H写CLI/serve仍关闭，余下配音/账号CLI、语言中立/真实H及完整上线继续，生产/pin不变；实现模板/测试仅Chef。

素材/录音CLI布局补充：assets-import/audio-import进入同一author_scope完整账本检查，media/recording新增薄author封装传固定产品到原import_bundle_impl；图片/角色/录音的校验、物理hash存储、不可变登记和事务不复制或降级。实际非所有者CLI在H同头像/角色/录音编号存在下登记B自身描述和来源，重复导入拒绝、H独有头像拒绝且本批图片/角色全rollback、重复批次零成员/审计、错误hash拒绝、H五表完整指纹不变；缺失/未知/漂移账本也拒绝两个导入并零新增。只用合成SVG/MP3，不调用供应商、不证明粤语或真人音色；H写CLI/serve与配音工作/账号CLI仍待迁移，生产和产品pin不动，通用实现与回归仅Chef。
