# 群聊编排系统 V3：串并行消息流可视化 + 用户直派 Agent

## 摘要
本版方案基于你的两个新增要求进行重构：

1. 串并行任务的主聊天区，必须像“真实项目群协作”那样逐步展示。
2. 用户可以直接 `@某个 agent` 派任务，不经过群主。

因此，系统不再只有“群主统一编排”一种模式，而是明确分成两条主路径：

- `群主编排模式`
  - 适合项目型、阶段型、串并行混合任务
  - 主聊天完整展示“规划 -> 派单 -> 回执 -> 阶段完成 -> 下一阶段”
- `用户直派模式`
  - 适合用户明确 `@agent` 指派一个具体任务
  - 不经过群主派单
  - agent 直接接单、执行、回报

目标是让消息区本身就成为“项目推进看板”，用户不用点到执行详情，也能直观看到：

- 现在进行到哪一阶段
- 当前由谁负责
- 哪个任务已完成
- 哪个任务正在推进
- 哪个任务被阻塞

---

## 一、最终产品行为定义

## 1.1 群主编排模式
适用场景：

- 用户没有显式指定 agent
- 用户提的是一个项目型目标
- 任务天然存在阶段顺序或跨角色协作

典型例子：

`帮我开发一个图书管理系统`

消息区应严格按这种叙事展开：

1. 用户发目标
2. 群主确认接收并说明要开始规划
3. 群主派发第 1 阶段任务
4. 被派 agent 回执
5. agent 完成并回报群主
6. 群主根据结果推进下一阶段
7. 到可并行阶段时，群主一次性派给多个 agent
8. 各 agent 分别汇报结果
9. 群主汇总本阶段或全局进度
10. 最终群主输出项目总结

这条链路的重点不是“系统后台自动跑了什么”，而是“群主在群里组织大家做事”。

---

## 1.2 用户直派模式
适用场景：

- 用户明确 `@某个 agent`
- 用户希望跳过群主，直接让 agent 做事

典型例子：

`@前端开发1 帮我把登录页做出来`
`@CodeAuditor 审计一下当前项目的鉴权逻辑`

系统行为：

1. 不生成群主确认消息
2. 不生成群主派单消息
3. 被 `@` 的 agent 直接回执
4. agent 直接执行并产出结果
5. 消息区展示该 agent 的进展与结果

这条路径的设计目标是：用户可以把群当成“多人工作台”，既可以让群主组织，也可以直接点人干活。

---

## 二、统一请求入口的路由规则

## 2.1 请求模式枚举
新增：

```ts
type RequestRouteMode =
  | "owner_orchestrated"
  | "direct_agent_assign"
  | "direct_answer";
```

### 含义
- `owner_orchestrated`
  - 群主负责拆解、阶段推进、汇总
- `direct_agent_assign`
  - 用户直接指定 agent，绕过群主派单
- `direct_answer`
  - 普通问答，不创建任务流

---

## 2.2 路由判定优先级
固定优先级如下：

1. 用户显式 `@agent`
2. 否则判断是否为普通问答
3. 否则进入群主编排

### 规则 1：用户显式 `@agent`
如果消息里 `@` 到一个或多个具体 agent，则默认进入 `direct_agent_assign`。

例外：

- 如果用户同时明确要求“请群主安排”或 `@群主`
- 或消息是整体项目目标而且只是在描述参与成员，不是直接下达任务

则进入 `owner_orchestrated`，但保留用户点名约束。

### 规则 2：普通问答
如果是知识性、解释性、建议性问题，进入 `direct_answer`。

### 规则 3：剩余都走群主编排
如：

- 开发一个系统
- 做一个项目
- 规划一个方案
- 分阶段推进一个目标

---

## 三、主聊天区的设计目标

## 3.1 消息区不是日志，而是项目推进叙事
主聊天必须展示以下内容：

- 谁发起了目标
- 群主做了什么安排
- 哪个 agent 接了什么任务
- 哪个 agent 完成了什么
- 当前进入到哪个阶段
- 当前哪些任务并行进行
- 哪些任务被阻塞
- 下一步谁接手

主聊天默认不显示：

- 原始工具调用 JSON
- LS / Read / Grep / WebSearch 等细节
- 技术中间输出碎片

