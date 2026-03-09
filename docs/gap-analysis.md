# 当前可用性差距清单（2026-03-09）

以下清单不是长期愿景，而是从“当前代码状态”到“用户实际可用的多 Agent MVP”之间必须补齐的缺口。

## 执行总表

| ID | 优先级 | 主题 | 当前状态 | 依赖 | 验收方式 |
| --- | --- | --- | --- | --- | --- |
| GA-01 | P0 | 修复编译和启动断点 | 已完成 | 无 | `cargo check`、`npm run build`、`tauri dev` |
| GA-02 | P0 | 单群组最小闭环验收 | 已完成 | GA-01 | 手工 checklist 或自动化 smoke |
| GA-03 | P0 | 单 Agent 执行能力收口 | 已完成 | GA-01 | provider 限制、降级提示、真实执行标识 |
| GA-04 | P1 | 独立权限模型 | 已完成 | GA-01, GA-03 | 授权拒绝测试、审计记录、UI 提示 |
| GA-05 | P1 | skill 驱动工具裁剪 | 已完成 | GA-04 | skill 变更前后工具集合对比 |
| GA-06 | P1 | memory 策略闭环 | 已完成 | GA-03 | memory 注入可见、读写规则生效 |
| GA-07 | P2 | `@Agent` 真实交互 | 已完成 | GA-02 | mention picker、调度结果一致 |
| GA-08 | P2 | 子任务图和依赖收口 | 已完成 | GA-02, GA-06 | 多子任务并行与父任务状态回流 |
| GA-09 | P2 | agent 间显式协作消息 | 已完成 | GA-07, GA-08 | 协作请求、回传结果、可追溯 |
| GA-10 | P3 | 输入区占位功能清理 | 已完成 | GA-07 | 无误导按钮、核心按钮真实可用 |
| GA-11 | P3 | 调度可解释性增强 | 已完成 | GA-02, GA-08 | UI 展示得分因子和状态原因 |

## 第一批建议

如果按最短路径把系统推到“可以对外演示”的程度，建议先只做下面 5 项：

1. `GA-01` 修复编译和启动断点。
2. `GA-02` 建立最小闭环验收。
3. `GA-03` 收口 provider 和降级行为。
4. `GA-04` 建立最小权限模型。
5. `GA-07` 把 `@Agent` 从文本约定做成真实交互。

这 5 项完成后，系统仍不是完整多 Agent 平台，但已经能避免“看起来能用，实际上不能用”的主要问题。

## P0：先达到可启动、可运行、可验证

### P0-1 修复编译和启动断点
- 现状
  - Rust 后端 `cargo check` 失败，`Storage::dashboard_state()` 调用了不存在的 `get_settings()`。
  - 前端仍调用未注册的 `delete_agent_profile`。
- 任务
  - 补齐 `settings` 读写接口，或移除未完成字段，确保 `src-tauri` 编译通过。
  - 为删除 agent 提供真实 command + service + storage 链路，或暂时移除 UI 入口。
  - 统一检查前后端类型定义，避免 `DashboardState`、settings、commands 再次漂移。
- 完成标准
  - `cargo check` 通过。
  - `npm run build` 通过。
  - `tauri dev` 能正常启动并进入主界面。

### P0-2 跑通最小闭环
- 现状
  - 代码已有 `human message -> task card -> bids -> lease -> run_task -> summary` 主链路，但没有最小闭环验收。
- 任务
  - 建立一个最小验收脚本或手工 checklist。
  - 验证单群组内至少 2 个 agent 时，消息发出后能稳定进入任务状态机。
  - 验证 summary、tool run、task status、lease 状态在 UI 中一致。
- 完成标准
  - 能稳定完成以下流程：
    - 创建 2 个 agent。
    - 创建 1 个 work group 并加入成员。
    - human 发消息。
    - 系统生成 task。
    - 至少 1 个 agent 认领并回 summary。
    - task 进入 `completed` 或 `waiting_approval`。

### P0-3 做实单 Agent 执行能力
- 现状
  - provider UI 需要和运行时支持范围保持一致，避免未配置 provider 继续伪装成可用。
  - 没有可用模型时，会退回模板式 summary。
- 任务
  - 明确首版只支持哪些 provider，并在 UI 中做真实限制。
  - 为无 API key、模型调用失败、工具调用失败提供清晰退化提示。
  - 将“模板 fallback”与“真实 LLM 执行”在 UI 上区分，避免误导用户。
- 完成标准
  - 用户能明确知道某个 agent 当前是“真实模型执行”还是“降级执行”。
  - 未接通的 provider 不能在 UI 中伪装成可用。
