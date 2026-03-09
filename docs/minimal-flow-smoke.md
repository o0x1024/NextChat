# 最小闭环 Smoke Checklist

用于验证当前系统是否已经具备最基本的多 Agent 工作流闭环。

## 自动化验证

后端最小闭环 smoke test：

```bash
cd /Users/a1024/code/NextChat/src-tauri
cargo test minimal_group_flow_creates_task_lease_and_agent_summary -- --nocapture
```

期望结果：
- 测试通过。
- 流程覆盖 `create_agent_profile -> create_work_group -> add_agent_to_work_group -> send_human_message -> task/lease/summary`。

## 手工验证

### 1. 构建检查
- 运行：

```bash
cd /Users/a1024/code/NextChat/src-tauri
cargo check
cd /Users/a1024/code/NextChat
npm run build
```

- 期望结果：
  - Rust 后端构建通过。
  - 前端构建通过。

### 2. 基础配置检查
- 启动应用。
- 在 Agent 管理页创建 2 个 agent。
- 每个 agent 至少绑定 1 个低风险工具，建议 `plan.summarize`。
- 创建 1 个新的 work group。
- 将这 2 个 agent 加入该 work group。

- 期望结果：
  - Agent 创建成功。
  - Work group 创建成功。
  - 成员列表中能看到 2 个 agent。

### 3. 发送任务检查
- 在该 work group 聊天窗口发送一条简单任务，例如：

```text
Please create a concise launch readiness plan.
```

- 期望结果：
  - human 消息进入主时间线。
  - 系统创建新的 task card。
  - 产生至少 1 条 claim bid。
  - 产生 1 条 lease。

### 4. 执行结果检查
- 等待任务执行完成。

- 期望结果：
  - 至少 1 条 agent summary 出现在聊天中。
  - task 状态进入 `completed`，或在高风险工具场景进入 `waiting_approval`。
  - lease 最终进入 `released`。

### 5. Backstage 检查
- 打开 backstage 或运行面板。

- 期望结果：
  - 能看到 task、lease、claim bid 的对应关系。
  - 如果触发了工具运行，能看到 tool call / tool result。

## 回归触发条件

以下改动后，应至少重新执行一次本清单：
- `service.rs` 中的任务分发、lease、approval、subtask 逻辑改动。
- `coordinator.rs` 中的 claim scoring 逻辑改动。
- `tool_runtime.rs` 或 `llm_rig.rs` 改动。
- 前端事件订阅或聊天页状态刷新逻辑改动。
