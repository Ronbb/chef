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
