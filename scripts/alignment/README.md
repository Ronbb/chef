# 课程配音逐词对齐

这是本机私有工具，读取 `/admin/speech-clips` 下载的已审听 TAR，将固定法语原文与真实 PCM 录音交给 Qwen3-ForcedAligner。生成的是待人工核对的预测，不会调用收费 TTS、访问应用数据库、登记录音或发布课程。输入文件仍需通过正式登记和服务端权限核对；本机清单中的审听记录不能替代服务端授权。

已验证 Windows、Python 3.12.12、CPU/float32/eager attention。GTX 1070 Ti 不作为 BF16/FlashAttention 环境使用；该模型和 Python 环境不装入 Web/API 生产镜像。模型固定到官方仓库 revision，配置、分词器及 safetensors 均核对仓库记录的 SHA-256。升级模型、编译器或关键依赖需要重新验证，不能静默接受不同版本。

## 安装与准备

以下命令在项目根目录执行，需要 `uv`。首次下载公共模型需要网络，下载不读取 Hugging Face 账户凭据或 TTS Key。Windows CPU 的全部依赖版本记录在 `requirements.windows-cpu.txt`，模型清单见 `model.json`。

```powershell
uv venv --python 3.12.12 .local/alignment-native-venv
uv pip sync --python .local/alignment-native-venv/Scripts/python.exe --index https://download.pytorch.org/whl/cpu --index-strategy unsafe-best-match scripts/alignment/requirements.windows-cpu.txt
.local/alignment-native-venv/Scripts/python.exe scripts/alignment/prepare.py
```

## 校验与对齐

下载包保存到 `.local/private`。先校验清单及媒体，此命令仅用 Python 标准库；正式对齐命令加载已准备且哈希正确的本机模型，设置离线模式，不发送音频。

```powershell
python scripts/alignment/align.py .local/private/speech-<plan-id>.tar --check
.local/alignment-native-venv/Scripts/python.exe scripts/alignment/align.py .local/private/speech-<plan-id>.tar --output .local/private/alignment-<plan-id>-v1.json
python -m unittest discover -s scripts/alignment -p test_align.py -v
```

输出必须位于 `.local/private`，已有文件不会覆盖。退出码0表示结构/模型预测范围通过；2表示已保存结果，但至少一个片段或分段需要处理；1表示输入、模型或运行失败。默认人工流程输出保留 `reviewRequired: true`，退出码0不代表听感/时间轴人工审核通过。所有者直接发布输入另用下文的显式 `--direct` 流程，报告不声明人工审核。异常中断可能留下不完整输出，需要使用新文件名重新执行，不能把旧文件当成完成回执。

清单校验固定 compiler、Rust Serde 字段顺序的 planHash、完整请求 generationKey、所有目标覆盖、原文 Unicode scalar 范围、真实来源审听记录、原始/修复 WAV 配对和 SHA-256。只接受未压缩、128 MiB以内的 TAR；清单4 MiB以内，媒体每个16 MiB以内、单声道24 kHz/16-bit PCM且不超过180秒。只在内存读取精确名称的普通成员，不 extractall，不接受路径穿越、链接、重复或多余文件。

对齐输入按清单词单元组成：模型侧做 NFC 和法语撇号规范化，原始文本及 scalar 范围不改。保留模型返回的原始预测及运行版本；词数/文字不符、非有限数、零时长、重叠或越界会标记问题，异常片段不给出可用词时间。毫秒转换采用十进制向外取整，避免二进制浮点引入重叠，不修正模型本身的时间。

课程 segment 若把一个词切成不同文本范围，无法与整词预测精确映射时标记 `segmentWordBoundaryMismatch`。工具不平均拆分声学区间。需要人工修改分段或经过实际音频核对的时间轴；修改课程须追加新版本，再重新生成固定计划。原生 Transformers 推理直接读取80毫秒量化的分类结果，不调用会插值修正时间点的 `decode_forced_alignment`。缺失、多余或无效分类保留为异常，输出不能宣称音素边界或人工审听已确认。

后台时间轴审查入口为固定计划的 `/admin/speech-alignments?planId=<id>`：导入这里生成的私有 JSON，逐片段试听、修改逐词毫秒范围并明确确认。服务端重建实际导出包核对 SHA-256、固定计划及最新已审听片段；其他包或测试清单不能冒充生产来源。异常预测保持原样保存，校正结果另记不可变人工决定。正式录音包组装、登记和新课程/release 发布仍是后续步骤；核对通过不直接修改已发布课程。

