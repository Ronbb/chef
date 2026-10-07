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

后端抽取不等于多产品迁移完成：账号逻辑目前仍和学习 API 同一运行进程，独立身份服务、产品数据库隔离和语言中立适配尚未完成。通用 Web 仍需迁入 Chef。必须继续完成这些工作及产品兼容回归，才能声称完整多语言框架复用。

拆分设计和验收范围见 [架构说明](docs/architecture.md)。秘密、生产账号、私有声音档案与恢复密钥不得提交。
