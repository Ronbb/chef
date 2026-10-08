# 双产品运行检查

通用检查工具为 `scripts/health-check.mjs`，产品仓库通过固定 Chef 引用使用它。对正式独立部署显式选择 `--layout product`，并填入共享 PostgreSQL 所属的 Compose 项目：

```text
node scripts/health-check.mjs --project chef-hargow --layout product --database-project brioche --origin https://<hargow-host> --disk-path <backup-disk-path> --minimum-free-gib 5
node scripts/health-check.mjs --project chef-brioche --layout product --database-project brioche --origin https://<brioche-host> --disk-path <backup-disk-path> --minimum-free-gib 5
```

域名、路径和数据库项目必须换成本机实际配置。磁盘检查只检查指定路径所在文件系统的可用空间，不能证明备份在异盘；生产 Docker 数据可能位于不同文件系统，需分别检查。

产品项目必须恰好有 identity、learning、web、router 四个常规服务，均为运行且 healthy，无 OOM。共享数据库检查只在明确指定的项目中筛选 postgres，不把旧组合项目里已停用的应用服务混入检查。缺失、重复、不健康、错误项目均失败。容器重启次数单独报告，不能把零重启理解为长期稳定性验收。

HTTP 检查 `/api/health`、`/api/ready`、`/health` 和首页；验证状态、响应类型与健康内容，不跟随重定向、不输出响应内容、凭据或容器环境。超时和响应大小受限。成功退出 0，运行检查失败退出 1，参数错误退出 2。JSON 仅包含选定运行状态与时延，适合保留运维证据。

默认 `--layout combined` 继续检查旧 postgres/migrate/server/web/traefik 布局，供隔离恢复或历史部署使用。不能用它检查已经切换的正式双产品布局。`--database-project` 仅用于 product 模式；该参数省略时不会验证外置数据库，报告中的 database 为 null。

2026-10-09 两产品实际生产检查通过：各四个服务和共享 PostgreSQL healthy、重启次数0、四类 HTTP 入口通过，指定本机文件系统超过5 GiB可用。工具回归同时覆盖身份服务异常、学习服务OOM、服务缺失/重复、跨项目数据以及共享数据库不健康/缺失。此证据是当次健康巡检，不替代真实登录、持续负载、备份恢复、异盘存放或 iPhone 验收。
