# Chef

Shared learning framework and identity service for Brioche and Hargow

通用学习框架，负责身份服务、学习 API、课程解释器、复习、后台、播放器、共享契约与运维工具。

此仓库是迁移起点，尚未抽取可运行引擎。现有实现仍在 Brioche；必须消除法语绑定、固定框架依赖、完成双产品数据库隔离和兼容回归后，才能声称两个产品复用 Chef。身份服务与学习 API 独立运行，共用 PostgreSQL 的隔离 schema 与数据库权限。

拆分设计和验收范围见 [架构说明](docs/architecture.md)。秘密、生产账号、私有声音档案与恢复密钥不得提交。
