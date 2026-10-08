# 身份与学习schema分离

课程配音计划/片段接口本地编号：固定产品模式在授权事务内查询实际产品主键列，只有确认 `(product_id,id)` 后才解除原外编号冲突拒绝，旧/部分布局仍保留拒绝，元数据错误失败关闭。表选择为Chef内部闭合枚举，不接受客户端表名、产品、Host或header。计划和片段原固定输入、精确重试、父引用范围及缓存复用保持；对齐/打包的接口保护继续待迁移，不能宣称所有配音接口或实际H入口已开放。所有实现/回归在Chef，产品pin、生产和CLI保持。实际受限 PostgreSQL 回归覆盖旧主键保护、双产品同编号保存/读取、精确重试、改参冲突和完整异产品父记录/事件/评价指纹保持；同产品缓存复用不增加模拟供应商 create/query/synthesis 调用。独立身份两项、完整 schema_split、原 legacy 配音计划/导出/对齐/打包回归、workspace check --tests、全目标 Clippy（-D warnings）、fmt 和 diff 检查已通过，未调用真实付费供应商。

课程配音工作本地编号：`learning_000023_local_speech_work_keys` 将计划、片段、片段事件/评价、对齐报告/评价和打包导入七表主键加入产品，并将打包的课程版本唯一键改为产品+课程号+版本。逐表删除旧父引用前核对其拥有列顺序、动作、匹配/延期语义相同的已验证产品替代，未知扩展依赖失败、不CASCADE，既有候选键、固定父图和不可变记录保留；布局账本24步。隔离PG独立身份2项和完整schema_split通过：同课程/计划/片段/对齐/打包编号及事件版本的双产品七表图可共存，逐表同产品重复拒绝，同产品同课程版本的第二打包拒绝；整个验证事务rollback并对照原七张非空表指纹。实际CLI未知片段父引用和最后打包唯一键同名碰撞使全部24步DDL/账本回滚；正常、幂等、定义漂移保护及原B受限后台完整模拟发布链保持。原legacy配音计划/归档/对齐/打包、工作区check --tests、全目标Clippy（-D warnings）和fmt/diff通过，没有真实付费供应商调用。当前接口仍保留临时外编号冲突拒绝，不能将数据层支持局部编号宣称四类接口已允许同名新提交；接口启用与真实双产品运行继续待验证。本机CLI/语言中立/Hargow入口和最终生产迁移亦未完成，产品pin/生产保持，SQL和通用测试只在Chef。


配音工作本地编号：`learning_000022_local_voice_work_keys` 将参考授权/撤销、克隆任务/事件、试听/事件/评价七表主键加上产品，并把每个参考授权只能创建一个克隆任务的唯一键改为产品+授权编号。迁移先补上同产品试听→克隆任务直接外键，再逐表核对旧父引用具有列、动作、匹配和延期语义相同的已验证产品替代；未知依赖失败，不CASCADE，原产品候选键及不可变记录保留。下载token_hash和供应商prefix继续全局唯一，它们属于全局下载能力/供应商命名空间；读审计自增ID保持全局，不是产品逻辑编号。账本23步。试听提交只在真实产品主键元数据确认后解除外编号冲突保护，旧/部分布局保留拒绝，查询失败关闭；本产品固定输入精确重试与防重复供应商调用规则保持。隔离PG结构图用例在同一事务中建立两产品相同授权/克隆/试听编号及同版本事件、撤销/评价，逐表同产品重复拒绝、同授权第二任务仍拒绝，最终rollback且旧非空指纹保持。CLI未知任务父引用和末尾评价主键命名冲突使整批DDL/账本回滚。初轮新测试重复了基础H样本已有撤销记录，修正样本构造后独立身份2项和完整split回归通过，没有放宽数据库规则。旧主键恢复时同名试听仍404；产品主键恢复后受限B提交相同H编号的自有系统试听并精确重试200、改输入409、1试听/2事件、仅1次模拟合成调用。补充的真实接口回归确认B新试听accepted为空，不继承H同编号已拒绝评价；B可登记自己的模拟拒绝评价200、重复409，H任务+事件+评价完整联合哈希保持不变，最终独立身份2项及完整split再次通过。该测试不代表真实人工试听或粤语质量。原legacy配音计划/归档/对齐/打包回归、工作区check --tests、全目标Clippy（-D warnings）与fmt/diff通过；全部供应商交互为隔离模拟，没有真实付费调用。共享实现只属于Chef，生产/pin和H入口不变。


