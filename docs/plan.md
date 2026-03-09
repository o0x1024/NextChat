# Desktop-First 多 Agent 聊天室实施计划

## Summary
- 首版只做 Desktop，技术栈锁定为 `Tauri v2 + React + TypeScript + Rust + rig-core + SQLite`。
- 运行架构采用 `Tauri UI + 本地 Rust runtime`，不依赖远程后端；数据、记忆、审计、任务都本地持久化。
- 核心调度不采用“智能 Orchestrator 自动分工”，而是采用一个不可配置的系统 actor：`Coordinator`。
- `Coordinator` 不负责思考，只负责 `task card 生成、claim 竞价、lease 发放、抢占、审批、审计、恢复`。
- agent 采用 `自由认领` 模式，但只能认领 `Coordinator` 规范化后的任务卡，不能直接绕过系统状态机。
- human 指令始终最高优先级；新指令默认通过 `安全点抢占` 中断或重排已执行任务。

## 目标产物
- 一个本地桌面应用，支持创建 agent、配置角色/工具/skills、创建 work group、多人格 agent 群聊、@协作、并行执行、任务追踪、审批、记忆管理。
- 用户能在一个群聊中看到摘要化协作过程，并可展开查看 agent 内部协作和工具执行细节。
- 用户能中断、改派、锁定工具、批准高风险动作、回看审计链。

## 核心机制定案
- human 发言先进入主会话，再被 `Coordinator` 转为一个或多个 `TaskCard`。
- 顶层 human 任务不允许 agent 直接抢原始消息；必须先转成 `TaskCard`。
- agent 可对 `TaskCard` 或后续子任务提交 `ClaimBid`。
- `Coordinator` 基于 `角色匹配、工具覆盖、当前负载、历史成功率、human @提及优先级` 计算能力分，并只发放一个主执行 `Lease`。
- 如任务需要协作，主执行 agent 可以创建 `SubTaskCard`；其他 agent 继续按同样机制认领。
- 支持协作型子任务，但每个任务只能有一个 `owner lease`；协作 agent 通过显式依赖的子任务参与，不做多人共持同一 lease。
- human 新指令到达时，`Coordinator` 将运行中任务标记为 `preempt_requested`；worker 在安全点暂停、取消或重排。
- 高风险工具调用不走自由认领，统一进入审批状态机。

## Desktop 技术架构
- 前端：`React + TypeScript + Vite + Tauri v2 frontend`。
- 状态层：`Zustand` 或等价轻量 store，按 `chat/workgroup/agent/runtime` 分片。
- 桌面宿主：`src-tauri` 内启动长期存活的 `Tokio runtime`。
- Rust 核心分层：
  - `core-domain`：领域类型、状态机、事件定义。
  - `storage`：SQLite schema、仓储、迁移、向量索引。
  - `coordinator`：任务卡、claim、lease、抢占、恢复。
  - `agent-runtime`：agent 生命周期、上下文拼装、技能注入、工具调用。
  - `tool-runtime`：内置工具注册、权限、超时、子进程隔离。
  - `llm-rig`：rig-core provider adapter、会话组装、流式输出。
  - `app-shell`：Tauri commands、事件桥接、窗口管理、系统通知。
- UI 与 Rust 通过 `Tauri commands + Tauri event stream` 通信，不引入本地 HTTP 服务。
- 浏览器/文件/命令类高风险工具运行在独立 worker 进程；普通低风险工具可在主 runtime 中运行。

## rig-core 集成方案
- 每个 agent 对应一个 `AgentRuntime` 实例，内部封装：
  - `persona prompt`
  - `skill prompt fragments`
  - `tool registry`
  - `memory resolver`
  - `model policy`
- `rig-core` 负责：
  - 多模型接入抽象
  - tool calling
  - streaming completion
  - structured output
- 平台自定义负责：
  - 群聊上下文裁剪
  - claim/lease 状态机
  - 任务图
  - 记忆召回
  - 审批与审计
- `skill` 不直接执行代码，只能影响 `prompt、planning hints、tool allowlist、done criteria`。
- `tool` 通过统一 `ToolManifest + Rust handler` 暴露给 rig-core。

## UI 信息架构
- 主布局分 4 个区域：
  - 左侧：work group 列表、agent 目录、临时任务组入口。
  - 中间：主群聊时间线。
  - 右侧：当前 work group 的任务图、运行中 lease、审批卡片。
  - 抽屉/面板：agent 配置、tool 配置、skills、记忆与审计。
- 主时间线默认只展示：
  - human 消息
  - agent 阶段性摘要
  - 关键工具结果
  - 审批卡
  - 任务状态变化
- 内部协作、详细推理、完整工具输入输出进入 `Backstage` 面板。
- agent 配置页必须支持：
  - 名称、头像、角色、目标
  - 默认模型
  - skills 组合
  - 可用工具
  - 最大并发数
  - 记忆读写策略
  - 是否可发起子任务
- work group 页必须支持：
  - 持久组 / 临时组
  - 共享目标
  - 成员 agent
  - 默认摘要粒度
  - 自动归档策略

