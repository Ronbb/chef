# 身份与学习schema分离

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
