# 独立账号服务：迁移中的可运行边界

共享Web课程预览的“录音发布”默认POST direct-publication，只提交固定lessonHash、说明和admin-web来源/无人工试听声明；可选人工试听单独POST audio-review。未知网络结果保留操作类型/载荷并拦截离开，核对按钮重发同一请求；明确校验拒绝才恢复编辑。成功授权只表示音频发布前置条件成立，目录激活仍是后台独立操作。受控浏览器验证两种路径、503精确重试/离开保留、成功释放、人工试听及409恢复，不替代实际生产发布证据；生产pin保持。

## 自动组装与直接发布输入

共享内容服务提供GET /api/v1/operator/speech-plans/{id}/export-direct和POST /api/v1/operator/speech-packages/automatic。前者导出ready片段的技术输入，不要求人工审核，但拒绝已否决片段；后者接收Rust契约AdminAutomaticSpeechPackageRequest（reportJson与package）。报告使用严格JSON解析，4MiB报告、5MiB请求、两媒体槽和原128MiB归档限制；保留固定引擎/原始预测/计划与课源/归档hash/时间轴/新revision检查。

HTTP与可信本机CLI共用db-only kernel；原请求Operator在计算前和交付前复核，最终内容快照比对后才返回private,no-store tar。不注册新课、不激活目录、不伪造heard；本机身份Backend仅用于边界授权。实际受限split-schema模拟报告验证tar的自动报告和无人工声明、learner/CSRF拒绝及二十五类等待后撤权，旧本机自动打包回归继续验证。模拟预测不证明实际模型准确度，无付费调用。共享Web入口、完整租户与粤语/生产装配继续实施。

分离后的维护升级使用Chef `migrate-layout <learning-schema>`：身份schema来自已登记布局，所有者在同一事务向两个域应用固定步骤，独立账本拒绝未知或变化的定义。首批身份节流清理/学习尝试查询索引已通过实际CLI、冲突回滚、重复执行和运行角色拒绝验证。此命令不在服务启动时自动运行；生产尚未迁移，操作边界见database-schema-split.md。

## 独立私有课程预览

固定课程revision、整批release目录、私有素材/录音与判分预览已进入共享内容服务，以AdminAuth和仅内容db的状态运行；legacy复用，独立身份服务不暴露预览路由。公开课程投影保持不含课源/私有答案，图片/录音及词汇/语法录音URL改写为operator路径；撤回课程返回410，目录预览保留撤回标记。素材文件验证与录音Range复用原实现，保持no-store。

POST判分取得account-admin事务锁后通过可信Operator复核原请求，返回有限判分结果，不写学习会话/尝试/进度。实际分离schema/受限内容角色验证未发布音频新revision、私有SVG、WAV bytes=0-11返回206、learner403/匿名401/CSRF403、撤回410/目录标记、学习session/attempt无增量、二十三类等待后撤权403，以及身份服务停止后预览/素材/录音均503。配置不新增身份或学习表权限。布局感知后续迁移、完整产品租户/粤语及生产双产品装配仍待实施，生产pin保持现有版本。

## 独立课程音频打包和授权

speech_package与lesson_audio_reviews的HTTP接口已进入共享内容服务，状态只含内容db。打包/导入在开始及交付/写入事务内用Operator复核原请求，最终锁内容并比对快照；保留两媒体槽准入、PCM拼接/实际哈希和时间轴验证、固定报告hash/新revision、录音+draft课源+package记录原子登记以及精确重试。音频审核保留heard、expectedLessonHash和版本CAS，不改变旧审核。

`POST /api/v1/operator/lessons/{id}/revisions/{revision}/direct-publication`复用原所有者直接授权kernel，输入Rust契约AdminDirectPublication（expectedLessonHash、reason、对象evidence，序列化证据最多65536字节）。它记录独立授权审计，不生成heard声明，也不单独激活课程目录；新音频仍要求真实媒体/课源校验。本机authorize_local在入口构造本地Operator，HTTP使用原请求证明，均在授权锁内复核。

内容角色仅新增speech_package_imports、lesson_audio_reviews及lesson_direct_publications INSERT。真实分离schema/最小角色验证完整模拟包导出与原子导入、固定版本/list/精确重试/冲突、未发布的新课、有证据且无heard的直接授权、后续拒绝使旧授权失效、音频审核版本及二十二类等待后撤权403；无额外包/审核/直接事件或合成。模拟时间/人工字段只测试协议，不声称生产人工听音。私有课程预览、布局感知后续迁移、完整产品租户/粤语及生产装配仍需实施。

## 独立音频导出与对齐

speech_export与speech_alignments的HTTP路由已进入共享内容服务，以AdminAuth和内容db状态运行，本机CLI只在入口取本地Operator并复用kernel。导出在开始打包与最终交付两个事务核验原请求，交付前取得内容锁并重新比较快照，保留private/no-store、归档大小限制、原始和规范化媒体哈希验证。对齐导入/审核在授权锁内核验，保留固定引擎/报告版本/归档SHA/课源音频/字词范围、不可变请求和精确重试。

内容角色仅新增对齐报告及审核INSERT。实际分离schema/受限角色回归补全模拟课音频后导出完整tar并校验每个文件hash，验证对齐导入/读取/列表/审核幂等、缺少timingsChecked拒绝，以及十八类请求等待后撤权403，没有额外报告/决定/合成调用。模型时间与heard/timingsChecked均为合成协议fixture，不声称真实对齐或人工审听。其他打包/发布、私有课程预览、完整租户与粤语仍未完成，生产维持固定版本。

