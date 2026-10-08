# 共享 Web 的产品范围

通用 React Router 路由、客户端状态与 SSR 边界全部由 Chef 实现。产品只通过可信构建配置 `Product.sessionNamespace` 选择 `brioche` 或 `hargow`，不复制页面、恢复逻辑或测试。配置不能来自 URL、Host 或浏览器存储；Hargow 必须显式指定 `hargow`。已知产品 ID 与 namespace 不一致、未知 namespace 均在加载共享消费者时失败。Brioche 和 Chef 兼容测试壳的缺省仍为 Brioche。

## 草稿与待确认操作

学习会话、收藏及复习的 sessionStorage 键包含产品、账号与固定会话/版本。Brioche 既有 `brioche.learning.v1:` 键不改写；Hargow 使用独立 `hargow.learning.v1:` 键。恢复列表、离开提醒、原请求确认和账号切换/退出清理都使用同一可信 namespace。清理当前产品账号不会删除另一产品或另一账号的草稿。幂等确认仍只删除匹配原 idempotencyKey 的记录，不自动向另一入口重发。

草稿事件使用产品范围，只有无数据的变更通知。身份 localStorage 通知也按产品分开，值只有随机 UUID；监听当前产品通知后释放播放、清理当前账号草稿并重新加载服务端授权页面。其他产品通知不触发该页面失效。焦点、可见性、定时与 BFCache 核验仍保留，密码重置等全局身份变化最终以服务器当前权限为准。产品范围的客户端键不是服务端权限机制。

## SSR 与证据

SSR 从同一可信 namespace 过滤精确会话 Cookie，只转发当前产品的一枚 Cookie；重复或非法会话在内部请求前拒绝。账号共享不意味着共享跨域 Cookie、自动 SSO 或产品管理权限。

验证由 Chef 持有：纯函数测试在相同账号/编号下检查恢复、清理和通知互不串用；实际 Vite SSR 加载检查缺省、合法及缺失/错配配置；合成 Hargow SSR API 检查精确 Cookie 与产品设置。独立 Chromium 会话加载真实 Hargow pending-saves 组件，同时种入 Brioche 哨兵草稿，验证外部确认和账号切换只清理 Hargow、Brioche 完整值保持且不自动提交请求。法语样例只用于协议测试，不表示粤语内容或音频质量已验证。

此阶段不更新产品依赖 pin 或生产部署。语言中立课程契约、真实粤语课程/语音、Hargow 服务入口及完整双产品装配仍须完成后统一验收。
