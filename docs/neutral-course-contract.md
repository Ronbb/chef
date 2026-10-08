# 课程 2.0 公开契约

`neutral::NeutralLesson` 是 Chef 的完整语言中立公开课程模型，schemaVersion 为 2.0，明确记录 targetLanguage 与 explanationLanguage。目标语言支持 fr-FR/yue-Hant-HK，解释语言支持 zh-CN/zh-Hant-HK；等级和单元 ID 沿用通用作者编号规则，不自动赋予粤语 CEFR 等级。

标题使用 target/zh；对话和短文的段落使用 NeutralSegment.reading，词汇 lemma 和语法例句 target 也使用 ReadingText。原句、作者词段与注音保持独立。填空模板使用 templateTarget。十类教学块、三种练习、角色快照、素材、音频、步骤、完成策略和回顾引用全部保留，未知字段在各层拒绝。公共模型不接纳 serverOnly 或 editorial；私有判分/作者数据及实际导入仍须后续装配。

## 共用验证

结构校验复用原有课程内核：ID/版本、角色和锚点、说明目标、步骤可达性、练习归属、完成引用、知识点和媒体描述均保持原规则。一个私有的临时结构视图只映射字段位置，不返回、不保存、不将粤语称为法语课程。目标语言以真实 locale 传入角色校验；旧 1.0 入口始终固定法语策略，外部调用不能用该内部入口绕过旧契约。

媒体校验复用同一音频内核，仍校验真实描述、哈希和 URL、时长、整句/语段父区间、引用和时间顺序。旧 1.0 保留原词界规则；2.0 对全部正文先校验作者词段，再要求每条词音 cue 精确对应该词段，允许相邻汉字与作者定义的语块，不按空格推断粤语词界。没有推造或平均分配时间轴。

Rust 生成 NeutralLesson/NeutralBlock/NeutralExercise 及其嵌套类型的 TS 和 neutral-lesson.schema.json；导出名独立，不覆盖现有 PublicLesson/Block/Exercise 文件。产品库不增加模型或解释器副本。Schema 只表达结构约束，引用、语言一致性、词段和音频语义必须执行 Rust validate。

## 验证与剩余装配

合成测试分别装配法语与粤语公开模型，验证没有 fr/templateFr 字段、实际目标 locale、嵌套未知字段拒绝、引用/完成规则、测量时序的父子区间和作者词音范围。粤语测试内容只用于协议，不代表正式课程或发音质量。旧契约回归仍检查法语角色策略不可绕过。

本轮完成公开模型和验证内核，不表示生产已经支持 2.0。旧 1.0→2.0 适配、私有作者/判分结构、导入与发布、版本化 API、Web 渲染/点击/注音、配音编译与实际粤语课程仍需贯通；Hargow 服务运行门槛、产品 pin 与生产 48 课保持。
