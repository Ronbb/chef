# 作者命令与拆分数据库

共享命令和权限模板由 Chef 提供，产品仓库只转发调用和指定部署配置。

已适配拆分布局的命令是 `import`、`release-stage`、`release-activate`、`content-withdraw`、`release-status`、`assets-import` 和 `audio-import`。`CHEF_PRODUCT` 使用可信进程配置，`DATABASE_SCHEMA` 指定学习 schema，`DATABASE_URL` 使用私有维护连接；请求参数、域名或课源内容不能切换产品。

旧 Brioche 组合数据库沿用原行为。已登记拆分的数据库要求精确迁移32边界和完整布局账本，未知、缺失或改变的步骤会拒绝命令，不自动补迁移，也不回退到旧查询。仅完成 schema 移动的数据库不能导入或发布。只读状态查询也遵循相同条件。

为作者 CLI 使用单独的非所有者登录，而不是 Web 运行连接。所有者先建立登录，再应用 Chef 模板：

```text
psql -v schema=<learning-schema> -v identity_schema=<identity-schema> -v role=<author-login> -f infra/database/author-grants.sql
```

模板复用内容权限，并增加布局登记、迁移历史与布局账本的只读权限；不给该登录身份表读取权限或账本修改权限。维护连接属于可信运维入口，actor/reason 用于原有审计，不代替 Web 的管理员授权证明。

```text
chef-server assets-import <bundle.json> <source-directory> <actor>
chef-server audio-import <bundle.json> <source-directory> <actor>
chef-server import <lesson.json>
chef-server release-stage <manifest.json> <actor> <reason>
chef-server release-activate <release-id> <expected-generation> <actor> <reason>
chef-server content-withdraw <lesson-id> <revision> <expected-generation> <actor> <reason>
chef-server release-status
```

导入仍拒绝重复版本，不能把失败当成成功或隐式更新；发布仍校验素材、课源状态和期望 generation；撤回仍不可逆。只影响固定产品的发布状态和记录。

尚未适配的配音工作和账号数据库命令在拆分布局中明确拒绝。Hargow 写命令和实际学习入口继续关闭，须完成语言契约、其余维护工具和双产品验收后开放；当前 French v1 合成隔离样本不表示粤语课程能力。生产迁移和产品依赖更新须在完整拆分验收后进行。

素材和录音导入共用 HTTP 导入内核的文件哈希、解码、来源和不可变登记校验；角色头像必须在同一产品登记。重复版本仍拒绝，不借用另一产品的登记或来源。批次中的角色引用或重复成员失败会回滚整批数据库记录；内容寻址文件仍沿用原先的哈希存储规则，不能以物理对象存在代替登记成功。

详细布局边界见 [database-schema-split.md](database-schema-split.md)。


## 分离后的私有配音导出

`chef-server speech-plan-export <plan-id> <operator-email> <new-private-output.tar>` 与 `speech-plan-export-direct` 可以使用完整 split 布局和作者数据库角色。旧组合模式继续兼容原本机入口；分离模式不构建身份 Backend，不查询 users 或 product_memberships，不允许邮箱参数作为管理员证明。Hargow 写入/运维命令的关闭门槛仍保留。

分离模式使用作者连接的 DATABASE_URL、DATABASE_SCHEMA 与 CHEF_PRODUCT，以及 PUBLIC_APP_URL、IDENTITY_INTERNAL_URL、IDENTITY_INTERNAL_KEY。作者角色使用 infra/database/author-grants.sql，具备内容权限与布局账本只读权限，仍无身份表读写权限。命令在读取私有会话文件前验证完整布局账本，不能顺便自动迁移。

`CHEF_OPERATOR_SESSION_FILE` 指向调用者私有目录中的 JSON，包含当前产品管理员的有效会话 Cookie 和 CSRF：

```json
{"cookie":"brioche.sid=<current-session>","csrfToken":"<current-csrf>"}
```

HTTPS 产品使用 `__Host-brioche.sid` Cookie。文件只能包含 cookie 与 csrfToken，最大 16 KiB；来源是已登录的本产品管理员会话。保护该文件，不提交到仓库；会话过期或 CSRF 轮换后重新取得当前值。内部服务密钥与浏览器会话都不能用命令参数或标准输出传递。

CLI 按可信公开来源向独立身份服务核验 POST 的会话/Origin/CSRF，确认实际产品管理员，并核对真实账号邮箱与参数一致。身份不可用、非管理员、另一产品 Cookie、错误邮箱或 CSRF 均失败关闭，无缓存、重试或兼容权限回退。内部服务密钥不能单独替代管理员会话。导出事务及交付复核继续使用同一原请求身份，并在共享数据库 account-admin 锁内复核权限，避免撤权后交付。

导出只能读取本产品计划与最新片段。已审听导出要求当前评价；direct 导出只要求就绪片段，不声明人工审听。归档通过最终来源/权限复核后才创建输出文件，采用 create_new，不覆盖已有文件；Unix0600，Windows使用受限私有目录。标准输出只有计划编号、字节长度、审听要求与 published=false。导出不是发布。文件写入中断可留下不完整私有文件，不自动覆盖或重试。

