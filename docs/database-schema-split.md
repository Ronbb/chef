# 身份与学习schema分离

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