这些都进入“执行详情”。

---

## 3.2 主聊天的标准消息类型
新增主聊天语义类型：

```ts
type NarrativeMessageType =
  | "owner_ack"
  | "owner_plan"
  | "owner_dispatch"
  | "agent_ack"
  | "agent_progress"
  | "agent_delivery"
  | "owner_stage_transition"
  | "blocker_raised"
  | "blocker_resolved"
  | "owner_summary"
  | "direct_assign"
  | "direct_result";
```

底层可以先复用现有 `ConversationMessage.kind`，但前端必须按 `message meta` 渲染成上面的语义。

---

## 四、群主编排模式的工作流模型

## 4.1 引入 Workflow / Stage / Task 三层结构
项目型任务必须显式建模为：

1. `Workflow`
   - 一次完整目标
2. `Stage`
   - 阶段
   - 阶段之间默认串行
3. `Task`
   - 阶段内任务
   - 任务之间可串可并

---

## 4.2 数据结构

```ts
type Workflow = {
  id: string;
  workGroupId: string;
  sourceMessageId: string;
  routeMode: "owner_orchestrated";
  title: string;
  normalizedIntent: string;
  status: WorkflowStatus;
  ownerAgentId: string;
  currentStageId?: string | null;
  createdAt: string;
};
```

```ts
type WorkflowStage = {
  id: string;
  workflowId: string;
  title: string;
  goal: string;
  order: number;
  executionMode: "serial" | "parallel";
  status: StageStatus;
  entryMessageId?: string | null;
  completionMessageId?: string | null;
};
```

```ts
type WorkflowTask = {
  id: string;
  workflowId: string;
  stageId: string;
  parentTaskId?: string | null;
  title: string;
  goal: string;
  assigneeAgentId: string;
  dispatchSource: "owner_assign" | "user_direct";
  dependsOnTaskIds: string[];
  status: TaskStatus;
  executionStandard?: string | null;
  deliverablesJson?: string | null;
  acknowledgedAt?: string | null;
  resultMessageId?: string | null;
  createdAt: string;
};
```

---

## 4.3 状态定义

```ts
type WorkflowStatus =
  | "planning"
  | "running"
  | "blocked"
  | "needs_user_input"
  | "completed"
  | "needs_review"
  | "cancelled";
```

```ts
type StageStatus =
  | "pending"
  | "ready"
  | "running"
  | "blocked"
  | "completed"
  | "needs_review"
  | "cancelled";
```

```ts
type TaskStatus =
  | "pending"
  | "ready"
  | "leased"
  | "in_progress"
  | "blocked_by_dependency"
  | "blocked_by_owner"
  | "blocked_by_user"
  | "waiting_approval"
  | "completed"
  | "needs_review"
  | "cancelled";
```

---

## 五、串并行任务的推进规则

## 5.1 阶段之间默认串行
固定规则：

- 当前阶段未完成，下一阶段不启动
- 当前阶段 `blocked`，下一阶段不启动
- 当前阶段 `needs_review`，默认不自动推进下一阶段
- 当前阶段全部关键任务完成，群主发“阶段完成”消息后进入下一阶段

---

## 5.2 阶段内串行
如果阶段的 `executionMode = serial`：

- 群主一次只派发一个 ready task
- 当前 task 完成后，自动解锁下一个 task
- 消息区表现为“一个人做完，另一个人接手”

这非常适合：

- 产品经理先出 PRD
- 架构师再看 PRD 出技术方案

---

## 5.3 阶段内并行
如果阶段的 `executionMode = parallel`：

- 群主一次派发所有无依赖的 ready tasks
- 多个 agent 同时接单
- 每个 agent 独立推进
- 全部完成后，群主宣布本阶段完成

这非常适合：

- 前后端同时开发
- 多个审计方向同时检查
- 测试和安全复核同时进行

---

## 5.4 串并混合
一个 workflow 可以这样定义：

- Stage 1：串行
- Stage 2：串行
- Stage 3：并行
- Stage 4：并行
- Stage 5：串行

这正对应你说的“图书管理系统”场景。

---

## 六、图书管理系统场景的标准消息流