- 本轮已完成
  - agent 创建/编辑弹窗现在只允许选择“运行时支持 + 已启用 + 已配置 API key + 有模型列表”的 provider。
  - 不可用 provider 会显示明确原因，例如禁用、缺少 API key、运行时未支持、没有模型。
  - 未满足条件时，创建/更新按钮会被禁用，避免继续制造“看起来可配、实际不可执行”的 agent。
  - agent 执行后写入 `executionMode`，主聊天消息会直接标记“真实模型就绪 / 降级执行”，避免用户误判本次结果来自真实 LLM。

## P1：让 tools、permissions、skills 从字段变成真实约束

### P1-1 独立权限模型
- 现状
  - 现在只有 tool manifest 的 `permissions` 字段和高风险审批，没有独立 agent 权限配置与校验。
- 任务
  - 设计 agent 级权限策略，例如：
    - `allow_tool_ids`
    - `deny_tool_ids`
    - `require_approval_tool_ids`
    - `allow_fs_roots`
    - `allow_network_domains`
  - 在工具执行前做统一授权检查，而不是只看风险等级。
  - 将权限拒绝写入审计与 UI。
- 完成标准
  - 同一个工具对不同 agent 可呈现不同授权结果。
  - agent 无权限时不能仅靠 prompt 绕过实际执行边界。
- 本轮已完成
  - `AgentProfile` 已新增独立权限策略：`allow_tool_ids`、`deny_tool_ids`、`require_approval_tool_ids`、`allow_fs_roots`、`allow_network_domains`。
  - 工具暴露阶段会先按 agent 绑定和权限策略过滤，避免把被拒绝的工具继续暴露给运行时。
  - 工具执行前会统一做硬校验；文件路径与网络域名也会落到真实输入级别校验，而不是只靠 prompt 约束。
  - 权限拒绝会写入 `tool_run.permission_denied` 审计事件，并在主时间线写入系统提示，任务进入 `needs_review`。
  - 已补测试覆盖：
    - `core::tool_runtime::tests::file_tool_respects_agent_fs_roots`
    - `core::service::tests::permission_denial_moves_task_to_review_and_records_audit`

### P1-2 让 skill 真正影响可用工具与行为
- 现状
  - `SkillPack.allowed_tool_tags` 只定义了，没有进入运行时裁剪逻辑。
  - skill 当前主要影响 prompt 文本。
- 任务
  - 将 `allowed_tool_tags` 接到 `available_tools` 过滤逻辑。
  - 定义冲突策略：agent 显式 tool 绑定与 skill allowlist 如何求交集。
  - 为 skill 加上可见的行为说明，帮助用户理解为什么某个工具没暴露。
- 完成标准
  - skill 变化会真实改变 agent 执行时看到的工具集合。
  - UI 能解释工具是因 agent 绑定缺失还是因 skill 限制而不可用。
- 本轮已完成
  - `allowed_tool_tags` 已接入运行时工具暴露逻辑，agent 真实可见工具集合现在取 `tool 绑定 ∩ permission policy ∩ skill categories`。
  - 调度入口选工具与 claim 评分已改为基于有效工具集合，避免 skill 已裁掉的工具继续参与竞价加分。
  - 工具执行前新增 skill 类别硬校验，不能再通过 prompt 或直接调用绕过 skill 裁剪。
  - Agent 工作区现在逐工具展示运行时状态，可区分 `未绑定 / 被权限阻止 / 被技能裁剪 / 可用`。
  - 已补测试覆盖：
    - `core::skill_policy::tests::selected_skills_filter_visible_tools`
    - `core::skill_policy::tests::blocked_tool_reports_skill_reason`
    - `core::tool_runtime::tests::skill_allowlist_blocks_network_tool_authorization`

### P1-3 记忆策略闭环
- 现状
  - `memoryPolicy`、memory items 已有基础字段与写入，但缺少读取、过滤、召回策略。
- 任务
  - 明确 `user / work_group / agent` 三层读写规则。
  - 在任务执行前按策略组装 memory context。
  - 加入 pinned、ttl、过期清理和最小可视化入口。
- 完成标准
  - agent 执行时能读取符合策略的 memory。
  - 用户能看到某次执行实际注入了哪些 memory。