录音本地编号：`learning_000021_local_recording_keys` 校验旧素材父引用的同产品替代后，将录音主键改为 `(product_id,asset_id,revision)`，保留同产品角色参考、参考授权和试听外键以及不可变记录，未知依赖拒绝、不CASCADE。布局账本22步。录音登记只有从实际主键列元数据确认完成后才解除全局编号冲突保护；旧或部分布局仍拒绝外产品同名冲突，元数据读取错误失败关闭。自有重复编号与固定素材身份/授权、哈希、解码、批次事务、精确复用校验保持。该登记内核共用于上传和课程打包，产品仓库不复制。隔离PG独立身份2项与完整schema_split通过：末步同名约束碰撞及未知录音父引用均整批回滚，旧非空录音两表指纹保留；恢复旧主键时上传同名录音409，恢复产品主键后受限B接口200、重复409、仅增加1条B登记审计且H完整行哈希不变。私有Range读取206/no-store且前12字节与实际上传MP3逐字节一致；首轮文件断言误以WAV识别MP3，已修正测试格式并重跑完整回归，没有改播放或解码规则。legacy配音计划/归档/对齐/打包、工作区check --tests、全目标Clippy（-D warnings）、fmt/diff均通过，没有真实付费供应商调用。当前仍不开放H入口，CLI和语言中立适配及实际双产品部署继续待完成，生产与产品pin保持。


声音档案本地编号：`learning_000020_local_voice_keys` 在核对旧三列父引用的同产品等价替代后，将声音档案主键改为 `(product_id,character_id,character_revision,revision)`，保留同产品角色、参考录音、授权和试听关联以及不可变记录；未知依赖拒绝，不使用CASCADE。布局账本21步。隔离PG验证未知声音外键和末步主键命名冲突使整批DDL/账本回滚、正常与幂等维护保留四张声音表旧字段指纹。受限B后台在H已有同角色/角色版本/声音版本时写入自身系统声音200，读取返回自身完整档案，旧expectedVoiceRevision重复提交409，H完整声音行哈希不变；原外产品角色/参考录音和授权拒绝、模拟克隆试听配音发布链保持。角色库和声音逻辑只在Chef，产品不复制。独立身份2项、完整schema_split与legacy配音计划/归档/对齐/打包回归通过；工作区check --tests、全目标Clippy（-D warnings）和fmt/diff通过，没有真实供应商调用。此项不表示真实粤语音质验证或H入口开放，录音局部键、CLI、语言中立及实际双产品装配仍待完成，生产和产品pin保持。


图片与角色本地编号：`learning_000019_local_visual_keys` 核对图片/角色每条旧二列父引用具有已验证同产品替代（父/子列、动作、匹配、延期语义）后，分别改为产品+素材编号+版本、产品+角色编号+版本主键，保留既有候选键和同产品头像/声音/试听关联，不CASCADE未知依赖。布局账本20步；未知角色引用或末尾角色主键同名冲突保持整批DDL/账本回滚。导入只有从真实两表主键确认迁移完成后，才将重复编号与角色expectedRevision检查限于本产品；旧/部分布局保留全局冲突保护，元数据错误失败关闭。不能以同名角色创建授权使用其他产品头像，授权、实际文件/哈希/解码、批次事务和不可变审计保持。隔离PG结构用例保留双产品同素材ID/角色ID/版本与各自头像父引用，重复登记和外头像引用拒绝，最后rollback并对照旧非空三表指纹。实际受限B后台在H同ID存在时登记自己的SVG和角色200、重复409、H完整行哈希不变；私有图片读取返回B的真实红色SVG且no-store。临时恢复图片旧二列主键时登记409，恢复真实产品主键后成功；角色旧外编号已可复用，原跨产品写拒绝用例改为明确引用H-only头像400且角色/声音/审计总数不增加，没有放宽授权。实际CLI未知角色外键及末尾主键命名冲突使全部20步DDL/账本回滚，正常/幂等/定义漂移与原完整模拟发布链保持。独立身份2项、最终split、legacy配音计划/归档/对齐/打包回归通过；unsupported legacy down预期更新为已移除audition_character处停止，原migration15的审计回滚保护与两条账号审计仍单独验证。不表示可逆布局迁移、真实粤语或音质/人工审听验证，workspace check --tests、最终全目标Clippy（-D warnings）与fmt/diff通过，没有真实TTS或生产操作。声音档案/录音等局部编号、CLI、语言中立与实际H入口继续待完成，产品pin/生产保持。

