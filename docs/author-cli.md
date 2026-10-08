# 作者命令与拆分数据库

共享命令和权限模板由 Chef 提供，产品仓库只转发调用和指定部署配置。

已适配拆分布局的命令是 `import`、`release-stage`、`release-activate`、`content-withdraw` 和 `release-status`。`CHEF_PRODUCT` 使用可信进程配置，`DATABASE_SCHEMA` 指定学习 schema，`DATABASE_URL` 使用私有维护连接；请求参数、域名或课源内容不能切换产品。

旧 Brioche 组合数据库沿用原行为。已登记拆分的数据库要求精确迁移32边界和完整布局账本，未知、缺失或改变的步骤会拒绝命令，不自动补迁移，也不回退到旧查询。仅完成 schema 移动的数据库不能导入或发布。只读状态查询也遵循相同条件。

为作者 CLI 使用单独的非所有者登录，而不是 Web 运行连接。所有者先建立登录，再应用 Chef 模板：

```text
psql -v schema=<learning-schema> -v identity_schema=<identity-schema> -v role=<author-login> -f infra/database/author-grants.sql
```

模板复用内容权限，并增加布局登记、迁移历史与布局账本的只读权限；不给该登录身份表读取权限或账本修改权限。维护连接属于可信运维入口，actor/reason 用于原有审计，不代替 Web 的管理员授权证明。

```text
chef-server import <lesson.json>
chef-server release-stage <manifest.json> <actor> <reason>
chef-server release-activate <release-id> <expected-generation> <actor> <reason>
chef-server content-withdraw <lesson-id> <revision> <expected-generation> <actor> <reason>
chef-server release-status
```

导入仍拒绝重复版本，不能把失败当成成功或隐式更新；发布仍校验素材、课源状态和期望 generation；撤回仍不可逆。只影响固定产品的发布状态和记录。

尚未适配的配音、素材和账号数据库命令在拆分布局中明确拒绝。Hargow 写命令和实际学习入口继续关闭，须完成语言契约、其余维护工具和双产品验收后开放；当前 French v1 合成隔离样本不表示粤语课程能力。生产迁移和产品依赖更新须在完整拆分验收后进行。

详细布局边界见 [database-schema-split.md](database-schema-split.md)。