- 本轮已完成
  - `memoryPolicy` 已从前端创建/编辑表单贯通到后端输入与持久化，不再在 service 层被默认值覆盖。
  - 运行时新增独立 memory 策略模块，按 `read_scope / pinned_memory_ids / ttl` 组装执行上下文，并在启动与执行前清理过期 memory。
  - 每次任务执行前都会生成一条 `task` scope 的 memory 注入快照，Task 视图可直接看到本次实际注入了哪些 memory。
  - 执行完成后的摘要写回现在受 `write_scope` 约束，可分别写入 `user / work_group / agent`，而不是无条件写入 agent scope。
  - 已补测试覆盖：
    - `core::memory::tests::pinned_memory_can_override_read_scope`
    - `core::memory::tests::ttl_marks_expired_memory`
    - `core::memory::tests::write_scope_respects_policy`
    - `core::service::tests::memory_policy_injects_snapshot_and_respects_write_scope`

## P2：把“多 Agent 协作”从启发式升级为真实编排

### P2-1 做实 `@Agent` 交互
- 现状
  - 后端能解析手输 `@AgentName`，但主聊天页没有真正的 mention picker/插入交互。
- 任务
  - 在当前实际使用的聊天组件中补 mention 选择器。
  - 明确 mention 的语义：
    - 提高 claim score
    - 指定协作者
    - 仅做通知
  - 为 mention 失败场景提供反馈，例如 agent 不在群组中、名字冲突。
- 完成标准
  - 用户无需手写精确名称，也能稳定 `@` 到正确 agent。
  - mention 行为在 UI 和调度逻辑中保持一致。
- 本轮已完成
  - 当前实际使用的聊天页已接入真实 mention picker，输入 `@` 或点击输入区 `@` 按钮都会弹出当前工作组成员候选列表。
  - mention picker 支持键盘上下选择、`Tab`/`Enter` 插入，顶部成员条也可直接点击插入 `@AgentName`。
  - 发送前会校验 mention 是否命中当前工作组成员，并对“成员不存在 / 同名冲突”给出明确提示，避免前端插入和后端调度结果不一致。
  - 现有后端 `extract_mentions` 链路保持不变，前端现在插入的是可被调度层稳定识别的精确 agent 名称。

### P2-2 从“单个建议子任务”升级为子任务图
- 现状
  - 当前执行流程最多只会派生少量启发式子任务，且更接近 coordinator 代发任务。
- 任务
  - 支持多个子任务。
  - 支持父任务等待全部依赖完成。
  - 支持 child task 的 `completed / cancelled / needs_review` 回流影响父任务。
  - 支持 agent 显式发起委派，而不只是 runtime 自动猜测。
- 完成标准
  - 一个父任务可以派给多个 agent 并行执行。
  - 父任务状态对所有子任务结果有稳定、可解释的收口逻辑。
- 本轮已完成
  - 子任务派发已从“最多 1 个”放开到多子任务并行执行，父任务会稳定进入 `waiting_children` 直到全部 child task 进入终态。
  - Agent runtime 现在支持显式委派协议：模型可在摘要末尾输出 `Delegate @Agent: task details`，运行时会解析成 child task，同时自动从用户可见 summary 中剥离该控制语句。
  - 旧的 mention / `parallel` 启发式仍然保留，但只作为显式委派缺失时的回退，不再是唯一来源。
  - 父任务回流逻辑已覆盖 `completed / cancelled / needs_review` 三类 child 结果；全部完成时父任务收敛为 `completed`，只要任一 child 出问题则父任务进入 `needs_review`。
  - Task 详情页现在会展示 child task 列表和状态分布，更适合查看多子任务收口结果。
  - 已补测试覆盖：
    - `core::agent_runtime::tests::explicit_delegation_directives_are_extracted`
    - `core::agent_runtime::tests::delegation_directives_are_removed_from_summary`
    - `core::service::tests::parent_task_waits_for_all_children_before_completion`
    - `core::service::tests::child_issue_bubbles_parent_to_review`

### P2-3 支持 agent 与 agent 的显式协作消息
- 现状
  - 现在更像“一个 agent 完成后触发下一个 child task”，不是 agent 间显式往返协作。
- 任务
  - 支持 agent 在 backstage 中显式发协作请求消息。
  - 支持协作消息与 task / child task 建立关联。
  - 支持协作者返回结构化结果给父任务 owner。
- 完成标准
  - 用户能看到 agent A 请求 agent B 协助，agent B 回传结果，父任务再继续推进。
  - 协作过程能追溯到对应 task 和 tool run。
- 本轮已完成
  - child task 派发时会在 backstage 写入显式 `collaboration` 请求消息，请求方为父任务 owner，消息上直接绑定 child task，并包含父任务、child task、协作者和目标内容。
  - child task 完成后会由协作者回传结构化 `collaboration` 结果消息给父任务 owner；`completed / needs_review / cancelled` 三类收口路径都会补发结果，避免协作只停留在隐式状态迁移。
  - 前端聊天页现在会直接标记协作消息，并显示其关联 task，用户可以从同一时间线追溯到对应 child task、工具执行痕迹和父任务收口。
  - 已补测试覆盖：
    - `core::service::tests::child_task_emits_collaboration_request_and_result_messages`