发布目录本地编号：`learning_000018_local_release_keys` 将目录主键、目录课程主键、位置唯一键分别改为产品+目录编号、产品+目录+课程、产品+目录+位置；保留同产品候选键和父引用，只有验证旧单列外键已有同产品替代（含动作/匹配/延期语义）才移除旧引用，不CASCADE未知依赖。暂存保护按产品查stage审计，另一产品同名目录的stage不阻止追加。布局账本19步，未知父引用或末尾位置约束同名冲突保持整批DDL/账本回滚。可信产品预检/暂存只有确认真实目录与条目主键及位置唯一键都已本地化后才使用本产品ID冲突查询，旧/部分布局保留全局冲突保护。目录聚合按产品复合主键分组，legacy按编号与manifest分组兼容原32布局，不依赖原全局主键的隐含函数依赖。隔离PG证明双产品同目录号/课程号/位置共存，本产品重复位置和目录拒绝，B stage不阻碍H尚未stage的追加、两者stage后均禁止追加、状态generation分别3/8、外产品目录不能成为本产品active父引用；最终结构事务回滚及旧非空指纹保持。实际受限B后台在H已有同名且已stage目录时预检/暂存200、重复409、原请求授权和有效课源/素材条件保持；恢复旧全局位置键时预检无效/暂存409，恢复真实键后成功。B激活仅增加B generation和两条stage/activate审计，H目录/条目/审计/状态完整哈希不变；两公共入口的同号课程分别返回自己的正文。首轮新PK导致目录聚合SQL的隐含函数依赖失效，修正分组后独立身份两项通过；发布用例曾误选已发布版本，保留发布不可重新审批规则改用已导入draft，再修正既有generation字符串契约，最终完整split模拟链通过。样本与试听/审批字段仅协议fixture，非真实粤语或人工审听证明。最终legacy配音计划/归档/对齐/打包回归、workspace check --tests、全目标Clippy（-D warnings）、fmt/diff也通过，没有真实TTS调用或生产变更。其余角色/素材/声音局部键、CLI、语言中立与真实H入口仍待完成，产品pin/生产保持。

课程关联记录本地编号：`learning_000017_local_lesson_records` 将撤回、导入审计、内容审核、音频审核、直接发布五表主键加上产品范围，保留原不可变触发器、账号引用、同产品课程外键和所有历史行。原撤回保护函数追加产品匹配，避免另一个产品撤回同名课程时阻止当前产品发布。布局账本18步；同名约束冲突在末步导致整批DDL/账本回滚，不CASCADE未知依赖。隔离结构用例为两个产品建立同编号/版本的五类记录，逐表验证同产品重复拒绝；H撤回不妨碍B发布，H自身恢复被拒绝，B自身撤回后恢复也被拒绝，导入审计修改仍拒绝，最后事务回滚并对照旧非空指纹。样本不表示真实审批/审听/素材授权。课程导入在可信产品模式读取数据库主键元数据，只有课程主表及导入审计都确认为精确 `(product_id,lesson_id,revision)` 主键后才解除旧全局编号保护；查询失败不回退成功，旧布局保留原拒绝策略。课程/审计写入和精确重试继续限定产品，保留原请求授权、固定课源/素材校验与保守的导入事务锁。旧本机CLI、发布目录局部编号及实际H运行仍需继续迁移，不更新产品pin或生产。 最终隔离PG独立身份2项、schema_split完整模拟链、legacy固定配音计划/归档/对齐/打包回归通过。实际受限B后台在H已有同名课程时检查有效、导入与精确重试200、仅1条B课程/导入审计、H原行哈希及公共正文不变、改源409；临时恢复导入审计旧二列主键时检查无效/导入404，恢复真实产品主键后再成功，证明旧布局兼容保护有效。workspace check --tests、全目标Clippy（-D warnings）、fmt/diff均通过。无真实供应商调用、生产课程发布或H入口开放。