## 独立课程语音片段

课程语音片段的生成、列表、读取、私有文件播放与审核已进入独立内容服务，HTTP使用AdminAuth和仅内容连接的状态。生成/审核在事务锁内通过可信Operator复核原请求；本机CLI在身份边界构造本地证明，复用同一kernel，供应商响应worker不持有身份Backend。

保留固定计划hash、expectedPreviousId、精确重试、先审计后调用和未知结果不自动重发。复用仍校验保存音频的完整哈希/解码，不产生额外合成，并保留reusedFrom与既有审核继承。内容角色新增片段/事件/审核SELECT和INSERT，无身份表或片段修改权限。真实分离schema模拟WAV验证生成、私有读取、列表、精确重试不重发、改参409、heard协议校验、审核幂等、复用与actor审计；十五类请求等待后撤权403，最终只有两个片段、三个事件和一个审核，两次合成分别来自此前角色试听与新课程片段。heard是隔离合成测试字段，不声明生产人工听音。导出/对齐/打包/发布/私有预览、完整产品租户和粤语仍需实施，生产固定版本不变。

## 独立课程配音计划

课程声音选项、编译预览、固定计划保存、读取和列表已进入共享内容服务，状态仅含内容连接。预览与保存携带可信Operator，在account-admin事务锁内核验原身份请求；CLI在入口构造本地证明并复用相同编译/保存实现。固定课源/角色/声音、撤回过滤、plan hash、真实actor和reason、同ID精确重试与冲突校验保留。编译和保存不会发起供应商调用。

内容角色仅新增course_speech_plans INSERT，无计划更新/删除或身份表权限。真实分离schema/受限角色验证声音选项、编译、保存、固定读取、列表、精确重试、hash冲突、learner/CSRF拒绝，以及十三类请求等待后撤权403；计划仍只有一条，原模拟供应商调用数不增加。课程语音片段、对齐、打包、发布、私有预览及完整租户仍待迁移，生产与产品固定版本不变。

## 独立声音试听与采纳

试听列表、固定读取、生成、私有文件播放和采纳已进入独立内容服务，运行状态只有内容数据库连接。创建/采纳传递可信Operator并在事务锁内重新核验；可信本机CLI只在入口构造本地身份证明，复用同一内容实现，供应商worker不持有身份Backend。

提交试听审计后才合成，精确重试返回原尝试、不自动重新调用；改变参数的同ID请求409。试听文件保持私有，读回会校验原始/规范化音频及哈希。采纳在同一事务中调用声音配置授权追加，保留expectedVoiceRevision和heard等既有协议校验；重复采纳拒绝，不修改旧配置。授权模板新增试听/事件/采纳SELECT和INSERT、仅试听主键列UPDATE行锁权限，原不可变trigger保留，无身份表权限。

真实分离schema和受限内容角色使用本地模拟供应商生成24kHz合成WAV，验证试听ready、精确重试不重发、修改重试409、私有播放、采纳新声音版本与重复409，以及十一类写入等待后撤权拒绝且无额外试听或合成调用。heard字段只是隔离协议fixture，不记录真实课程人工审听或审批。其余课程语音流水线/私有课程预览、完整多产品和粤语仍需实施，生产保持固定版本。

## 独立音色创建任务

音色创建任务的列表、固定任务读取、创建及状态查询已进入独立内容服务，以db-only状态与AdminAuth授权。创建/查询事务在account-admin锁内重新核验原管理员请求；参考录音授权人的权限单独通过身份接口检查，不能因请求人仍是管理员就使用已撤权的声音来源，也不在内容连接JOIN身份表。

先持久化submitted/checking与真实actor审计，再调用供应商；后台响应worker只持有内容数据库连接，按事件版本CAS写入结果，保留未知结果不自动重新创建的恢复规则。内容角色增加任务/事件INSERT与仅任务主键列UPDATE用于行锁，不授予DELETE或修改现有事件，原不可变trigger保持。

实际分离schema测试用本地模拟供应商验证提交→processing→ready、重复创建和旧版本查询409，九类写入各自等待授权锁后撤权403且不增加事件或供应商调用。另一有效管理员尝试消费已撤权授权人的参考得到404。模拟仅一次创建、一次状态查询，无真实收费调用；既有完整后台回归继续覆盖未知结果、模型匹配、参考授权与任务状态恢复。试听、其他语音流水线及课程私有预览仍需迁移，生产固定提交未更新。

## 参考录音授权与供应商临时下载

参考录音列表、授权签发及撤销已进入独立内容路由，使用数据库状态和AdminAuth，不读取身份表。写事务在account-admin锁内重新验证原管理员请求。临时bearer下载独立装配在浏览器会话gate之外：供应商无需登录，仍需固定token哈希、未过期、未撤销、读取少于32次；文件按哈希完整解码后再次检查期限，成功读取写不可变审计。

独立身份新增凭证保护的GET /internal/v1/operators/{id}，只接受正数canonical账号ID，按服务固定产品查询成员，204允许/404拒绝；没有浏览器会话、账号资料或全局角色响应。内容下载在同一数据库account-admin事务锁内调用该接口，阻止授权人撤权提交后的新下载；身份失败返回503，不回退本地权限。HTTP沿用2秒上限、不代理、不重定向、不重试和并发限制。身份服务所有响应统一private, no-store，包括未匹配路径。

