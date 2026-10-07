# Chef

Shared learning framework and identity service for Brioche and Hargow

通用学习框架，负责身份服务、学习 API、课程解释器、复习、后台、播放器、共享契约与运维工具。

已抽取 Rust 公共课程/学习/后台契约及其生成的 TypeScript、Schema，来源见 SOURCE.json。暂保留原包名与法语 v1 字段，以兼容现有不可变课程；粤语语言中立适配尚未完成。

```sh
cargo test --workspace --locked
cargo run --locked -p brioche-course-contract --example export
cargo run --locked -p brioche-course-contract --example export -- --output /your/product/packages/contracts/src/generated
```

这只是框架契约的首步抽取，完整引擎与身份服务尚未迁入。必须继续消除法语绑定、完成双产品数据库隔离和产品兼容回归，才能声称多语言框架复用完成。身份服务与学习 API 将独立运行，共用 PostgreSQL 的隔离 schema 与数据库权限。

拆分设计和验收范围见 [架构说明](docs/architecture.md)。秘密、生产账号、私有声音档案与恢复密钥不得提交。
