# 独立账号服务：迁移中的可运行边界

Chef 提供 `chef-identity` 独立进程和 `infra/Dockerfile.identity`。产品库不复制账号实现，也不自建登录服务。当前生产仍使用已验证的旧组合 API；此入口尚未完成学习 API 的服务间接入、产品设置迁移和数据库最小权限分离，不能直接替换生产账号路由。

## 已实现的边界

每个实例固定一个 `IDENTITY_PRODUCT`，只接受 `brioche` 或 `hargow`。两个实例可以连接同一账号数据库，共用稳定账号 ID 和密码；各入口使用 host-only Cookie，并在服务器会话数据中记录产品。客户端更换 Cookie 名称不能跨产品登录，也不能撤销另一产品的会话。旧无产品字段的会话只允许 Brioche，成功保存时补上范围。

公共端点为 `/api/v1/auth/csrf`、`login`、`logout`、`accept-invite`、`reset-password` 以及 `/api/v1/account`。前五项位于 `/api/v1/auth/` 下。账号响应只包含 id、email、displayName、role、version；不包含学习设置、密码哈希或私有 token。公共写入保留精确 Origin、CSRF、限流和 Cookie 策略。单产品退出只撤销该产品会话，密码重置撤销同一账号的所有产品会话。

内网 `GET /internal/v1/session` 要求单一 `Authorization: Bearer <服务密钥>` 和单一 `x-chef-product`，产品必须等于该实例配置。密钥为 64 位十六进制随机值，配置中只保留其 SHA256 摘要，比较使用恒定时间方法。凭据与产品验证在会话/数据库查询之前完成；无效客户端拒绝访问。有效客户端仍须提供该产品真实 Cookie，匿名或失效会话返回 401。服务每次从数据库读取当前账号状态，响应 `product` 和 `account`，使用 `private, no-store`。全局 role 不是其他产品的管理授权。

独立进程没有课程、媒体、学习或管理员路由。内省路由必须保持内网私有，不能在公网 Traefik 中开放；学习服务调用地址和产品来自服务器配置，不能接受浏览器选择的内部目标。当前尚未提供该学习服务消费者。

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