课程主表本地编号准备：`learning_000016_local_lesson_keys` 把 lesson_revisions 主键改为 `(product_id,lesson_id,revision)`，保留既有同产品候选键及所有历史行。先对每条旧二列课程外键核对已验证的同产品替代约束（列顺序、父表、更新/删除动作、匹配模式和延期行为）；未知依赖、未验证替代或其他 schema 的依赖均拒绝，不使用 CASCADE。原维护事务保持整批 DDL/账本原子性、超时、所有者与定义漂移检查，账本增加为17步。实际隔离 PostgreSQL 证明两个产品可保留同一课程编号/版本、同产品重复仍拒绝、H目录引用自己的版本、旧二列外键全部移除，旧非空行指纹保持；新增未知外键触发明确错误并回滚全部17步。此处只完成主表键与其父引用：导入/审核/撤回等子表旧全局键、课程导入全局冲突保护与本机CLI继续待迁移，不能据此开放Hargow或切生产。共用实现和回归只在Chef，产品没有迁移副本。 本批独立身份2项、最终schema_split完整模拟流水线、legacy配音计划/归档/对齐/打包1项、workspace check --tests、全目标Clippy与fmt/diff通过。新增断言初次误要求CLI公开内部数据库错误，已保留固定脱敏CLI提示并改为所有者连接核对具体原因，最终复验通过；没有真实提供方请求或生产改动。

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

独立课程导入补充：后台导入和lesson/check使用可信Store.product，管理员证明产品必须一致。相同版本重试查询固定产品，新lesson_revisions及lesson_import_audit显式写产品并原子提交。全局ID迁移前仅查询归属的碰撞保护先于素材hydration，并在全局导入锁后重新核对，避免并发写入成为异产品重试；不读取外产品server_document。actual split外课源编号导入404、上传valid=false，原外课源私有document及审计不变；B新课归属与相同重试单审计回归通过。CLI/配音交付、媒体录音hydrate/权限、私有预览与产品局部ID仍需迁移，H后台和生产pin保持。

独立目录暂存补充：stage_operator接收可信产品并核对Operator，状态锁与checked_entries课源/撤回读取固定产品；上传前release/check传相同上下文且不锁行。目录、条目与暂存审计显式登记产品，保持不可变hash和原子提交。全局目录编号冲突规则仍保留，审批/课音频/素材下层内核继续迁移。actual split外产品课源暂存404、上传检查valid=false，无目录/条目/审计新增；原B正向暂存发布与权限回归保持。CLI、导入、私有预览及媒体待继续，H后台/生产pin保持。

独立目录激活补充：HTTP将可信Store.product传activate_operator并核对管理员证明；内核锁对应产品状态、目录存在检查固定产品，条目/撤回/课源关联均固定产品，发布标志和active_release/generation只更新该产品，发布审计显式记录产品。实际split跨产品目录激活404，双状态/课源/撤回及审计指纹不变；原B原子切换/回滚/撤回与失效管理员回归保持。审批、课音频及素材门槛保留；这些下层内核、目录暂存、固定产品CLI、导入/预览/媒体继续迁移，H后台与生产pin保持。

独立撤回写入补充：HTTP将Store.product传共用withdraw内核并核对Operator归属，锁产品状态、仅更新同产品课源、显式写撤回与发布审计产品，代数只更新该状态。actual split异产品撤回404，双状态/完整课源行/撤回及审计计数指纹不变；原B硬撤回保护继续由数据库套件验证。旧本机CLI和组合入口仍传None，后续须迁移固定产品CLI与旧选择器；导入/目录写入、媒体与私有预览仍待产品化，H后台与生产保持。

独立编辑审核写入补充：概览与审核共用Store.state_selector，远端审核锁固定产品状态；课源/撤回、最新决定与精确重试actor核对均过滤固定产品，新增editorial_reviews显式登记产品。旧组合布局不查询新产品列。actual split HTTP验证H已发布/未发布课程在B审核入口均404、审核记录零增量；原B审核/CAS/重试/撤权与权限回归保持。音频accepted内核、课程导入/暂存/激活/撤回和私有预览/媒体仍需产品迁移，H后台未开放，生产pin保持。

## 独立管理员课程读取范围

独立content_router的Store.product来自已验证Client配置，legacy组合路由传None；客户端不选择范围。概览按固定产品读取发布状态、课程和目录，撤回/编辑审核关联同产品，目录条目计数也要求同产品。历史中已具product_id的五类课程记录（编辑审核、发布审计、课源导入、课音频审核、直接发布授权）采用相同范围。录音、音色、素材等历史尚未产品化，不能据此开放H后台。

