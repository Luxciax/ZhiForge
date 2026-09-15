# SelectionTranslator Documentation

## v0.2.0 阅读顺序

### 1. `v0.2-product-spec.md`

先读产品规格。

回答：

- v0.2.0 要解决什么问题；
- 哪些是 P0 / P1 / P2；
- 用户体验不能被破坏的核心是什么；
- Settings / Toolbar / Result Window 应该长什么样；
- 哪些功能暂时不要做。

### 2. `v0.2-core-architecture.md`

再读核心架构。

回答：

- Provider Profile 与 Protocol Adapter 如何分离；
- Action / Route / Prompt 如何解耦；
- Selection Engine 的边界；
- Registry 如何替代大量 switch；
- Event / DTO / Error 如何稳定跨模块协议；
- 后续新 Provider / Action 应从哪里扩展。

### 3. `v0.2-extension-maintenance-guide.md`

开发前必须读实施与维护规范。

回答：

- Platform Port 怎么设计；
- 配置和 Credential 怎么迁移；
- ResultWindow / AI Runtime 怎么拆；
- 哪些实现方式明确禁止；
- 测试和 Release 如何验收；
- Phase 1–6 的开发顺序；
- 如何验证架构真的方便二开。

---

## 开发前最重要的八条原则

1. Selection 只负责选中文本，不关心 AI。
2. Action 只描述用户要做什么，不绑定具体协议。
3. Route 决定 Action 使用哪个 Provider / Model。
4. Provider Profile 保存渠道，Adapter 只实现协议。
5. UI 只调用 Service，不越层操作网络和平台。
6. 系统能力通过 Platform Port 暴露。
7. 扩展优先 Registry，不优先修改核心 switch。
8. 配置永远可迁移，Secret 永远独立保存。

## v0.2.0 实施入口

从 `v0.2-extension-maintenance-guide.md` 的 **Phase 1 — Domain + Storage Foundation** 开始。

不要直接先重写模型设置 UI。先建立 Domain、ProviderProfile、Registry、Credential 分层和 Settings Migration，再进入多渠道 UI。