## 6.1 目标
用户输入：

`帮我开发一个图书管理系统`

## 6.2 系统采用 `owner_orchestrated`
群主先规划。

## 6.3 标准阶段模板
默认拆成 5 个阶段。

### Stage 1：需求定义
执行模式：`serial`

任务：

- `ProductManager` 输出 PRD

### Stage 2：技术方案
执行模式：`serial`

任务：

- `Architect` 基于 PRD 输出技术栈和架构设计

### Stage 3：实施开发
执行模式：`parallel`

任务：

- `BackendEngineer1`
- `BackendEngineer2`
- `FrontendEngineer1`
- `FrontendEngineer2`

### Stage 4：测试验收
执行模式：`parallel`

任务：

- `QATester`
- `SecurityReviewer`

### Stage 5：发布收尾
执行模式：`serial`

任务：

- `ReleaseOwner` 输出上线说明和交付总结

---

## 6.4 主聊天必须呈现的标准话术
以下文案作为产品默认模板，不留给实现者二次决定。

### 1. 用户发起
`用户：帮我开发一个图书管理系统`

### 2. 群主确认
`群主：好的，我来规划和安排一下。`

### 3. 群主发布阶段计划
`群主：本次任务分为 5 个阶段：需求定义、技术方案、实施开发、测试验收、发布收尾。我先安排需求定义阶段。`

### 4. 群主派发 PRD
`群主：@产品经理 你先出一版图书管理系统 PRD，包含核心角色、主要功能、业务流程和验收标准。`

### 5. 产品经理接单
`产品经理：收到，我先整理系统 PRD。完成后我会同步文档。`

### 6. 产品经理交付
`产品经理：@群主 已完成系统 PRD，文档已整理如下：……`

### 7. 群主推进下一阶段
`群主：需求定义阶段已完成。@架构师 请基于 PRD 评估系统架构、技术栈、模块划分和部署方案。`

### 8. 架构师接单
`架构师：收到，我将基于 PRD 输出系统架构与技术方案。`

### 9. 架构师交付
`架构师：@群主 已完成技术方案，文档已放入项目文档中，请查看。`

### 10. 群主宣布进入开发阶段
`群主：技术方案阶段已完成，进入实施开发阶段。`

### 11. 群主并行派单
`群主：@后端开发1 负责用户与认证模块；@后端开发2 负责图书与借阅模块；@前端开发1 负责管理后台界面；@前端开发2 负责登录与借阅交互页面。开始编码。`

### 12. 多个开发分别接单
每个 agent 独立回一条：

- `后端开发1：收到，我开始实现用户与认证模块。`
- `后端开发2：收到，我开始实现图书与借阅模块。`
- `前端开发1：收到，我开始开发管理后台界面。`
- `前端开发2：收到，我开始开发登录与借阅页面。`

### 13. 开发完成分别汇报
每个 agent 独立汇报：

- `后端开发1：@群主 用户与认证模块已完成，代码已提交。`
- `前端开发1：@群主 管理后台页面已完成，界面可预览。`

### 14. 群主推进测试阶段
`群主：实施开发阶段已完成，进入测试验收阶段。@测试工程师 @安全审查员 请开始验证。`

### 15. 群主最终汇总
`群主：图书管理系统开发流程已完成，当前已具备 PRD、技术方案、核心编码、测试结果和上线说明。以下是最终交付摘要：……`

这套叙事就是消息区默认表现。

---

## 七、用户直派 Agent 模式

## 7.1 路由规则
如果用户显式 `@某个 agent`，且不是要求“群主安排”，则默认进入 `direct_agent_assign`。

例子：

- `@前端开发1 先做一个登录页`
- `@架构师 给这个系统出一版技术方案`
- `@CodeAuditor 审计一下这个仓库的身份认证逻辑`

---

## 7.2 行为规则
系统行为必须固定为：

1. 不创建群主确认消息
2. 不创建群主派单消息
3. 直接给目标 agent 创建任务
4. agent 直接回执
5. agent 直接执行
6. agent 直接向用户回报结果

---

## 7.3 多 agent 直派
如果用户一次 `@多个 agent`：

例子：

`@后端开发1 @后端开发2 分别把用户模块和图书模块做掉`

