# Chef

Shared learning framework and identity service for Brioche and Hargow

通用学习框架，负责身份服务、学习 API、课程解释器、复习、后台、播放器、共享契约与运维工具。

已抽取 Rust 公共契约及学习/账号/管理/配音后台实现、数据库迁移和回归测试，来源见 SOURCE.json、ENGINE-SOURCE.json。框架引擎包为 `chef-engine`，通用入口为 `chef-server`；产品可以用薄启动入口调用 `chef_engine::command::run()`。暂保留契约/迁移原内部包名与法语 v1 字段，以兼容现有不可变课程；粤语语言中立适配尚未完成。

```sh
cargo test --workspace --locked
cargo run --locked -p chef-engine --bin chef-server -- check docs/examples/a1-bakery.lesson.json
cargo run --locked -p brioche-course-contract --example export
cargo run --locked -p brioche-course-contract --example export -- --output /your/product/packages/contracts/src/generated
```

首次检出执行 `git submodule update --init --recursive`，课程兼容测试使用独立 `brioche-courses` 的固定提交；不把正式课程复制进框架。少量已提交的法语例子和图片是兼容 fixture。产品库只拥有品牌、配置、入口与部署装配，不维护引擎业务副本。

后端抽取不等于多产品迁移完成：生产账号逻辑仍和学习 API 同一运行进程；已增加独立账号进程和产品会话边界，学习 API 接入、产品数据库隔离和语言中立适配尚未完成。通用 Web 已迁入 Chef，共享实现继续执行独立测试。必须继续完成这些工作及产品兼容回归，才能声称完整多语言框架复用。

拆分设计和验收范围见 [架构说明](docs/architecture.md)。秘密、生产账号、私有声音档案与恢复密钥不得提交。

## 共享 Web 真源

`packages/web/app` 现在拥有 React Router 页面、学习/复习界面、播放器、弹层、账号和管理员界面；`packages/web/tests`、`ssr-tests`、`browser-tests`、`browser-ssr-tests` 拥有通用回归。`apps/web` 是合成协议数据使用的 **Chef 独立兼容测试壳**，没有正式课程/生产域名或账号，不是复制的 Brioche 产品。`pnpm install --frozen-lockfile` 后执行 `pnpm typecheck`、`pnpm build`、`pnpm test:web`、`pnpm test:ssr`；浏览器测试需已安装 agent-browser 的 Chromium。

产品通过 `Product` 配置提供名称、文字标志、首页文案、图标、语言和颜色/字体变量，Vite 将 `@chef/product` 明确绑定到自己的 `product.ts`。React Router 直接以固定框架源码为 appDirectory，TS rootDirs 对齐产品内生成的路由类型；生成类型/构建缓存不提交。产品只维护配置、品牌静态资源和构建/部署入口，不包装或复制每个业务页面。部署产物仍是正常 React Router SSR，不绕过 `.server` 代码分离检查。

这次迁移继续兼容法语 v1 字段、旧账号 Cookie 和草稿格式；它们尚未完成产品化/语言中立改造。独立身份服务、产品数据库与客户端草稿隔离、粤语适配仍须继续实施，不能只改品牌就将 Hargow 接到 Brioche 的现行 API。

独立账号服务的配置、真实隔离验证和剩余迁移边界见 [账号服务说明](docs/identity-service.md)。产品仓库只提供装配，不复制服务端逻辑。