actual split在已有B后台基线后加入H目录、合成课源及发布审计，B概览与历史JSON保持一致，按H独特标题/目录搜索为空。合成H课源的私有document有意保持空对象，隔离后B概览不会解析它；不代表H课程可供教学。写入/预览/课音频门槛及媒体范围继续迁移，H入口与生产pin保持，产品无后台实现副本。

首页课源关联补充：继续学习/课程状态和复习计数/下次时间共用product_source_filter，同时限制事实与lesson_revisions归属；与学习/收藏/复习接口采用相同规则。实际split回归中的独立H课源、同账号异产品进度/事件/到期与未来复习继续覆盖B首页不串数据。数据库课源外键提供另一层约束；产品局部编号、媒体后台和真实粤语仍待完成，生产保持。

## 产品课程行锁

learning_000010_product_locks新增chef_lock_product_lesson(TEXT,TEXT,INTEGER)，固定迁移schema及pg_catalog search_path，通过SECURITY DEFINER取得匹配产品/课程/版本的FOR SHARE锁，只返回boolean，PUBLIC EXECUTE撤销。split-only learning-product-grants显式授权有限学习角色。共享lock_lesson从LearningStore.product传参数，缺少当前产品课源返回404；所有学习、收藏及复习写入采用同一入口，legacy None仍用旧课程锁。独立readiness要求新的产品课程/状态函数存在。

actual split由所有者持有B课程FOR UPDATE，受限学习事务以100ms lock_timeout调用H范围返回false、B范围触发预期锁超时，释放锁后B返回true；身份角色执行拒绝。运行角色仍共同访问共享学习schema，不能将函数参数化声明为数据库角色级产品隔离。全局课ID、legacy函数移除和媒体后台继续迁移，H认证业务与生产pin保持。

## 学习事实到课程的同产品外键

learning_000009_product_sources新增四个已验证复合外键：会话(product_id,lesson_id,revision)、复习与收藏(product_id,source_lesson_id,source_revision)、进度(product_id,lesson_id,latest_completed_revision)引用lesson_revisions。进度未完成时版本可为空，已有last_session同产品/同用户/同课约束继续保留。升级立即验证已有数据；不以NOT VALID跳过历史，不自动修复跨产品归属，任何失败回滚整批DDL和账本。

actual split核对原九事实旧字段指纹、故意同名约束冲突导致全部布局步骤回滚，以及具体外键拒绝三类跨产品课源和不存在的已完成版本。独立H合成课源与九事实在事务内正向验证后回滚。全局课编号和有限课程锁、媒体/后台及真实粤语继续迁移，H认证入口与生产保持；迁移只属于Chef。

## 学习事实读取课源的产品范围

共享学习load/start/history和收藏/复习课源关联按可信LearningStore.product限制事实与lesson_revisions。手动收藏及复习参与的新建课源查询同样固定产品；None仅旧组合Brioche布局。现有课程锁仍是全局编号锁，数据库课源复合外键与局部编号迁移尚未完成，不能因此开放完整Hargow业务。

实际受限split HTTP将已发布H合成课源及其中有效知识点用于B收藏、复习参与、开始学习：返回404，当前产品三类事实均无新增。原B正向学习/收藏/复习及重放仍由同一套数据库回归验证。合成法语数据不证明粤语能力，生产pin保持。

## 公共课程API的服务器产品上下文

independent_product_router接收可信部署选择的ProductId，不能从浏览器query、header或Host选产品。目录、当前课程及指定版本课程都按该上下文查询；指定版本同时拒绝已撤回记录。legacy router仍兼容原组合布局。Hargow不能回退到内置法语演示课程或演示判分。

实际受限split HTTP验证H自有目录及公开详情200、B/H双向外产品ID当前及指定版本404、公开详情无私有字段、伪造x-chef-product不改变B目录、product查询400和H演示判分404。测试使用合成法语夹具，不表示实际粤语课程能力。生产命令仍装配Brioche，H认证学习入口关闭；完整课源外键、后台/媒体及产品局部编号继续迁移，生产pin保持。

## 产品目录读取内核及首页推荐

catalog_matching_for_product接受服务器可信产品上下文，共用原单语句摘要/搜索投影；按产品选择状态并要求目录、条目、课程和撤回同产品。未发布目录返回空，不回退Brioche；catalog_matching legacy包装仍传None兼容旧32。首页调用该内核，推荐也选择同产品状态/课程并排除撤回。