处理规则：

- 如果用户给出了清晰的职责划分，则各自创建独立 direct task
- 如果用户只是笼统地 `@多个 agent` 但没分工，系统不自动让群主介入，而是要求被点名的第一个 agent 或系统生成一条澄清请求给用户

默认选择：

- 只要职责不清晰，就直接向用户澄清
- 不悄悄转给群主

---

## 7.4 直派模式下的阻塞处理
在 `direct_agent_assign` 下，阻塞升级目标默认是“用户”，不是群主。

规则：

- 用户直接派的任务，责任链就是 `用户 -> agent`
- agent 遇到缺信息、方向冲突、审批需求时，直接 `@用户`
- 不默认转给群主

示例：

`前端开发1：@用户 当前缺少页面结构和视觉要求，请确认是管理后台风格还是简洁读者端风格。`

---

## 7.5 混合模式
如果用户说：

`@架构师 你先看下这个项目，然后群主再安排研发`

则进入混合规则：

- 当前这条消息先按 `direct_agent_assign` 路由给架构师
- 架构师完成后，如果用户或系统明确触发“请群主安排后续”，再进入 `owner_orchestrated`

不自动替用户补脑切换模式。

---

## 八、群主编排模式下的阻塞升级机制

## 8.1 基本原则
你提出的方向是正确的：

- agent 遇到问题时，默认 `@群主`
- 群主负责协调并解决后继续推进
- 只有真正需要用户决策时，群主才 `@用户`

---

## 8.2 新增 Blocker 数据结构

```ts
type TaskBlocker = {
  id: string;
  taskId: string;
  workflowId?: string | null;
  raisedByAgentId: string;
  resolutionTarget: "owner" | "user";
  category:
    | "missing_dependency"
    | "missing_context"
    | "permission_required"
    | "tool_failure"
    | "design_conflict"
    | "need_user_decision";
  summary: string;
  details: string;
  status: "open" | "resolved" | "cancelled";
  createdAt: string;
  resolvedAt?: string | null;
};
```

---

## 8.3 群主编排模式下的阻塞规则
如果任务是群主派的，则：

- agent 先 `@群主`
- 群主尝试内部解决
- 群主可执行以下动作之一：
  - 补充说明
  - 改派给更合适 agent
  - 创建前置依赖任务
  - 请求审批
  - 暂停当前任务
  - 向用户提问

---

## 8.4 典型阻塞对话
### agent 提出阻塞
`后端开发2：@群主 当前图书借阅模块被阻塞，缺少架构层定义的借阅状态流转规则，建议先补充接口与状态模型。`

### 群主处理
群主可发：

`群主：收到，这个阻塞由我协调。@架构师 请补充借阅状态流转和接口约束。`

或者：

`群主：收到，这个问题需要用户确认。@用户 借阅超期后是直接禁止续借，还是允许支付罚金后续借？`

### 问题解决后恢复
`群主：借阅规则已明确，@后端开发2 继续推进实现。`

---

## 八、用户显式点名与群主编排的冲突规则

## 8.1 用户点名优先
如果用户在群主编排请求中显式指定某一角色必须做某事，则群主必须尊重。

例子：

`帮我开发一个图书管理系统，@产品经理 先出 PRD，后续让群主安排`

处理规则：

- Workflow 仍由群主编排
- 但 Stage 1 的 assignee 被锁定为 `产品经理`

---

## 8.2 同时 `@群主` 和 `@agent`
如果用户同时点名群主和 agent：

- 若语义是“请群主安排，但某任务指定给某 agent”
  - 走 `owner_orchestrated`
  - 被点名的 task assignee 固定
- 若语义是“我直接让某 agent 做事，不需要群主”
  - 走 `direct_agent_assign`

默认判定关键词：

- 包含“安排、规划、组织、分配、推进” -> 群主编排
- 包含“你来做、直接处理、马上完成” -> 用户直派

---

## 九、后端接口与模型变更

## 9.1 新增路由模式
```ts
type RequestRouteMode =
  | "owner_orchestrated"
  | "direct_agent_assign"
  | "direct_answer";
```

---

## 9.2 新增 Workflow 结构
用于项目型群主编排。