来源：[Transformers 原生 Qwen3-ASR / ForcedAligner](https://huggingface.co/docs/transformers/model_doc/qwen3_asr)、[官方转换的固定模型](https://huggingface.co/Qwen/Qwen3-ForcedAligner-0.6B-hf/tree/c07281df297b9905d24a508279258cccf987a064)。模型许可证 Apache-2.0，模型文件不进入 Git。


`rust-plan.fixture.json` 是实际 Rust 编译器输出的测试向量，来自公开面包店示例与测试用 Léa 档案，不是生产声音选型或审听决定。用于防止 Python 对 JSONB 字段排序或请求哈希的处理与 Rust 漂移；更改编译器版本时重新生成并核对。CI 只运行标准库校验测试，不下载模型、不生成新音频。

运行版本来自 `runtime.json`，本机工具和服务端新导入共用该清单。迁移改用 Transformers 5.19.0 的原生实现，移除固定旧版本的 qwen-asr 包及其 Web UI 依赖；独立环境使用58项精确版本。安装采用 sync 清除多余包，不能只在旧环境追加安装。模型和 Python 环境仍不进入生产镜像，旧私有报告与声音文件保持不变；新的导入要求当前固定模型/运行版本，已保存的旧报告仍可读取和人工核对。

2026-10-07 依赖检查：旧环境检出9项唯一 GHSA；新的58项依赖 OSV 查询未发现告警。依赖检查不替代真实模型推理和后台导入验收，也不表示未来不会出现新告警。

原生实现已对8段现有法语试听运行完整命令，实际模型权重、配置、分词器等6文件哈希均通过。7段预测通过范围检查，1段保留重叠（前词结束0.72秒、后词开始0.56秒），命令返回2，全部结果仍要求人工核对。修正处理器参数传递后复验结果完全一致且无弃用提示。该输入使用明确的测试清单，不是生产生成或人工审听回执；没有登记或发布正式音频。

## 所有者直接发布输入

2026-10-07 所有者取消逐项人工审批后，可使用本机 `speech-plan-export-direct <plan-id> <operator-email> <new-private-output.tar>` 获取中性输入包。服务端仍校验当前管理员、固定计划、全部最新成功片段、原始/修复媒体配对、实际哈希和解码；已明确退回的最新片段不能导出。生成前后核对同一快照与权限，不调用 TTS，不写任何试听决定。

新包具有 `kind: brioche-speech-inputs`、`publicationPolicy: owner-direct-publish` 和 `humanListeningAsserted: false`，片段 review 为 null。只有显式 `--direct` 才接受它；不会将这类包冒充旧人工审核输入：

```powershell
.local/alignment-native-venv/Scripts/python.exe scripts/alignment/align.py .local/private/direct-input.tar --direct --check
.local/alignment-native-venv/Scripts/python.exe scripts/alignment/align.py .local/private/direct-input.tar --direct --output .local/private/direct-predictions-v1.json
```

这类报告标识为 `brioche-automatic-alignment-predictions`、reviewRequired=false，保留同样的真实模型原始预测、异常和固定来源包哈希。取消人工门槛不会取消模型、媒体、词范围或时间区间校验，也不会自动修补异常或登记/发布课程。旧人工报告导入API仍要求原类型；直接报告的正式组装入口尚在接入中，不能直接送入旧审核API。

### 显式法语数字输入映射

`--spoken-cardinals` 使用固定 `transcript-aliases.json` 中的单词数词拼写，例如源词 `20` 对应模型输入 `vingt`。数字仍保持原词文本及 Unicode scalar 区间；`rawPredictions` 保留实际模型输入文字和80ms分类，不改写为数字。每个使用映射的片段记录 `transcriptAliases`，引擎声明独立策略；服务端核对固定字典、原词/索引、无重复、实际预测文字及原始分类毫秒与结果完全相同，旧策略不能携带别名。1涉及性别，复合数词需要多个模型词，均不在此一对一策略内；不按猜测扩展。词数、重叠、零时长、越界检查保持。此选项不声明人类审听，也不保证模型推理一定通过。