## 数据模型与重要类型
- `AgentProfile { id, name, role, objective, model_policy, skill_ids, tool_ids, max_parallel_runs, can_spawn_subtasks, memory_policy }`
- `WorkGroup { id, kind, name, goal, member_agent_ids, default_visibility, created_at, archived_at }`
- `ConversationMessage { id, conversation_id, sender_kind, sender_id, kind, visibility, content, mentions, task_card_id, created_at }`
- `TaskCard { id, parent_id, source_message_id, title, normalized_goal, input_payload, priority, status, work_group_id, created_by, created_at }`
- `ClaimBid { id, task_card_id, agent_id, rationale, capability_score, expected_tools, estimated_cost, created_at }`
- `Lease { id, task_card_id, owner_agent_id, state, granted_at, expires_at, preempt_requested_at, released_at }`
- `ToolManifest { id, name, risk_level, input_schema, output_schema, timeout_ms, concurrency_limit, permissions }`
- `ToolRun { id, tool_id, task_card_id, agent_id, state, approval_required, started_at, finished_at, result_ref }`
- `MemoryItem { id, scope, scope_id, content, tags, embedding_ref, pinned, ttl, created_at }`
- `AuditEvent { id, event_type, entity_type, entity_id, payload_json, created_at }`

## 对外接口与应用内公共 API
- Tauri commands：
  - `create_agent_profile`
  - `update_agent_profile`
  - `create_work_group`
  - `add_agent_to_work_group`
  - `send_human_message`
  - `list_task_cards`
  - `approve_tool_run`
  - `cancel_task_card`
  - `pause_lease`
  - `resume_task_card`
  - `get_audit_events`
- Tauri events：
  - `chat.message.created`
  - `task.card.created`
  - `claim.bid.submitted`
  - `lease.granted`
  - `lease.preempt_requested`
  - `task.status.changed`
  - `tool.run.started`
  - `tool.run.completed`
  - `approval.requested`
  - `memory.updated`
- Rust traits：
  - `trait AgentExecutor`
  - `trait ToolHandler`
  - `trait MemoryStore`
  - `trait ClaimScorer`
  - `trait ModelProviderAdapter`

## 存储与恢复
- 所有业务数据落本地 SQLite，路径位于系统 app data 目录。
- 启动时执行 migration，并恢复：
  - 未完成 task card
  - 未释放 lease
  - 待审批 tool run
  - 中断前的 conversation cursor
- 恢复策略：
  - 若任务处于高风险工具执行中断点，标记为 `needs_review`
  - 若低风险任务中断，允许 `resume_from_last_safe_point`
- 记忆分层：
  - `user memory`
  - `work group memory`
  - `agent memory`
- 首版使用本地向量索引，不做云同步。

## 工具与技能系统
- 首版只支持内置工具，不做第三方插件市场。
- 内置工具最小集合：
  - 文件读写
  - 项目搜索
  - Shell 命令
  - HTTP 请求
  - 浏览器自动化
  - Markdown/文本处理
  - 计划与总结工具
- 风险等级：
  - `low`：自动执行
  - `medium`：自动执行但强审计
  - `high`：必须 human 审批
- `skill` 首版作为结构化配置对象存储在 SQLite，不热插拔脚本，不允许运行任意代码。

## 里程碑
1. 建立 Tauri v2 desktop skeleton、React UI、Rust core crate 拆分、SQLite migration、基础事件总线。
2. 完成 agent/work group/message/task card/lease 的领域模型与存储层。
3. 完成主群聊 UI、agent 配置页、work group 管理页、任务图侧栏。
4. 完成 `Coordinator`、claim bidding、能力分评分、lease 发放、安全点抢占。
5. 接入 rig-core，打通单 agent 流式回复、tool calling、skill 注入、消息摘要。
6. 完成多 agent 子任务协作、@agent、并发执行、Backstage 细节视图。
7. 完成审批系统、审计日志、记忆层、恢复机制、工具进程隔离。
8. 完成打包、崩溃恢复、性能测试、Beta polish。

## 当前实现判断
- 已基本完成：
  - Desktop skeleton、React + Tauri + Rust + SQLite 基础工程。
  - agent / work group / message / task card / lease / tool run / audit / memory 的基础数据模型与持久化。
  - 主群聊、任务面板、审批面板、工具面板、Backstage 基础 UI。
  - `Coordinator` 的基础 claim scoring、lease 发放、抢占标记、审批入口。
- 部分完成：
  - rig-core 已接入，但仍是单次 completion 级别，未完成流式输出、结构化输出、完整 tool calling。
  - 子任务协作已具备骨架，但仍是简化版本，尚未形成完整的多 agent 并发编排。
  - 审批、审计、记忆已打通最小闭环，但未达到文档目标中的完整能力。
- 尚未完成：
  - 启动恢复策略、崩溃恢复、工具独立 worker 隔离、完整记忆召回、任务改派/工具锁定、性能测试与 Beta polish。

## 实施 TODO（按优先级）

