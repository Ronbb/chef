# Chef 协作约定

学习持久化边界补充：远端学习进程只初始化LearningStore，不持有身份Backend、密码服务或会话存储。设置读取只查product_user_settings；账号有效性由身份内省负责，未知账号写入仍由外键拒绝。独立学习ready不读取身份表。迁移31提供固定schema、固定search_path、撤销PUBLIC EXECUTE的内容行锁函数，学习数据库角色无内容UPDATE权限；授权模板和实际受限角色回归属于Chef。此阶段仍未分离物理schema或完成Hargow事实隔离、远端内容后台及生产装配；禁止推进生产迁移或产品pin。

通用学习框架与独立身份服务真源。产品品牌与正式课程属于各自仓库，不把 Brioche 的法语绑定作为未来语言中立契约。

当前先抽取共享契约，暂保留 `brioche-course-contract` / `@brioche/contracts` 包名与 v1 法语字段以兼容既有不可变课程。这是明确的迁移阶段，不代表粤语支持已经完成。改变公共契约须通过版本化适配与 Rust 生成的 Schema/TS，不手改生成物。

tests 的法语样本仅兼容 fixture，不是 Chef 拥有的正式课程。所有产品依赖固定已验证提交；身份服务、学习 API、数据产品隔离和完整引擎抽取尚待实施，边界见 docs/architecture.md。秘密、生产数据与私有声音资料不得提交。

## 后端抽取阶段

学习/账号/管理/配音实现与 SeaORM 迁移已迁入 `crates/server` / `crates/migration`；`chef-engine` 提供通用入口，产品只调用入口。所有数据库回归属于框架，使用隔离 PostgreSQL，不允许读取生产配置。`curriculum` 子模块固定法语课源版本，仅用于兼容测试；图片和少量旧例子是明确的测试 fixture。保持原依赖锁定、迁移顺序与公开载荷，不在抽取时顺带升级。当前身份仍同进程，独立身份服务、产品隔离、共享 Web 与粤语适配需继续实施。

Web 真源补充：通用页面/播放器/管理员界面与完整 Web/SSR/浏览器回归在 packages/web；apps/web仅独立合成兼容测试壳。产品提供 Product品牌配置并直接引用固定框架 appDirectory，不创建页面包装副本或在产品重建业务逻辑。路由类型与构建产物忽略，不提交到框架或产品。法国v1、现有Cookie/草稿仍兼容待后续迁移，不能把前端抽取宣称完整多产品隔离。

运维/TTS/离线对齐与回归真源在scripts；产品仅保留兼容转发入口。当前保留法语样本和legacy序列化标识，不声称粤语能力。对齐私有模型/输出根是调用工作区（可CHEF_WORKSPACE_ROOT显式指定），固定模型/runtime JSON仍来自源码旁。测试Docker演练必须隔离随机资源，不读取生产配置。

独立账号迁移补充：chef-identity与产品会话范围校验已经落地，配置/限制见docs/identity-service.md。学习服务消费者、产品settings/schema/权限分离仍未完成；禁止把新二进制或账号DTO去settings当作完整迁移，不在未验证产品数据隔离前更新生产路由。账号实现继续只属于Chef，产品仅配置和部署装配。

产品学习设置补充：迁移28已移除users.settings，Brioche legacy组合API使用product_user_settings独立版本；账号Backend/独立identity不读取学习设置。新消费者必须区分account version与product preference version，不把全局role当产品管理员；迁移必须匹配新API，旧prod二进制不可先drop列。Hargow设置核心已有隔离回归，其余学习/目录/媒体/后台产品隔离与identity消费者未完成。

内省消费者补充：learning_identity按每个私有请求验证，严格Cookie/产品/ID/载荷与有界HTTP，无缓存/失败回退。写请求在独立identity核验原method+Origin+CSRF；远端模式仍Brioche learner-only，globalrole不作为其他产品管理员授权。原本地AuthSession仅legacy及未迁移后台。不得切生产或给Hargow开放未隔离的业务路由，余下角色/后台/账号编辑/schema隔离仍待完成。

产品成员授权补充：迁移29将既有全局role仅复制为Brioche product_memberships，Hargow不继承；独立identity内省必填membership，消费者profile取产品role。成员修改在account-admin锁内复核产品actor+CAS/lastoperator/audit；legacy后台已改为读取Brioche成员权限，仍依赖本地AuthSession，不代表独立后台服务迁移完成或可切生产。旧无scope邀请只在B入口建立B成员，不bootstrapHargow管理员，明确初始化流程仍待实施。

账号编辑契约补充：AccountProfile/AccountProfileUpdateRequest/AccountAuthResult真源在Rust契约，TS由export生成。独立GET/PATCH /account只改名称，expectedAccountVersion不能取UserProfile产品version。共享Web的昵称与学习日常分成独立编辑入口；账号读取/保存只采用AccountProfile版本，学习保存不接受displayName，账号role/version不替换产品role/version。登录不读取返回载荷中的学习资料，整页跳转后由根SSR重新GET /me；无需额外客户端合并账号与产品版本。不允许在learning新增账号数据库写入，或用隐式From转换混淆版本。编辑/版本恢复/跨账号取消的回归属于Chef，不向产品复制。

SSR会话边界：Product.sessionNamespace是可信构建配置（brioche/hargow），缺省仅兼容现有Brioche。SSR公共身份及私有读取都用相同精确Cookie过滤，拒绝重复或非法会话，不能依据浏览器参数、Host或header选择产品namespace。Hargow SSR测试使用隔离合成API，不代表其学习事实/目录/后台已产品隔离或生产已开放。登录返回账号全局operator/version也不能覆盖根SSR的产品learner/version。产品入口仍须匹配固定的产品学习服务/身份服务。

后台授权事务：legacy Brioche HTTP入口每次读取product_memberships；导入/暂存/激活/撤回在事务中先取得account-admin锁并复核真实actor，再取得内容锁。Operator是内部可信证明，不允许反序列化客户端产品或actor。未完成内容租户迁移前明确拒绝Hargow证明操作Brioche内容。录音/媒体/音色事务及参考录音授权也复核产品成员权限；旧角色接口更新成员版本与审计，不修改全局users.role。可信本机CLI保留独立入口，不得用它绕过HTTP权限。账号后台所有权、带产品范围的邀请、远端管理员消费者和最小数据库角色仍待实施。

账号后台归属补充：account_admin由独立identity装配，legacy组合路由复用同一实现；产品不复制账号管理。迁移30将旧identity_tokens/account_admin_audit仅归Brioche，新增产品字段与索引；令牌发行、接受、重置、列表、撤销及审计按可信配置范围执行，不能接受客户端product选择。密码仍全局共享，重置成功撤销全账号会话/重置令牌；普通会话管理只能读取/撤销当前产品（旧无范围记录仅Brioche）。Hargow邀请创建Hargow成员权限，不能赋予其他产品或全局operator。身份清理任务属于identity_cleanup，独立身份进程和legacy启用，远端学习进程不启动。Hargow首位管理员初始化、数据库schema/最小角色、独立内容后台消费者与生产装配仍未完成；不更新产品pin/生产迁移。