actual split使用受限学习连接读取两个同时发布的合成法语目录夹具：空H目录不回退、H摘要独立、独特搜索只命中H、B目录JSON保持；H撤回在所有者事务中只过滤H，B保持，事务回滚。B HTTP首页目录等于原B目录且推荐不含H。该测试不是粤语教学或真实发布验证，H公共业务入口仍关闭。公共catalog/lesson API上下文、后台、产品局部课编号/媒体和legacy选择器移除待实施，生产不变。

## 独立发布状态及学习锁

learning_000008_product_release_state将content_state主键替换为product_id，保留原Brioche active_release/generation。singleton暂为legacy选择器，约束singleton=(product_id='brioche')；新增Hargow状态必须显式false，未自动创建生产H行。所有调用按产品迁移后需移除旧选择器，不将其作为最终多产品接口。

chef_lock_product_release_state(TEXT)使用迁移时确定的完整schema引用、固定pg_catalog search_path、SECURITY DEFINER，只读取/共享锁指定产品状态。PUBLIC执行权限撤销；所有者在布局维护和基础learning-grants后应用infra/database/learning-product-grants.sql。这是split-only附加授权，基础legacy32授权仍有效。远端学习开始传可信LearningStore产品，浏览器不传产品；缺状态返回空目录指针，不能回退Brioche。

实际受限split测试覆盖B/H双状态、读取H目录与旧B函数一致性、H generation修改后B整行指纹不变、B引用H目录外键拒绝、身份角色无执行权限，以及双状态下B课程开始/学习与后台回归。全局课源ID、目录读取/后台写入和媒体仍未完成范围迁移，Hargow业务关闭，生产不迁移。

## 课程内容归属准备

learning_000007_product_content为lesson_revisions、content_releases、release_entries、content_state、content_withdrawals、content_audit、lesson_import_audit、editorial_reviews、lesson_audio_reviews和lesson_direct_publications添加默认brioche且仅允许brioche/hargow的product_id，使用归属不可变trigger。课程和目录增加复合候选键，目录条目/状态/撤回及相关审核用复合外键引用同产品父记录；不改课程/目录全局编号、主键、快照、版本或singleton。

实际split维护命令比较十表全部旧字段指纹（部分审计表初始为空），故意在第二内容表trigger冲突验证本步骤前面的DDL及整批布局账本回滚；测试拒绝H目录引用B课程和修改课程归属，同产品H课程/条目正例在事务中回滚。原非空九学习事实指纹保持，后续内容后台协议回归仍执行。运行查询、独立目录状态、产品局部编号、学习事实的课源归属及媒体约束尚待实施；Hargow仍关闭，生产无迁移。

## 首页学习统计的产品范围

dashboard日历统计从固定产品的四类事件读取，步骤关联会话要求同产品；课程状态、继续学习及首次完成关联同产品进度。完成数、待复习/下次时间与推荐课已学标记同样过滤产品，目标与时区读取当前产品设置。legacy32无产品上下文保持旧查询；不增加产品仓库副本。

受限split HTTP将Hargow步骤、答题、评分、首次完成及同课进度放到同账号当前周，B日历只计自身步骤/评分，答题/完成为0、活动日为1；课程状态只B且resume首次完成为空、推荐课未完成。H卡片到期/未来都不影响B待复习/下次时间。公开目录仍全局，完整内容/媒体与Hargow业务尚未开放，不执行生产迁移。

## 复习的产品范围

learning_000006_product_reviews替换卡片去重为(product_id,user_id,knowledge_id)，增加产品到期队列、卡片及评分近期索引；原卡片ID、知识快照、版本和评分时间保留。读写产品来源是LearningStore固定配置，评分排期从同产品设置读取时区；legacy无产品上下文仅兼容旧Brioche布局。加入复习与课程完成共享review_conflict。

受限split HTTP覆盖预存Hargow卡片/评分历史时Brioche详情/评分/暂停404且数据保持，队列及历史过滤；同知识B新卡独立、加入和评分精确重放、卡片列表和队列计数、评分时间区和暂停版本。维护命令及九表旧字段指纹继续验证。dashboard、内容/媒体隔离与Hargow真实双产品行为尚待完成，不据此开放Hargow或迁移生产。

## 收藏的产品范围

learning_000005_product_saved用(product_id,user_id,knowledge_id)替换原收藏唯一键，增加产品内已收藏近期索引。详情、列表、行锁前引用、新建、取消和请求锁使用可信学习产品；legacy保留原布局SQL。source课源仍全局，尚不代表内容隔离。