## P3：补产品体验与可解释性

### P3-1 补聊天输入区占位功能
- 现状
  - 当前输入区的多个图标仍是视觉占位，没有真实行为。
- 任务
  - 区分首版必须可用功能与后续功能。
  - 将无实现功能隐藏、置灰或标注实验态。
  - 优先补 `@agent`、工具提示、审批入口、任务卡跳转。
- 完成标准
  - 主输入区不存在误导性“可点击但无行为”的核心按钮。
- 本轮已完成
  - 输入区底部工具栏已从纯占位图标收敛为真实可用入口，只保留 `@Agent`、审批队列、任务看板三类可执行动作。
  - 原本未实现的附件、设置、成员、记忆、表格、数据库等占位按钮已移除，避免继续制造“可点击但无行为”的错觉。
  - 输入区增加了明确说明：工具由运行时根据 agent 暴露能力自动选择，手动 picker 暂不提供伪入口。
  - 关联 task 现在支持真实跳转：消息中的 task badge、任务看板卡片和审批卡片都可以打开运行面板并滚动定位到对应 task。

### P3-2 强化调度可解释性
- 现状
  - backstage 有 bid 数据，但用户不容易理解为何某个 agent 赢得 lease。
- 任务
  - 将得分因素结构化展示出来，例如：
    - mention 加分
    - 工具覆盖加分
    - 并发负载扣分
    - 角色匹配加分
  - 为审批、抢占、恢复等状态变化增加简短理由。
- 完成标准
  - 用户能直接看出“为什么是这个 agent 在执行”和“为什么它被暂停或需要审批”。
- 本轮已完成
  - `ClaimBid` 已新增结构化 `scoreBreakdown`，调度侧会把 `基础分 / mention / 工具覆盖 / 并发余量 / 角色匹配 / skill 匹配 / 负载扣分` 等因子拆开持久化，而不再只剩一条自由文本 rationale。
  - Task 详情页现在会直接标记 winning bid，并逐项展示各个得分因子和分值，能明确看出 lease 为什么落到某个 agent。
  - Task 头部新增状态原因说明，会按 `waiting_children / waiting_approval / paused / needs_review / completed` 等状态给出当前收口原因，而不是只显示状态枚举。
  - 已补测试覆盖：
    - `core::coordinator::tests::bid_breakdown_exposes_mention_and_tool_factors`

## 推荐执行顺序

1. 先完成 `P0-1` 和 `P0-2`，确保系统真实可启动、可验证。
2. 再完成 `P0-3` 和 `P1-1`，把“能跑”升级为“不会误导用户”。
3. 再补 `P1-2` 和 `P1-3`，让 tools / skills / memory 真正进入运行时约束。
4. 最后推进 `P2`，把多 Agent 从骨架升级成可协作产品。

## 验收清单模板

后续每完成一个阶段，建议至少回归下面这些检查：

1. 构建检查
   - `cargo check`
   - `npm run build`
2. 启动检查
   - `tauri dev`
   - 主界面可加载
3. 数据流检查
   - 创建 agent
   - 创建 work group
   - 加入成员
   - 发 human 消息
   - 生成 task / bid / lease
   - agent summary 回写
4. 审批检查
   - 高风险工具进入待审批
   - 批准后恢复执行
   - 拒绝后进入 `needs_review`
5. 协作检查
   - `@Agent` 能正确定位成员
   - child task 状态能回流父任务

## 建议拆分方式

为了避免一次改动太大，推荐按下面的开发批次推进：

### 批次 A：系统可启动
- 覆盖 `GA-01`
- 输出物
  - 编译通过
  - 所有命令接口存在且前后端一致

### 批次 B：系统可验证
- 覆盖 `GA-02`、`GA-03`
- 输出物
  - 最小闭环 checklist
  - provider 和降级策略收口

### 批次 C：系统可控
- 覆盖 `GA-04`、`GA-05`、`GA-06`
- 输出物
  - 权限、skill、memory 的真实运行时约束

### 批次 D：系统可协作
- 覆盖 `GA-07`、`GA-08`、`GA-09`
- 输出物
  - 可解释的 `@Agent`
  - 多子任务协作
  - agent 间显式协作消息

## 当前判定

- 现在的状态适合描述为：`多 Agent 桌面协作系统骨架 + 单轮调度最小闭环 + 部分审批/审计/恢复能力`。
- 现在还不适合描述为：`已实际打通、可稳定使用的多 Agent 协作产品`。