内容权限增加参考授权/撤销/读取的SELECT和INSERT、读取审计序列，以及grant主键列UPDATE权限用于PostgreSQL FOR UPDATE。原不可变trigger仍拒绝真正更改授权；未授予DELETE或身份表权限。实际分离schema回归使用5秒合成WAV验证签发、重复冲突、无Cookie下载、no-store、读取审计、撤销404、七类写入等待后撤权403零新增与旧bearer撤权404、身份离线503。没有上传真实说话者音频、调用供应商或扩大产品范围；其余声音任务/试听/课程预览仍待迁移。

## 独立角色资料与声音配置

角色库的列表、固定资料版本、头像、固定声音版本以及资料/声音追加已进入独立内容路由，使用仅db状态与可信AdminAuth。角色追加把Operator传到共享媒体事务；声音追加在授权锁内重新核验身份后执行共用写实现。两类写入保留expectedRevision/expectedVoiceRevision冲突检查与不可变历史，actor来自真实身份而非请求字段。可信CLI及试听采纳仍通过本地成员复核调用同一私有实现，不能绕过权限。

内容角色新增character_revisions和character_voice_profiles的INSERT，禁止修改/删除已有资料和声音配置，不读取身份表。实际schema分离回归验证角色第二版本、旧资料与头像读取、声音保存/固定版本读取/重复冲突/真实actor审计、learner和错误CSRF拒绝。角色和声音写入在入口校验后等待授权锁，撤权再放锁均403、无新行；身份服务停止后角色列表503。只测试已存在的Brioche法语契约和合成声音方向，不调用配音供应商或宣称粤语支持。参考授权、语音任务、试听及课程私有预览仍需迁移。

## 独立图片和录音管理

图片素材与录音的列表、上传及私有文件读取已合并到独立内容路由。两者使用只含数据库连接的状态，与legacy组合进程复用同一实现；远端模式不需要账号Backend或读取身份表。上传的内部Operator证明传入共享导入事务，取得account-admin锁后重新向身份服务核验原请求，再取得内容锁。可信本机CLI保留独立导入入口，HTTP不能提交actor或产品证明。

content-grants新增media_assets、audio_assets及对应导入审计的INSERT和审计序列USAGE。素材与录音保持不可变，不授予UPDATE或DELETE；不授予身份表或学习事实权限。文件仍只通过管理员私有接口读取，登记素材或录音不会自动发布到公开课程。

真实schema分离回归以独立身份HTTP服务、受限内容角色验证SVG和实际合成MP3上传、列表、文件读取、重复409、真实actor审计、learner和错误CSRF拒绝。课程导入、图片上传、录音上传分别在入口校验后等待授权锁，撤权再放锁，均返回403且无登记记录；逐项验证遵守两项解析并发上限。身份服务停止后列表返回503，无本地权限回退。其余角色/音色流水线和课程私有预览仍待迁移；生产仍固定原提交。

## 远端课程管理消费者

验证：常规Rust工作区与16项隔离PG回归通过；补充实际独立审批后，最终5项后台/schema分离PG回归及全目标Clippy/fmt/diff通过。实际内容角色不能读取身份/成员/学习设置或修改课程正文，却能导入、审批、暂存、激活、撤回和读取内容历史。错误CSRF/普通学习者403，内容入口不装配账号管理；用pg_locks确认请求在授权锁等待后撤销成员，旧请求重新核验得到403且无课程写入。身份服务停止后请求503，无权限回退。注册素材为仓库测试SVG，临时目录核对范围后删除，无生产数据或提供方调用。

课程检查、导入、审批、目录暂存/激活、撤回、概览和内容历史路由已从身份Backend状态分离，legacy和独立入口复用同一实现。AdminAuth接受服务器内部远端证明或legacy真实AuthSession，不从请求载荷构造actor/product。远端证明保留请求范围的身份客户端、会话和原method/Origin/CSRF，不序列化、不记录日志、不作授权缓存；每次入口仍执行有界、失败关闭的身份内省。全局账号role不授予课程权限。

