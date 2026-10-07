# 身份与学习schema分离

本阶段验证：常规Rust工作区、16项隔离PostgreSQL回归、全目标Clippy（-D warnings）、fmt与diff通过。最后补充精确迁移版本校验和实际CLI/旧启动拒绝后，3项相关真实PG/HTTP与Clippy/fmt/diff再次通过。共享Web和生产入口没有变更。

这项维护操作属于Chef，不属于产品库。当前实现支持在同一PostgreSQL数据库中，将现有身份表移动到独立schema；学习、课程和媒体表留在原schema。它已在专用临时数据库验证，尚未部署生产。录音、媒体、音色等管理员仍依赖本地身份表，后续schema感知迁移及完整产品数据隔离还需完成，当前生产不得执行此操作。

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
