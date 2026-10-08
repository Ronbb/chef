# 原生粤语配音与产品装配

课程 2.0 的固定配音计划使用 `speech-plan-2/author-scalar-1`。正文和知识点统一提取作者标注的 Unicode scalar 词段；请求哈希包括这些词段。1.0 的 UAX 分词、生成键与请求字段保持兼容。角色语音 locale 必须与课源角色相同，粤语使用官方多语种系统音色和香港粤语指令，不写未文档化的 language_hints 值。

管理员选项的新入口为 `/api/v2/operator/lessons/{id}/revisions/{revision}/speech-options`，由 Rust 导出中立 DTO，Web 不解析法语字段。旧选项入口保持 1.0，拒绝把 2.0 降级。

`scripts/qwen-plan-tts.mjs --plan <compiled-plan.json> --output <new-private-directory> --confirm-cost` 消费固定计划。每次付费前写入不可覆盖的 attempt，保留供应商原始 WAV、修复 RIFF 后的 WAV、参数和哈希收据；不自动重试。端点只来自私有环境配置，不能由课源指定。该本机流程不登记数据库配音任务。

固定 Qwen3 ForcedAligner 使用公开 chat template，以每个作者语块作为一个文本单元。保留实际 80ms 时间类别，禁止插值或均分字数。打包同时核对原始预测、词段映射、音频字节和解码时长；粤语不允许法语数字别名。

离线交付复用共享 PCM/课程打包内核：

```text
chef-server speech-package-local <lesson.json> <voice-plan.json> <inputs.tar> <predictions.json> <package-request.json> <new-private-output.tar>
```

输入课源和角色配置重新编译，须与归档固定计划完全一致。归档成员限普通文件、固定媒体命名及大小；报告须匹配归档哈希、固定模型与 runtime。输出保持私有且不覆盖；没有 actorId，明确 `speechTasksRegistered=false`、`humanListeningAsserted=false`。公开课程素材由正常作者素材/录音/课源导入登记，供应商原件和付费尝试不进入公开课程仓库。课程作者须补齐角色头像的明确媒体描述；结构检查不能代替登记后的发布检查。

Hargow 启动必须使用数据库、独立身份与完整可验证 split 布局，不允许旧 combined 服务或法语演示课。作者命令走同一固定产品范围。管理员内容锁的远端权限复核适用于两产品；新角色 locale 由可信产品装配选择。

所有者已授权直接发布的维护入口为 `lesson-direct-publication-owner <lesson-id> <revision> <existing-product-operator-email> <authorization.json>`。它仅接受完整 split 布局，要求当前数据库角色实际拥有身份 users/memberships 和内容 lesson/direct-publication 四表；在账号管理锁内核对本产品现任管理员，再复用原始版本哈希、真实音频、不可变授权与发布检查。证据明确记录 table-owner authority，不声称浏览器登录或人工试听。受限 author/content 运行角色不可使用；普通维护入口继续需要真实管理员会话。目录激活仍是独立动作。

`infra/compose.product.yaml` 是共享装配模板，产品只配置固定服务/Web 镜像、产品、域名、专用连接和外部网络/媒体卷。身份、学习、内容数据库登录独立，运行进程不持有所有者凭据。内部 Traefik 把账号/认证接口送往身份服务，其余 API 送往学习服务；Web SSR 使用同一内部路由。两个产品身份实例访问同一账号 schema，并有各自 Cookie、origin 与产品成员范围。没有宿主 HTTP 端口，TLS 由外部网关处理。

2026-10-08 证据：Hargow 首课实际 11 请求、42 字符、解码总时长 12.805 秒；11 段固定模型对齐无问题，共享打包产生六份公开 WAV 与一段对话的 32 个 cue。原创两个 SVG 头像、公开音频和课源通过检查。生产备份 1,496 媒体对象校验通过；离线恢复副本从迁移27升级32、分离身份并应用24步布局，两个共享账号字段与原 Brioche 发布指针完全一致。在副本登记原生粤语首课 revision3、拒绝受限作者的 owner 授权、实际所有者授权及暂存/激活成功，H目录 generation1。原生产尚未迁移或切换，不能将这些证据写成已上线或已人工试听。

验证包含69项库测试、真实 split 数据库全链路回归、原48课计划兼容、32项 SSR（含原生配音选项）、11项 TTS 传输/计划测试、20项对齐测试及 Clippy/fmt。正式切换仍需双产品完整容器演练和切换时数据库快照。

2026-10-08 后续正式切换已完成：原生产迁移32、身份分离与24布局，双产品受限运行角色/四服务与真实HTTPS入口健康；B48课与两账号字段/全部课源/发布状态保持，H首课rev3已实际发布。最终冻结数据库快照验证通过，完整备份与恢复验证先于切换。线上及隔离HTTP/媒体/移动尺寸验证见 Hargow 产品部署记录。前文‘尚未上线’仅为当时演练状态。


角色声音后台语言补充：角色声音编辑缺档案时按固定character.speechLocale初始化，系统试听的文案、台词、指令和候选locale同样按固定角色语言；复刻试听按可信产品装配初始化台词。官方多语种音色从Rust生成，法语旧导出为兼容别名。已有声音参数/参考录音/版本保留，候选生成不覆盖声音；真实组件回归验证粤语初档保存、粤语候选请求、旧档案保持，原法语收费回执恢复回归保持，未调用实际提供方或声明审听。私有原生课程预览仍使用旧模型，是后续明确缺口。