新增表或持久化模型：

- `workflows`
- `workflow_stages`
- `task_blockers`

---

## 9.3 Task 扩展
在现有 `TaskCard` 基础上新增：

```ts
dispatchSource: "owner_assign" | "user_direct";
workflowId?: string | null;
stageId?: string | null;
dependsOnTaskIds?: string[] | null;
acknowledgedAt?: string | null;
resultMessageId?: string | null;
lockedByUserMention?: boolean;
```

### 字段含义
- `dispatchSource`
  - 谁发起的任务
- `workflowId`
  - 属于哪个项目工作流
- `stageId`
  - 属于哪个阶段
- `dependsOnTaskIds`
  - 串并依赖
- `lockedByUserMention`
  - 用户显式点名，群主不得改派

---

## 9.4 新增消息元数据
必须为消息补充渲染元信息：

```ts
type MessageMeta = {
  narrativeType: NarrativeMessageType;
  workflowId?: string | null;
  stageId?: string | null;
  taskId?: string | null;
  progressPercent?: number | null;
  blocked?: boolean;
};
```

如果不建新表，先在 message content 旁边增加 `meta_json` 字段，或者通过结构化 content 存储。

---

## 9.5 新增核心服务函数
```rust
pub async fn classify_request_route_mode(
    &self,
    work_group: &WorkGroup,
    source_message: &ConversationMessage,
    members: &[AgentProfile],
) -> Result<RequestRouteMode>
```

```rust
pub async fn build_owner_workflow_plan(
    &self,
    work_group: &WorkGroup,
    source_message: &ConversationMessage,
    members: &[AgentProfile],
) -> Result<WorkflowPlan>
```

```rust
pub fn dispatch_owner_workflow<R: Runtime>(
    &self,
    app: AppHandle<R>,
    plan: WorkflowPlan,
) -> Result<()>
```

```rust
pub fn dispatch_direct_agent_task<R: Runtime>(
    &self,
    app: AppHandle<R>,
    source_message: &ConversationMessage,
    target_agent_ids: &[String],
) -> Result<Vec<TaskCard>>
```

```rust
pub fn raise_task_blocker<R: Runtime>(
    &self,
    app: AppHandle<R>,
    task_id: &str,
    blocker: RaiseTaskBlockerInput,
) -> Result<TaskBlocker>
```

```rust
pub fn resolve_owner_blocker<R: Runtime>(
    &self,
    app: AppHandle<R>,
    blocker_id: &str,
    resolution: OwnerBlockerResolution,
) -> Result<()>
```

---

## 十、`send_human_message` 的新总流程

## 10.1 路由分流
`send_human_message` 新逻辑固定如下：

1. 写入 human message
2. 解析 mentions
3. 调用 `classify_request_route_mode`
4. 按模式分流

### `direct_answer`
- 直接回答
- 不建 task

### `direct_agent_assign`
- 直接给目标 agent 建 task
- 不触发群主规划

### `owner_orchestrated`
- 群主先规划 workflow
- 再按 stage 推进

---

## 10.2 owner_orchestrated 下的推进逻辑
1. 群主确认
2. 群主发布阶段计划
3. 启动第一个 stage
4. 当前 stage 内按 `serial / parallel` 派任务
5. agent 回执
6. agent 完成或阻塞
7. 群主根据当前 stage 结果推进
8. 全部结束后群主汇总

---

## 10.3 direct_agent_assign 下的推进逻辑
1. 用户直接点名 agent
2. 系统创建 direct task
3. agent 接单
4. agent 执行
5. agent 向用户汇报
6. 若阻塞，agent 直接 `@用户`

---

## 十一、前端展示规则

## 11.1 主聊天区渲染原则
主聊天区要按“群协作叙事”渲染，而不是按原始 message kind 生硬显示。

### 群主消息样式
- `owner_ack`
- `owner_plan`
- `owner_dispatch`
- `owner_stage_transition`
- `owner_summary`

统一用“群主”身份徽标。

### agent 消息样式
- `agent_ack`
- `agent_progress`
- `agent_delivery`

统一显示 agent 名称和所属任务。

### blocker 消息样式
- `blocker_raised`
- `blocker_resolved`

统一用高亮警示卡片。

