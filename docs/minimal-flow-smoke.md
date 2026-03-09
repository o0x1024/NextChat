# 当前 MVP Smoke Checklist

用于回归验证当前仓库已经完成的 `GA-01 .. GA-11` 能力是否仍然可用。

## 自动化验证

### 1. 全量回归

```bash
cd /Users/like/code/NextChat
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

期望结果：
- 前端构建通过。
- Rust 测试全部通过。

### 2. 关键链路定点测试

```bash
cd /Users/like/code/NextChat
cargo test --manifest-path src-tauri/Cargo.toml minimal_group_flow_creates_task_lease_and_agent_summary -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml memory_policy_injects_snapshot_and_respects_write_scope -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml child_task_emits_collaboration_request_and_result_messages -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml bid_breakdown_exposes_mention_and_tool_factors -- --nocapture
```

期望结果：
- 单任务最小闭环通过。
- memory 注入与写回策略通过。
- agent 间显式协作消息通过。
- 调度得分拆解可见且测试通过。

## 手工验证

### 1. 启动检查

运行：

```bash
cd /Users/like/code/NextChat
cargo check --manifest-path src-tauri/Cargo.toml
npm run build
```

期望结果：
- Rust 后端构建通过。
- 前端构建通过。

### 2. 基础配置检查

- 启动应用。
- 在 Agent 管理页创建 3 个 agent：
  - `Planner`：绑定 `plan.summarize`
  - `Builder`：绑定 `file.readwrite` 或其他低风险工具
  - `Reviewer`：绑定 `plan.summarize`
- 给 `Builder` 配一个受限权限策略，例如只允许特定 `allowFsRoots`。
- 给 `Planner` 或 `Reviewer` 配一组有限 skill。
- 创建 1 个新的 work group，并把 3 个 agent 加入该工作组。

期望结果：
- Agent 创建成功。
- provider 只显示真实可用配置。
- Agent 页面能直接看到 permission / skill / memory 摘要。

### 3. 主链路检查

在该 work group 聊天窗口发送一条简单任务，例如：

```text
Please create a concise launch readiness plan.
```

期望结果：
- human 消息进入主时间线。
- 系统创建新的 task card。
- 至少产生 1 条 claim bid 和 1 条 lease。
- 任务完成后出现 agent summary。
- Task 详情中能看到 winning bid、得分因子和状态原因。

### 4. Mention 与协作检查

发送一条带 mention 的任务，例如：

```text
@Planner coordinate the release plan and ask @Reviewer to verify the checklist.
```

期望结果：
- 输入区 `@` picker 可正常插入成员。
- mention 不存在或命中重名时会收到明确错误提示。
- 若触发 child task，父任务进入 `waiting_children`。
- backstage 中能看到显式 `collaboration` 请求与结果回传。
- child task 终态会稳定回流父任务。

### 5. 审批与权限检查

发送一条会触发高风险或越权工具的任务，例如：

```text
@Builder run a shell command to inspect the workspace.
```

或：

```text
@Builder write blocked/spec.md and save content: hello
```

期望结果：
- 高风险工具进入审批队列。
- 输入区底部审批入口可以直接跳到审批卡片。
- 批准后任务继续执行。
- 拒绝或权限不足时，任务进入 `needs_review`，并写入审计。

### 6. Memory 检查

- 为某个 agent 配置 `readScope / writeScope / pinnedMemoryIds`。
- 执行一条新任务。
- 打开 Task 详情页查看 Memory 区块。

期望结果：
- 能看到本次执行实际注入的 `task` scope memory 快照。
- 摘要写回遵守 `writeScope`，而不是无条件写入。

### 7. 运行面板与输入区检查

- 打开聊天输入区底部操作条。
- 分别点击 `@Agent`、审批、任务看板。
- 在消息时间线中点击关联 task badge。

期望结果：
- 输入区不存在“可点但没行为”的核心按钮。
- 审批入口可以打开审批队列。
- 任务看板入口和消息里的 task badge 可以滚动定位到对应 task。

## 回归触发条件

以下改动后，应至少重新执行一次本清单：
- `service.rs`、`core/service/*` 中的任务分发、approval、subtask、collaboration 逻辑改动。
- `coordinator.rs` 中的 claim scoring 或 bid breakdown 逻辑改动。
- `storage.rs`、`storage/*` 中的 schema 或持久化字段改动。
- 聊天页、任务详情页、运行面板、审批入口相关 UI 改动。