实际隔离 schema 回归通过受限作者角色和真实独立身份服务启动 CLI 子进程，比较两种 CLI 与 HTTP 归档逐字节一致及文件哈希，验证输出不覆盖、普通成员/错误邮箱/CSRF/外产品计划拒绝且无输出、身份停止时失败。测试仍使用合成供应商与素材，不调用付费语音服务。实现、授权和测试只在 Chef，产品不复制配音逻辑，生产/产品固定版本保持。


## 分离后的自动配音打包

```powershell
chef-server speech-package-automatic report.json package-request.json operator@example.test .local/automatic-package.tar
```

完整 split 布局下，此命令采用上面的作者数据库角色和私有管理员会话配置，核验实际账号与本产品成员权限，不读取身份表或根据全局角色授权。报告和打包请求使用既有严格作者 JSON/契约；产品由可信 CHEF_PRODUCT 配置确定，不能从报告、邮箱或客户端字段选择。旧组合入口保留兼容，Hargow 运维命令仍须等待完整引擎与语言适配验收。

命令复用 HTTP 的 assemble_authorized 内核，以本产品的固定计划、课源和最新片段验证报告来源归档 SHA、报告哈希及请求。另一产品共享生成键的较新片段不能被采纳。归档生成后，在同一共享数据库授权锁内复核原身份和本产品来源；权限撤销、身份不可用或来源变化时不交付。最终使用共用私有输出函数 create_new，不覆盖已有文件，凭据与私有内容不写日志。

自动打包只产生归档，不导入录音/课程、不登记审听、不授权直接发布或激活目录，也不调用语音供应商。humanListeningAsserted=false 与 approvalRequired=false 保持自动流程的真实含义，不声明人工审听。后续导入和发布采用各自的明确命令或后台操作。

隔离实际 CLI 回归使用专用非所有者作者连接和真实身份 HTTP 服务，验证归档与 HTTP 自动包逐字节一致、课源结构与文件哈希/本产品片段保持、既有输出不覆盖；外计划/外片段、错误邮箱、普通成员、CSRF 与已停止身份服务均拒绝且无输出。录音/课程/包导入/审听/直接发布六类表总数及模拟供应商三计数不增加。通用实现/输出规则和详细测试仅位于 Chef，生产和产品固定提交保持。

## 分离后的配音计划预览与保存

`speech-plan-preview <request.json> <operator-email> <new-private-output.json>` 和 `speech-plan-save <request.json> <operator-email>` 使用完整布局账本、受限作者数据库连接及上述私有管理员会话。真实身份服务核验产品管理员、邮箱与 CSRF，学习数据库不读取身份表；权限与产品来源由原 HTTP 内核再次在事务中验证。旧组合入口保持兼容，Hargow 写入门槛保持。

预览将包含完整声音参数的计划仅写入新私有文件，不覆盖。保存复用不可变计划、固定预览哈希与精确请求重试规则，审计理由加 `[local-cli]` 标记；同产品相同编号的不同请求拒绝，不读取或改变其他产品的同编号计划。标准输出只含编号、哈希、请求数量、文本字符总量与 audioGenerated=false；这两个命令不生成音频或发布课程。

实际隔离回归用作者角色和真实身份服务运行 CLI：私有预览与 HTTP 逐字段一致，输出不覆盖，错误邮箱/普通成员/CSRF/外产品课源拒绝；CLI 首次保存与 HTTP 精确重试共享记录，两个产品同编号独立，另一产品计划指纹保持，改变理由不能覆盖。独立身份服务停止后，预览和保存均拒绝。合成供应商测试不代表真实粤语或语音质量验收。

## 分离后的语音生成命令

`speech-clip-generate <request.json> <operator-email>` 与 `voice-audition-generate <request.json> <operator-email>` 使用完整布局、受限作者连接及私有管理员会话。可信产品、真实身份/CSRF/邮箱核验与创建事务权限复核保持；CLI 不查询全局账号表。请求理由使用 `[local-cli]` 标记，固定请求、成本确认、未知结果重试确认与当前角色声音版本规则均复用 HTTP 内核。旧组合命令继续兼容。

提交与完成轮询都带相同产品范围。片段缓存只复用本产品已校验的原始与规范化音频；精确请求重试不重复生成。无可用供应商时，允许读取精确已有请求或有效缓存；需要新生成则拒绝，不假造就绪状态。供应商配置错误与身份错误仍失败关闭。轮询上限沿用245秒，超时的 submitted 不表示完成，也不能据此自动创建新请求；先核对固定记录。

这些命令不发布课程、不登记人工审听。输出仅编号、状态、时长和审听要求；文件和工作记录遵循既有私有媒体/不可变事件规则。实现、授权和轮询只位于 Chef，产品只装配。

隔离验证运行真实 CLI 子进程且清空供应商环境：有效本产品缓存首次登记与 HTTP 逐字段一致、精确重试不新增事件，已就绪角色试听可以精确重放；双产品同编号的另一产品记录指纹保持，供应商计数不增加。普通成员/错误 CSRF/另一产品 Cookie/错误邮箱与身份服务停止拒绝。需要新供应商生成时不插入已提交记录。模拟供应商只验证协议，不能作为真实语音质量或粤语模型验收。