---

## 11.2 阶段卡片
对于 `owner_plan` 消息，前端渲染成阶段清单卡片：

每个阶段展示：

- 阶段名
- 阶段目标
- 串行/并行
- 当前状态
- 涉及 agent

---

## 11.3 任务进度标签
每条派单或交付消息旁展示：

- 所属阶段
- 所属任务
- 当前状态
- 可选进度百分比

---

## 11.4 用户直派消息展示
用户直派模式下，主聊天不显示“群主规划卡”。

显示形式是：

- 用户消息
- agent 接单
- agent 进展
- agent 最终结果

视觉上要明显比群主编排模式更短、更直接。

---

## 十二、执行详情规则

## 12.1 继续只显示底层执行细节
执行详情仍显示：

- tool call
- tool result
- stream summary
- backstage
- tool run state
- approval

---

## 12.2 新增筛选
执行详情必须支持：

- 按 workflow
- 按 stage
- 按 task
- 按 agent

---

## 十三、测试方案

## 13.1 群主编排模式
1. `帮我开发一个图书管理系统`
   - 出现群主确认
   - 出现阶段计划
   - 先派 PRD，再派架构，再派研发，再派测试，再派上线
2. Stage 1 未完成前，Stage 2 不启动
3. 开发阶段的多个开发 agent 并行启动
4. 每个 agent 的回执和交付都进入主聊天
5. 群主最终输出汇总

---

## 13.2 用户直派模式
1. `@前端开发1 做登录页`
   - 不出现群主确认
   - 直接由前端开发1回执
2. `@CodeAuditor 审计鉴权逻辑`
   - 不出现群主派单
   - 直接执行并回报
3. `@多个 agent` 但职责不清晰
   - 系统要求用户澄清
   - 不偷偷转交群主

---

## 13.3 阻塞升级
1. 群主编排任务遇阻塞
   - agent 先 @群主
   - 群主协调
   - 协调后恢复任务
2. 用户直派任务遇阻塞
   - agent 直接 @用户
3. 群主需要用户决策时
   - 由群主统一 @用户
   - 不是多个 agent 同时问用户

---

## 13.4 前端渲染
1. 主聊天能清楚看出当前阶段和负责人
2. 阶段切换消息正确显示
3. 并行任务的多个 agent 进展独立可见
4. 用户直派模式不会出现群主编排卡片

---

## 十四、实施顺序

## Phase 1：模型重构
- 新增 `Workflow / Stage / Blocker / MessageMeta`
- 扩展 `TaskCard`
- 拆分超长文件
- 特别是：
  - `/Users/a1024/code/NextChat/src-tauri/src/core/service.rs`
  - `/Users/a1024/code/NextChat/src-tauri/src/core/service/runtime.rs`
  - `/Users/a1024/code/NextChat/src/components/dashboard/ChatManagement.tsx`

## Phase 2：请求分流
- 实现 `direct_answer`
- 实现 `direct_agent_assign`
- 实现 `owner_orchestrated`

## Phase 3：群主编排工作流
- Workflow/Stage 落库
- 群主阶段计划生成
- 串并调度
- 阶段 gate

## Phase 4：用户直派
- 直接点名路由
- direct task 创建
- 直接回执与直接阻塞

## Phase 5：叙事层 UI
- 主聊天 narrative 渲染
- 阶段卡片
- blocker 卡片
- 任务状态标签

## Phase 6：执行详情增强
- workflow/stage/task/agent 筛选
- 和主聊天语义对齐

---

## 十五、已锁定决策与默认值

1. 项目型任务默认由群主编排
2. 用户显式 `@agent` 时，默认绕过群主
3. 群主编排模式下，阻塞默认先找群主
4. 用户直派模式下，阻塞默认直接找用户
5. 串并行任务必须用 `Workflow -> Stage -> Task` 三层建模
6. 阶段之间默认串行
7. 阶段内是否并行，由群主规划显式决定
8. 主聊天区展示叙事，执行详情展示底层运行细节
9. 图书管理系统这类应用开发任务，默认采用五阶段模板
10. 实施前必须先拆分超长文件，遵守 `/Users/a1024/code/NextChat/AGENTS.md` 约束