实际维护命令/受限split HTTP验证预存Hargow同账号同表达收藏时Brioche详情404、B新建独立ID、精确重放、列表仅B，取消B令B版本递增但H仍saved且version1；原九表字段指纹保留。复习相关路由及完成卡片去重、dashboard、内容媒体范围未完成，Hargow仍关闭，生产不迁移。

## 学习会话与进度的产品范围

learning_000004_product_sessions将learning_one_active改为(product_id,user_id,lesson_id)部分唯一索引，lesson_progress主键改为(product_id,user_id,lesson_id)，追加产品近期会话索引；保留原会话ID、版本、时间与进度字段。固定产品的学习路由显式写产品、过滤会话及子事实，历史关联要求同产品，开始请求锁加入产品；未分离legacy仍兼容原32布局。

实际split维护命令和受限HTTP覆盖双产品同账号同课活动会话共存、Brioche首次完成为空而Hargow原时间保持、异产品会话GET/步骤/答题/提示/完成404且不改版本或完成时间、B历史不含H、成功步骤登记B。九表旧字段指纹和布局整批失败回滚仍验证。dashboard、收藏/复习查询、review_cards完成去重和目录/媒体范围仍待实施，Hargow保持关闭，未迁移生产。

## 幂等请求的产品范围

布局步骤learning_000003_product_operations将learning_operations主键改为(product_id,user_id,scope,idempotency_key)，保留原scope、请求键、hash和结果。独立学习路由以可信Client.product构造LearningStore；学习步骤、答题/提示/完成、收藏和复习的共用replay/record按该产品读写，不接受浏览器选择产品。legacy组合Brioche保留旧查询兼容，仍不能在分离布局启动；独立服务必须先应用布局升级。

实际split-schema/受限连接测试预放同一账号/同scope/同key的Hargow操作：Brioche HTTP启动忽略异产品hash/result、保存自己的结果，精确重跑相同，改请求返回409；两产品记录同时保留。九表原字段指纹仍保持。身份服务受限学习测试安装同一固定SQL步骤，生产维护命令与失败回滚由schema_split覆盖。其余事实查询、活动/收藏/复习唯一键、内容及媒体产品范围尚未完成，Hargow入口仍关闭，不能将幂等隔离当作完整租户隔离。

## 学习事实产品归属准备

布局步骤learning_000002_product_facts为learning_sessions、lesson_progress、step_progress、exercise_hints、exercise_attempts、learning_operations、review_cards、review_attempts、saved_items添加必填product_id，旧行默认brioche。原字段、ID、完成时间、幂等键和快照不改写；只接受brioche/hargow，更新不能把既有事实转移到其他产品。会话/用户/课程及复习卡关系新增带产品的复合外键，跨产品父记录不能作为学习事实的来源。

这是存储准备，旧全局唯一键与旧读取暂时保留；未完成按产品SQL/RLS、内容及媒体范围前，不能持久化Hargow业务或开放入口。不能将字段与外键验收解释为完整租户隔离，也不能提前切生产。步骤仍属于Chef所有者命令，整批失败回滚；文件SQL登记时只规范CRLF为LF，避免Windows/Linux构建产生伪定义漂移，其他定义变化仍拒绝。

隔离验证使用九张非空合成事实，逐表比较排除新增product_id后的完整行指纹；产品变更、跨产品会话/复习卡/进度引用拒绝，同产品九表写入在测试事务可行并回滚。临时函数冲突使步骤后半段失败，新增列、前两个索引和迁移账本一起回滚；恢复后正常执行及重跑通过。原Brioche学习回归继续保持。所有数据均fixture，不读取生产连接。

## 分离后的升级入口

分离布局使用Chef所有者维护命令 `chef-server migrate-layout <learning-schema>`。连接仍选择学习schema，身份schema只从已登记布局读取；要求旧迁移历史精确到32、两域相关表归当前所有者。旧combined serve/migrate的拒绝保护继续保留，产品不复制迁移实现。

升级在同一维护锁、同一事务中执行。独立的chef_layout_migrations留在学习schema，登记固定版本、目标域/schema和完整SQL定义；未知历史或定义不一致拒绝执行。首批分别建立身份节流过期索引和学习尝试的用户/时间索引。未登记的同名索引冲突会失败，不静默接管；任一步失败回滚全部本批DDL和登记，精确重跑跳过已登记步骤。SET LOCAL不改变连接的长期search_path。没有自动逆向命令，迁移账本不授运行角色权限。