写事务先取得account-admin事务锁，再重新内省原请求，确认账号未替换、当前产品成员仍operator，然后取得原有内容锁并写入。身份成员修改也先取得相同数据库的account-admin锁，因此已过入口校验但仍等待写锁的旧证明不能绕过撤销。该机制要求身份、学习和内容连接指向同一实际PostgreSQL数据库；schema与登录角色可以分离，不能改为另一个数据库或集群。[PostgreSQL锁文档](https://www.postgresql.org/docs/current/view-pg-locks.html)说明advisory锁只在同一数据库内协调。

远端身份模式配置IDENTITY_INTERNAL_URL；可额外配置CONTENT_DATABASE_URL和必填CONTENT_DATABASE_SCHEMA装配核心课程后台的独立运行连接，不复用学习角色的权限。只有CONTENT_DATABASE_URL而没有远端身份时启动拒绝；不配置内容连接则不开放这些管理路由。运行连接的数据库口令仍私有。infra/database/content-grants.sql明确授予核心课程所需INSERT、只更新published/目录状态、内容历史SELECT及审计序列USAGE，撤销身份和学习事实权限。内容历史不查询身份审计；账号审计由身份服务的/operator/accounts/history提供，legacy综合历史仍兼容。

目前远端入口仍限Brioche；Hargow构造拒绝。录音、媒体、角色音色、声音任务/试听、对齐和音频包后台仍使用legacy路由，完整管理员UI/网关装配与后续schema感知迁移、产品租户数据和生产部署尚待完成。这一步没有升级生产或产品框架pin。

## 实际身份schema移动（迁移32）

迁移32新增不可变布局，显式维护命令split-identity-schema在所有者事务内移动七张身份表到全新schema，保留记录、ID、会话、序列、约束、外键及审计。两个运行角色和授权模板支持分别配置身份/学习schema；旧组合服务与迁移入口在已分离布局明确拒绝，32回滚保护身份数据。[操作和验证边界](database-schema-split.md)包含实际命令及权限参数。独立服务配置本身仍不自动搬迁表。

专用PG回归已验证实际CLI移动、七表完整指纹不变、旧Cookie有效、旧邀请消费与新ID、身份资料CAS、远端学习profile/偏好/课程启动、未知账号跨schema外键拒绝及运行角色互访拒绝。仍未执行生产分离；远端内容后台、后续schema感知迁移与完整租户事实/Hargow继续实施。

## 身份数据库运行角色与显式schema配置

独立身份进程支持IDENTITY_DATABASE_SCHEMA，学习/兼容命令支持DATABASE_SCHEMA。配置来自可信进程环境，不来自请求或产品域名；仅允许1–63字节、以小写ASCII字母开头、后续小写字母/数字/下划线的单个schema标识符，拒绝列表、引号、点号与SQL片段。未配置时保留原连接默认行为。该入口只选连接范围，不自动创建schema、搬迁表或执行迁移；完整物理身份/学习schema拆分尚待实施。

infra/database/identity-grants.sql由迁移所有者对专用非所有者、NOINHERIT、非超级用户身份登录角色执行。它将权限收敛到账号/成员SELECT、INSERT、UPDATE，会话/令牌/节流所需读写删除，账号及授权审计仅SELECT、INSERT及相关序列USAGE；不授予账号DELETE、审计UPDATE/DELETE、学习/内容访问或内容锁函数执行。角色创建/口令与数据库URL属于私有装配，不放入公开仓库；不得将已有所有者或继承广泛权限的角色当此模板的运行角色。

验证使用真实独立PostgreSQL角色和实际授权模板：身份HTTP仍能邀请开户、双产品登录/内省、账号名称CAS、令牌发行/撤销与两条准确审计、管理员账号/令牌/审计/会话读取、全局密码重置与会话撤销；Hargow无成员管理员权限仍403。直接读取/删除学习、复习、收藏、课程和媒体表、更新/删除审计及调用内容锁函数均被数据库拒绝。它与同一测试中的受限学习角色同时运行，无所有者身份Backend。schema输入单元、2项真实身份PG/HTTP、最终全目标Clippy/fmt/diff通过。未更改产品pin或生产；远端内容后台、完整租户表/schema、Hargow及最终部署仍待完成。

## 学习持久化与受限数据库角色（迁移31）

远端学习启动只建立LearningStore数据库状态，不初始化身份Backend、密码服务或会话存储；legacy组合服务复用相同学习路由。产品偏好读取只查询product_user_settings，缺省返回默认值/version1，账号有效性由已验证身份负责；非法非正ID拒绝，未知账号写入仍由外键拒绝。复习时区事务锁定产品设置行，而非账号行。独立学习就绪检查只读取学习/内容表并要求两个锁函数存在，不要求账号表权限。

迁移31增加chef_lock_lesson(text,integer)和chef_lock_release_state()，保持学习操作与课程撤回/目录切换的行锁保护。函数使用SECURITY DEFINER、固定pg_catalog search_path和迁移时确定的完整schema引用，撤销PUBLIC EXECUTE；它们只能锁定指定课程或当前发布状态，不能写入内容或执行任意SQL。部署所有者显式授予运行角色执行权限。[PostgreSQL SELECT文档](https://www.postgresql.org/docs/current/sql-select.html)说明直接FOR SHARE需要UPDATE权限，因此学习调用这些有限函数，避免取得课程修改权限。

infra/database/learning-grants.sql提供现阶段授权模板，需显式schema和专用非所有者、NOINHERIT、非超级用户角色，由私有部署流程创建角色。模板撤销身份表权限，授予所需学习读写、内容只读及两函数执行权限。它尚不代表完整租户/RLS或身份数据库权限方案，不能直接用于旧生产程序。

验证：常规Rust工作区通过；15项隔离PostgreSQL回归、全目标Clippy（-D warnings）、fmt/diff通过。最终以实际授权模板再次运行2项身份HTTP/PG回归与Clippy：真实受限学习连接无法SELECT/DELETE身份表或UPDATE课程/发布状态，实际HTTP学习开始、步骤、收藏、复习入列/提交、队列、历史、dashboard及ready成功。两个函数实际阻塞另一事务内容更新，PUBLIC无执行权限。所有测试使用无生产挂载的临时数据库。未更新生产或产品固定框架版本；schema分离、远端内容后台、Hargow学习事实与最终部署仍待完成。

Chef 提供 `chef-identity` 独立进程和 `infra/Dockerfile.identity`。产品库不复制账号实现，也不自建登录服务。学习服务的私有内省消费者、产品设置与成员授权、账号编辑及账号后台边界已逐步实现；生产仍使用已验证的旧组合 API。完整学习/内容租户隔离、远端内容后台和数据库最小权限尚未完成，不能直接替换生产账号路由。

## 已实现的边界

每个实例固定一个 `IDENTITY_PRODUCT`，只接受 `brioche` 或 `hargow`。两个实例可以连接同一账号数据库，共用稳定账号 ID 和密码；各入口使用 host-only Cookie，并在服务器会话数据中记录产品。客户端更换 Cookie 名称不能跨产品登录，也不能撤销另一产品的会话。旧无产品字段的会话只允许 Brioche，成功保存时补上范围。

公共端点为 `/api/v1/auth/csrf`、`login`、`logout`、`accept-invite`、`reset-password` 以及 `/api/v1/account`。前五项位于 `/api/v1/auth/` 下。账号响应只包含 id、email、displayName、role、version；不包含学习设置、密码哈希或私有 token。公共写入保留精确 Origin、CSRF、限流和 Cookie 策略。单产品退出只撤销该产品会话，密码重置撤销同一账号的所有产品会话。

内网 `GET /internal/v1/session` 要求单一 `Authorization: Bearer <服务密钥>` 和单一 `x-chef-product`，产品必须等于该实例配置。密钥为 64 位十六进制随机值，配置中只保留其 SHA256 摘要，比较使用恒定时间方法。凭据与产品验证在会话/数据库查询之前完成；无效客户端拒绝访问。有效客户端仍须提供该产品真实 Cookie，匿名或失效会话返回 401。服务每次从数据库读取当前账号状态，响应 `product` 和 `account`，使用 `private, no-store`。全局 role 不是其他产品的管理授权。

独立进程提供账号管理路由，不提供课程、媒体或学习管理路由。内省路由必须保持内网私有，不能在公网 Traefik 中开放；学习服务调用地址和产品来自服务器配置，不能接受浏览器选择的内部目标。学习服务消费者的实际边界见后文。

## 运行配置

先在专用数据库运行既有显式迁移，账号进程不会自动迁移或 schema-sync。私有环境文件配置以下变量，不提交秘密：

| 变量 | 含义 |
| --- | --- |
| `DATABASE_URL` | 账号数据库连接 |
| `IDENTITY_PRODUCT` | 固定产品，默认 brioche |
| `IDENTITY_INTERNAL_KEY` | 必填，64 位十六进制随机服务凭据 |
| `PUBLIC_APP_URL` | 必填，产品完整来源；生产使用 HTTPS |
| `ADDITIONAL_APP_ORIGINS` | 可选，逗号分隔的精确可信来源 |
| `IDENTITY_BIND` | 默认 `0.0.0.0:3002` |

HTTPS 来源启用 Secure/HttpOnly/SameSite=Lax Cookie；非安全 Cookie 仅用于隔离开发测试。`/health` 表示进程响应，`/ready` 验证账号所需表可查询。Docker 使用非 root 用户 chef，支持 SIGTERM 优雅退出。

```sh
cargo run --locked -p chef-engine --bin chef-identity
docker build -f infra/Dockerfile.identity -t chef-identity:local .
```

## 验证与待完成内容

2026-10-08：Rust 工作区测试及 fmt/Clippy 全目标通过；12 项真实隔离 PostgreSQL 集成测试通过，其中新增测试覆盖双产品共享账号、伪造 Cookie 跨范围拒绝、角色实时变化、单产品退出、全局密码重置、旧会话迁移和过期会话。独立 Linux Docker 实际构建与启动成功，健康/就绪/CSRF 为 200、无凭据或匿名内省为 401、学习/后台路由为 404；非 root、0 次重启。测试容器和网络核对任务标签后全部清理，没有读取生产配置或修改生产数据。

迁移28已将 users.settings 移至 product_user_settings，以产品/账号复合键及独立版本控制；身份读取不再依赖学习设置。既有 Brioche 组合 API 的设置、复习时区与学习日历已读取新表，公开 v1 载荷保持兼容。后续必须实现产品成员/管理员授权、学习 API 私有内省消费者和故障拒绝测试，再拆分 schema/数据库角色及账号后台。旧组合 API 和其权限依赖尚存，不把独立二进制视为完整身份迁移。课程/媒体/目录、学习事实和客户端草稿的产品隔离同样尚未完成。

迁移28必须与匹配的新学习API版本协调部署；旧二进制仍读取users.settings，不能在旧生产运行中先删除该列。当前产品没有更新框架pin，生产未执行迁移。回滚将Brioche设置和版本写回旧表，存在其他产品的设置时拒绝回滚，避免静默丢失数据。账号profile_version与产品设置version分别表示账号/产品并发状态，不能在新服务消费者中混用。

## 学习 API 的独立身份消费者

配置 `IDENTITY_INTERNAL_URL` 和同一私有 `IDENTITY_INTERNAL_KEY` 后，chef-server 使用内网身份验证，不再包裹本地登录层。该 URL 必须是服务器配置的完整来源，不能来自请求；配置错误或上游失败不会自动回退本地身份。远端模式只接入 Brioche 学习、复习、收藏、学习日历及兼容的 /api/v1/me 和设置路径，尚未接入账号编辑或管理员后台；当前禁止将它作为生产完整替代。其他产品业务数据隔离未迁移，Hargow 学习路由构造明确拒绝。

每个私有请求只转发当前产品唯一的会话 Cookie 与必要 Origin/CSRF，不转发其他 Cookie、浏览器 Authorization 或伪造身份头。消费者禁用环境代理、重定向和自动重试，1秒连接/2秒完整请求超时、32个同时验证、4096bytes响应上限。严格解析产品、规范账号ID、role和version；失效会话401、CSRF/来源拒绝403、上游故障/错误载荷/产品不匹配503。响应private,no-store，不转发服务密钥或上游Set-Cookie，不缓存账号身份或权限。

写请求通过内部x-chef-request-method交给身份服务核验同一会话中的CSRF和产品精确Origin，不向学习服务返回CSRF秘密。学习服务不生成/保存登录会话、验证密码或清理身份token；只读取已验证账号，并在产品设置版本上执行CAS。全局账号名称修改在新学习设置接口明确拒绝，后续接入身份账号编辑API。旧部署的组合路由和回归保持，但远端验证失败不能进入旧路由。

2026-10-08真实TCP+PostgreSQL验证覆盖远端profile、有效/错误CSRF设置写入、收藏列表、Brioche/Hargow设置独立、另一产品Cookie拒绝及身份服务关闭后503。独立HTTP边界回归覆盖错误产品/坏JSON/超大响应/503/重定向/超时/401、重复或缺失产品Cookie本地拒绝，响应不发Cookie。既有13项实际PG回归和全工作区测试通过；完整多产品成员授权、管理员原子权限复核、账号后台、数据库最小权限和正式双服务部署仍需实施。

## 产品成员与管理员授权（迁移29）

身份服务拥有product_memberships(product_id,user_id)及独立version。迁移只把现有账号role复制为Brioche授权，不创建Hargow授权；缺少记录视为learner/version0。全局账号role与产品授权分别维护，不能把前者当另一个产品的管理员。旧Brioche邀请在Brioche入口接受时建立相应Brioche成员记录；尚未带产品范围的旧邀请不能在Hargow入口创建管理员授权。Hargow首位管理员仍需要后续明确的初始化流程。

内部SessionIdentity增加必填membership，身份服务每次查询当前产品授权；学习消费者严格验证role/version，并用membership.role组装产品profile。两个服务必须使用相同协议版本，旧载荷没有membership时拒绝而非回退全局role。身份/学习ready检查成员表存在。

身份公共GET /api/v1/account/membership返回当前产品授权；PATCH /api/v1/account-admin/members/{id}修改当前产品目标，载荷为role、expectedVersion和reason，不接受浏览器指定product。会话/Origin/CSRF沿用身份层。事务先取得account-admin锁，再复核操作者当前产品operator，锁定目标、CAS版本和每产品最后管理员检查后更新并追加product_membership_audit，记录产品/真实actor/目标/旧新角色版本/理由/时间。没有授权的全局operator不能操作Hargow。同产品并发只允许一个期望版本成功；撤销后的操作者不能继续授予权限。

迁移29回滚拒绝存在其他产品授权、任何产品审计或与原全局角色不同的Brioche权限，避免删除授权历史。测试中的合成数据清理不是生产回滚流程。当前旧后台还用global role及旧账号操作，尚未迁移到上述产品授权；远端学习仍不开放后台。继续完成后台事务内权限复核、账号编辑/管理UI、产品事实/schema及数据库最小权限后才可正式切换。

实际验证：14项专用PG回归通过，新增28→29升级保留Brioche、Hargow不继承、双产品grant隔离、并发CAS、actor撤销复核、最后管理员、5条准确审计及回滚拒绝；真实身份HTTP确认global operator在Hargow membership为learner，修改Hargow授权403。常规工作区、Clippy全目标、fmt和公共生成契约无diff通过；内部HTTP协议同步更新。无生产迁移或授权变更。

## 账号资料编辑与独立版本

GET/PATCH /api/v1/account是身份服务的账号资料资源，兼容旧组合进程也提供同一路径。PATCH只接受displayName和expectedAccountVersion；不接受settings、role、email或userId。账号ID来自真实会话，姓名经过trim、非空/80字符/控制字符校验；单条SQL以账号profile_version执行CAS并递增，过期版本409，超过持久化范围拒绝。只更新账号名称，不读写产品设置、产品授权或密码字段。Brioche/Hargow共享该名称及账号版本，产品学习设置版本独立。

AccountProfile、AccountProfileUpdateRequest、AccountAuthResult已成为共享Rust契约并由ts-rs生成TS，替代服务端手写重复DTO。旧v1 UserProfile的version继续表示产品学习设置；不能从它构造expectedAccountVersion，也不能把旧登录UserProfile直接当新账号响应。移除了旧From<UserProfile>/From<AuthResult>隐式转换，以避免混用这两类版本。

真实双产品HTTP+PG验证包含名称trim/跨产品读取、两入口同账号版本并发只有一个200另一个409、设置版本仍1且身份流程不创建设置行、超长/空白/控制字符拒绝、额外settings/userId/role字段422。14项专用PG、普通工作区、Clippy和fmt通过。该后端阶段尚未迁移Web，不能据此宣称生产已完成双服务部署。

## 共享Web的独立编辑资源

个人摘要的编辑入口只修改昵称；学习目标/时区入口只修改当前产品的学习日常。打开昵称编辑前读取GET /account，提交PATCH /account时只发送displayName和expectedAccountVersion；读取失败不打开猜测版本的表单。学习保存继续PATCH /me/settings并使用UserProfile.version，ProfileChanges类型排除displayName。

两个资源分别保留版本、错误和恢复状态。账号响应只更新名称/邮箱，不将全局role或账号version放入产品profile；迟到的产品设置响应保留已确认的账号名称。账号写入失败只读回真实版本，保留编辑草稿，要求显式重试；不会自动再次发送写入。账号切换、会话丢失和Provider卸载会取消请求及迟到的CSRF引导，旧响应不能重新恢复旧身份。两个编辑入口共享离页/放弃草稿/焦点保护，但不把两次独立写入包装成一次保存。

此UI和测试仅属于Chef，产品无新增账号逻辑。旧组合账号API也支持/account，框架升级可兼容现有产品入口。管理员页面/事务的身份迁移、生产独立路由及最小数据库权限仍待实施；本阶段不更新产品固定框架提交或执行生产迁移。

验证：严格TS7检查、Web单元测试、29项SSR以及10项独立浏览器个人页回归通过。浏览器包含账号与偏好并行保存、独立版本/角色、迟到响应、旧CSRF取消、会话过期、冲突/失败读回、保留草稿/显式重试和离页/退出保护；模拟账号载荷不包含学习设置，账号版本与产品版本故意不同。运行浏览器回归时不改受测源码，以免Vite热更新重置Provider和测试状态。

## 产品SSR会话与独立登录响应

Product.sessionNamespace允许可信构建选择brioche或hargow；缺省brioche保留现有产品兼容。共享api.server在GET /me与所有getPrivate请求中只转发当前产品的精确sid或__Host-sid Cookie，忽略其他产品及跟踪Cookie；重复Cookie、两个变体同时存在、空/非法/超大值直接401，不发内部请求。产品名来自配置，不由请求Host、URL参数或客户端header决定。身份读回使用no-store。具体安全Cookie是否可用仍由身份服务的secure配置验证，SSR不假定反向代理内部HTTP就是外部访问协议。

核对源码后修正前一阶段的待办判断：Account组件本来就不使用登录返回的UserProfile或AccountProfile；成功后整页跳转，由根SSR读取产品GET /me。因此不添加多余的客户端账号/学习资料合并。真实浏览器回归以账号专属登录响应（无settings、global operator/version47）设置会话，再跳转/profile：页面使用产品learner及设置version17，后续偏好PATCH发送version17。模拟代理正确转发Set-Cookie，原真实SSR跨账号编辑测试的合成API也补上实际/account读取。

Hargow配置的Vite SSR真实模块通过本机隔离HTTP检查身份和私有读取：只转发Hargow Cookie、产品时区/版本原样读取、仅Brioche Cookie得到匿名/401、重复H会话在网络前拒绝。它验证Web边界，不表示Hargow生产API或完整多产品数据库已经可用。产品仓库只需将namespace与相应服务部署配置配对，无新增页面副本。

验证：最终严格TS7、生产Web/SSR构建、30项单元和32项SSR回归通过；2项真实构建浏览器回归覆盖账号专属登录响应及服务器授权替换账号时清除旧草稿/页面。精确Cookie过滤也覆盖无等号、名称后缀、非法/超大/重复值。所有网络服务与浏览器均为专用合成测试，退出后关闭；没有生产数据库、域名、Cookie或API路由变更，也未推进产品固定框架提交。

## Legacy后台的产品授权迁移

Brioche后台入口每次按真实会话账号读取product_memberships，不再使用全局users.role授权。兼容profile和后台账号列表同样显示Brioche成员角色。旧角色修改接口在account-admin锁内更新成员独立版本、每产品最后管理员检查与product_membership_audit；全局账号角色不随此操作改变。该兼容接口仍采用expectedRole，独立身份接口采用expectedVersion，两者不能混淆。

HTTP课程导入、暂存发布目录、激活和撤回使用内部Operator证明，并在写事务取得account-admin锁后再次查询当前权限，随后取得内容状态锁。已撤销的旧证明不能提交写入。内容表仍只属于Brioche，Hargow证明明确拒绝；可信本机CLI保留独立入口。录音、图片媒体、角色音色及参考录音授权的事务复核也改为产品成员权限。

这一步仍使用legacy本地AuthSession；远端学习模式没有开放管理员路由。独立账号后台所有权、产品范围邀请、Hargow内容/学习事实隔离、最小数据库角色及完整双服务部署仍待实施。所有实现与回归只进入Chef，产品没有新增业务副本；不更新产品框架固定提交或生产数据库。

验证：最终工作区测试、14项隔离PostgreSQL回归、全目标Clippy（-D warnings）、fmt和diff检查通过。回归包括保留全局operator但撤销产品成员后，旧Operator证明不能导入/暂存/激活/撤回课程，内容与审计无写入；后台会话、参考录音和发布并发测试也通过。专用无生产挂载PostgreSQL容器验证标签后关闭删除。

## 身份服务拥有账号后台（迁移30）

账号列表、邀请/重置令牌、待用令牌及撤销、账号会话及撤销、兼容产品角色修改的实现从内容admin移入account_admin。独立identity直接装配这些路由，legacy组合进程复用同一模块；共享Web既有 /api/v1/operator/accounts 路径保持兼容。此模块只依赖账号、会话、产品授权和通用SQL帮助函数，不要求课程、媒体或产品学习设置。产品范围来自服务器Extension配置，载荷不允许指定产品；权限撤销在写事务account-admin锁内重新核验。

迁移30为identity_tokens和account_admin_audit增加必填product_id，旧记录默认仅Brioche，并创建产品索引。升级通过增加列保留旧不可变审计，不更新审计记录。令牌发行、验证、接受、列表和撤销按产品查询；同邮箱在另一个产品发行邀请/重置不作废当前产品令牌。不同入口不能接受或撤销另一产品令牌，失败不消费原令牌、不创建账号或审计。存在任何非Brioche令牌或账号审计时回滚拒绝，不能用删除记录绕过生产回滚保护。

账号邮箱、姓名、密码和账号ID仍共享，产品成员权限独立。Hargow operator邀请只授予Hargow成员operator，账号全局role保持learner，Brioche成员不创建。旧Brioche邀请仍兼容既有账号响应。密码重置只在发行产品入口接受，但成功后改变全局密码并作废该账号所有产品会话及所有未使用重置令牌。账号列表是共享账号目录，返回的角色属于当前产品；产品角色修改和最后管理员检查同样按当前产品执行。

普通账号会话列表/撤销只操作当前产品范围；无chef.product的旧会话仅Brioche可管理。知道另一产品的会话记录哈希也不能撤销它。Brioche兼容综合发布历史的账号审计仅显示Brioche事件。独立身份进程不装配课程、媒体、学习或发布后台，其内省仍限内网。

identity_cleanup每分钟清理过期浏览器会话与节流记录，令牌过期超过7天才删除；不删除账号审计或近期过期令牌。独立身份进程启动同一任务并在服务退出时取消，远端学习模式不会启动身份清理，legacy只复用它。就绪检查验证迁移30字段存在；不自动执行迁移。

完整数据库最小权限、学习与内容租户隔离、远端内容管理员消费者、Hargow首位管理员初始化及双产品生产装配仍待实施。当前仅提交框架实现，产品没有新增业务副本，生产固定版本未推进。

账号审计另提供GET /api/v1/operator/accounts/history，按当前产品过滤，复用有界时间/事件游标分页与公开AdminHistory载荷；不读取内容历史，也不返回令牌或会话秘密。独立身份服务可以单独提供账号审计，旧Brioche综合历史保持兼容。

验证：工作区常规测试与15项隔离PostgreSQL回归通过；增加账号审计接口后，6项相关真实数据库回归再次通过，最终全目标Clippy、fmt和diff检查通过。双产品回归包含真实29→30升级/Brioche-only安全回滚、Hargow范围回滚拒绝、跨产品邀请/重置/撤销拒绝、不继承管理员、共享账户角色独立、会话范围、产品审计/分页与撤销后权限失效；身份清理任务实际删除过期会话/节流与过久令牌，同时保留未来会话和近期过期令牌。既有登录/发布/参考录音/学习回归保持通过。未改共享Web公共DTO，未执行生产迁移。


## 分离后的本机邀请与密码重置

账号维护命令由 Chef 的 `chef-identity` 提供，产品仓库不复制实现。它使用专用非所有者身份连接，不需要内省密钥，也不读取课程或学习表。配置 `DATABASE_URL`、`IDENTITY_DATABASE_SCHEMA`、`IDENTITY_PRODUCT` 和 `PUBLIC_APP_URL`；数据库角色采用 `infra/database/identity-grants.sql`。身份 schema 必须独立且包含完整七张身份表；旧组合 schema 不接受此入口，旧组合维护命令仍属于兼容阶段。

```powershell
chef-identity invite learner@example.test .local/invite-link.txt operator@example.test 'Account invitation'
chef-identity invite learner@example.test .local/operator-link.txt operator@example.test 'Product operator invitation' --operator
chef-identity reset-password learner@example.test .local/reset-link.txt operator@example.test 'Requested password reset'
```

`IDENTITY_PRODUCT` 只能由可信进程配置选择 brioche 或 hargow。操作邮箱必须是该产品当前管理员；全局账号角色不授予产品权限。命令复用账号后台事务内的权限复核、邮箱锁、发行逻辑和不可变审计。本机操作者仍须保护数据库凭据；审计邮箱是受信维护操作的归属，不是浏览器登录证明。此命令不能初始化首位管理员，也不允许邀请已经存在的共享账号，成员加入与首位管理员初始化仍待完成。

输出路径必须是尚不存在的私有文件，不覆盖已有文件。一次性链接包含敏感 fragment，只写文件，不写标准输出或日志；Unix 新文件权限为 0600，Windows 应使用受限的私有目录。邀请只建立发行产品的成员权限；Hargow 管理员邀请不创建 Brioche 成员，也不改变共享账号为全局管理员。密码重置只在发行产品入口接受，成功后变更共享密码并使所有产品的账号会话和重置令牌失效。

数据库提交与文件写入不是同一事务。如果命令报告发行未确认，或发行完成但私有文件未保存，先通过账号后台检查待用令牌并撤销不需要的令牌，再重试；不要假定失败即未发行。命令不会自动重复发行。默认无参数或显式 `serve` 继续启动身份 HTTP 服务；服务配置与维护命令配置分开。

隔离 PostgreSQL 回归使用实际非所有者授权模板、真实 CLI 子进程和身份 HTTP 路由，覆盖旧组合拒绝、无课程读取权限、全局管理员不能越权、两产品同邮箱独立发行/审计、文件不覆盖、跨产品令牌拒绝、Hargow 接受邀请不授予全局/Brioche 管理员以及共享密码重置。测试使用合成账号与私有临时输出，不访问生产。