### P0：先补系统闭环与可靠性
- 恢复机制
  - 启动时扫描未完成 `task card`、未释放 `lease`、待审批 `tool run`。
  - 按文档规则恢复到 `paused`、`needs_review`、`queued` 等可解释状态。
  - 补充恢复审计事件与恢复后 UI 提示。
- 工具执行安全边界
  - 将 `shell`、文件写入、浏览器自动化等高风险工具迁移到独立 worker 进程。
  - 补齐超时、中断、失败回收、输出裁剪、权限边界与审计记录。
  - 明确 low / medium / high 风险级别对应的执行和审批策略。
- 真实任务执行闭环
  - 从“关键词选工具 + 固定 summary”升级为可配置执行流程。
  - 让 agent 能基于任务上下文决定是否调用工具、何时结束、何时等待审批。
  - 完成任务状态机收口，避免出现 task / lease / tool run 状态不一致。
- 多 agent 协作最小可用版本
  - 支持多个子任务，而不是只派生单个固定 review 子任务。
  - 支持父任务等待全部子任务完成后再统一收口。
  - 明确子任务失败、取消、需复核时对父任务的影响。
- 审批链完善
  - 审批卡展示工具、输入摘要、风险说明、发起 agent、所属 task。
  - 支持批准、拒绝、拒绝原因记录。
  - 审批结果写入审计链，并驱动任务恢复或进入 `needs_review`。

### P1：补齐核心产品能力
- 记忆系统完整化
  - 补齐 `user memory`、`work group memory`、`agent memory` 三层读写。
  - 接入检索与召回链路，让记忆真正参与任务执行。
  - 实现 `pinned`、`ttl`、基础清理策略与 UI 管理入口。
- rig-core 深化集成
  - 支持流式输出。
  - 支持结构化输出。
  - 支持更完整的 tool calling 和失败回退策略。

## 当前可用性差距

当前代码状态到“用户实际可用的多 Agent MVP”之间的差距、优先级、完成标准，已拆分到独立文档：

- [gap-analysis.md](/Users/like/code/NextChat/docs/gap-analysis.md)
  - `GA-01 .. GA-11` 已全部完成。
- 当前状态更适合描述为：
  - 已打通的多 Agent 桌面协作 MVP
  - 具备权限、skill、memory、审批、协作消息、调度解释性
- 下一阶段建议聚焦：
  - 打包与发布链路
  - 崩溃恢复和异常压测
  - 高并发性能验证
  - Beta polish 与长任务体验

### P2：发布前收尾
- 打包与发布流程
  - 完成开发、预发布、正式包的构建链路。
  - 校验 app data 目录、权限、首次启动体验。
- 崩溃恢复与异常测试
  - 验证应用异常退出后，待审批、高风险工具、未完成任务的恢复行为。
  - 为关键状态机补自动化测试。
- 性能与并发测试
  - 覆盖文档中的 `10 agents / 20 并发子任务` 场景。
  - 验证 UI 主线程不阻塞、事件风暴下仍可交互。
- Beta polish
  - 空状态、错误态、长任务反馈、国际化文案补齐。
  - 优化主时间线摘要质量与 Backstage 信息密度。

## 推荐开发顺序
1. 先完成恢复机制、工具隔离、审批链和任务状态机一致性。
2. 再完成多子任务协作、真实 tool calling、记忆召回。
3. 最后做改派、锁定工具、性能测试、打包与 Beta polish。

## 测试 cases 与验收场景
- 创建 3 个 agent，分别配置不同角色与工具，加入同一 work group，human 发出复合任务，系统能生成 task card 并只发放一个 owner lease。
- 多个 agent 同时对同一 task card 提交 claim，系统按能力分正确裁决。
- human `@某个 agent` 时，该 agent 的 claim 得分应被抬高，但仍不允许绕过 lease 机制。
- 主执行 agent 创建 2 个子任务后，其他 agent 可认领并并行执行，父任务状态正确等待依赖完成。
- human 在任务执行中发送新指令时，运行中的 lease 被标记为 `preempt_requested`，并在安全点暂停或取消。
- 高风险工具调用必须进入审批；未经批准不得执行。
- 应用异常退出后重启，未完成任务、待审批记录、审计链可恢复。
- 主时间线默认展示摘要，Backstage 可查看完整工具输入输出和协作过程。
- 本地 SQLite 中的消息、任务、审批、审计、记忆可以相互追溯，主键链路完整。
- 在 10 个 agent、20 个并发子任务下，UI 仍保持可交互，主聊天流不阻塞。

## 假设与默认值
- 当前是 greenfield 项目，没有既有代码约束。
- 首版严格限制为 Desktop，不做 Web/Mobile 同步与远程控制。
- `Coordinator` 是不可配置系统 actor，但它是“调度内核”，不是“智能经理”；任务思考与执行仍由 agent 完成。
- `自由认领` 仅指 agent 可以主动竞价与认领任务卡，不代表可以绕过系统状态机直接抢占 human 原始消息。
- 默认前端技术选型为 `React + Vite`，默认 Rust 数据层选 `SQLite + sqlx`。
- 若未来需要切回更强的集中式 Orchestrator，可在不改 UI 协议的前提下把 `ClaimScorer` 升级为更智能的规划器。