实际CLI/隔离PostgreSQL回归验证第二步冲突使第一步索引及账本建表全部回滚、正常执行/重复执行、七张身份表行指纹不变、错误布局/被修改定义/三个受限运行角色拒绝。常规工作区测试、4项后台PG回归、Clippy/fmt通过；生产未执行，完整产品隔离与生产装配仍待完成。

本阶段验证：常规Rust工作区、16项隔离PostgreSQL回归、全目标Clippy（-D warnings）、fmt与diff通过。最后补充精确迁移版本校验和实际CLI/旧启动拒绝后，3项相关真实PG/HTTP与Clippy/fmt/diff再次通过。共享Web和生产入口没有变更。

这项维护操作属于Chef，不属于产品库。当前实现支持在同一PostgreSQL数据库中，将现有身份表移动到独立schema；学习、课程和媒体表留在原schema。它已在专用临时数据库验证，尚未部署生产。课程、媒体及配音后台HTTP已通过独立身份内省授权，完整产品数据隔离和生产装配仍需完成，当前生产不得执行此操作。

迁移32只创建空的chef_schema_layout，不自动移动表。执行split-identity-schema要求迁移记录的最新版本精确为m20261008_000032_schema_layout；后续版本必须先更新并验证维护流程。数据库连接必须以原schema作为current_schema，使用七张身份表的真实所有者。运行身份或学习角色不能执行迁移。

维护命令接受原学习schema和一个尚不存在的新身份schema：

```text
chef-server split-identity-schema <learning-schema> <new-identity-schema>
```

DATABASE_URL必须是私有的迁移所有者连接，DATABASE_SCHEMA指定原schema；不要在公开文件、命令输出或聊天中记录连接口令。目标schema不能已存在，不会接管既有schema；名称只允许已验证的单个ASCII标识符。正式操作前需验证备份恢复、停止身份/学习/后台及维护写入，再由所有者运行。

命令在一个事务中取得全局维护锁，校验版本、布局和所有权，创建目标schema并撤销PUBLIC权限，锁定七张表后依次执行ALTER TABLE SET SCHEMA：users、browser_sessions、identity_tokens、auth_throttle、product_memberships、product_membership_audit、account_admin_audit。最后写入原schema的不可变布局记录并提交。锁等待上限5秒、每条语句上限30秒；目标冲突或其他失败回滚整个事务。不会复制账号、重新哈希密码、重签会话、消费令牌或改写审计。[PostgreSQL ALTER TABLE文档](https://www.postgresql.org/docs/current/sql-altertable.html)说明关联索引、约束和列所属序列也随表移动。

迁移后以所有者对两个专用非所有者、NOINHERIT、非超级用户运行角色分别应用权限模板。两个schema参数必须明确：

```text
psql -v schema=<identity-schema> -v learning_schema=<learning-schema> -v role=<identity-login> -f infra/database/identity-grants.sql
psql -v schema=<learning-schema> -v identity_schema=<identity-schema> -v role=<learning-login> -f infra/database/learning-grants.sql
```

私有部署流程负责创建角色、设置口令和连接；公开模板不包含秘密。身份进程配置IDENTITY_DATABASE_SCHEMA为身份schema，学习进程配置DATABASE_SCHEMA为原学习schema并启用IDENTITY_INTERNAL_URL。身份账户与产品成员仍共享，其他学习事实的产品范围尚未完成，不能给Hargow开放旧业务表。

迁移记录继续留在原schema。已分离时旧combined serve和migrate明确拒绝，迁移32的down也拒绝删除布局；不通过删除布局、审计或账号规避保护。现阶段尚不提供已提交分离操作的自动逆向命令；生产恢复需完整备份与匹配的服务版本。后续迁移需要按布局明确选择身份/学习schema，不能直接复用旧组合迁移入口。

真实验证在迁移前对七张非空表记录完整行指纹，迁移后逐表完全一致；旧会话Cookie继续通过内省和资料更新，迁移前未用邀请继续开户，序列产生后续账号ID。受限身份与学习连接各自只选一个schema，不能直接访问对方表；跨schema外键在偏好更新及实际课程启动中有效，未知账号写入遭外键拒绝。目标冲突无数据变化，重复迁移/非所有者/旧服务与迁移启动拒绝，迁移32回滚拒绝且数据指纹保持。命令测试使用空私有环境文件和清空继承环境，不读取生产配置。
