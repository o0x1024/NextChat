# LLM 工作流断点续跑实现清单

## 1. 目标

为多阶段协作型 LLM 工作流增加“断点记录、失败恢复、降级续跑”能力，避免一次 `500` 或单个 agent 失败导致整条链路中断，尤其覆盖以下场景：

- greenfield 项目在空目录中应继续初始化并产出代码
- 中间阶段失败后应从最近检查点继续
- 非关键阶段失败后应允许跳阶段或切换执行者

## 2. 交付范围

本实现清单覆盖：

- 检查点数据模型
- 检查点写入时机
- 恢复状态机
- 500 错误处理
- assignee 切换和阶段跳过
- greenfield 快速路径
- 可观测性与验收

不覆盖：

- 模型供应商侧 500 的根本修复
- 具体游戏项目代码生成逻辑

## 3. 里程碑拆分

### M1 数据落盘

- 定义 checkpoint 数据结构
- 在 workflow/stage/task/action 关键点写入 checkpoint
- 提供最近检查点查询能力

### M2 自动恢复

- 对 500 增加自动重试
- 从最近检查点恢复上下文
- 控制恢复时的上下文裁剪

### M3 降级续跑

- 连续失败后切换 assignee
- 对非关键阶段支持跳阶段
- 空目录场景支持 greenfield 初始化

### M4 可观测与验收

- 增加恢复链路日志和指标
- 增加集成测试和故障注入

## 4. 实现清单

## 4.1 数据模型

### 必做

- 新增 `WorkflowCheckpoint` 结构体或等价数据模型。
- 字段至少包括：
  - `checkpoint_id`
  - `workflow_id`
  - `stage_id`
  - `stage_title`
  - `task_id`
  - `task_title`
  - `assignee_agent_id`
  - `assignee_name`
  - `status`
  - `working_directory`
  - `repo_snapshot`
  - `artifact_summary`
  - `todo_snapshot`
  - `resume_hint`
  - `failure_count`
  - `created_at`
  - `updated_at`
- `repo_snapshot` 至少保存：
  - `entry_count`
  - `is_empty`
  - `top_level_entries`
- `artifact_summary` 保存最近成功产出的摘要，而不是整段原文。
- `todo_snapshot` 保存 task 内 todos 的状态快照。

### 建议

- 增加 `last_tool_name`、`last_tool_result` 字段，方便恢复时判断是否重复执行工具。
- 增加 `degraded_from` 字段，记录是否由重试降级为切换 assignee 或跳阶段。

## 4.2 存储层

### 必做

- 选择稳定存储：
  - 若当前系统已有 SQLite：优先增加 `workflow_checkpoints` 表
  - 若当前系统以文件为主：按 workflow 落 JSON 文件
- 保证按 `workflow_id + stage_id + task_id` 可以查询最近检查点。
- 支持“写新版本，不覆盖旧版本”，保留恢复历史。

### 表结构建议

```sql
CREATE TABLE workflow_checkpoints (
  checkpoint_id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL,
  stage_id TEXT,
  stage_title TEXT,
  task_id TEXT,
  task_title TEXT,
  assignee_agent_id TEXT,
  assignee_name TEXT,
  status TEXT NOT NULL,
  working_directory TEXT,
  repo_snapshot_json TEXT NOT NULL,
  artifact_summary_json TEXT NOT NULL,
  todo_snapshot_json TEXT NOT NULL,
  resume_hint TEXT,
  failure_count INTEGER NOT NULL DEFAULT 0,
  degraded_from TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

## 4.3 写入时机

### 必做

- 在以下时机强制写 checkpoint：
  - workflow 创建后
  - stage 切换前
  - stage 切换后
  - task 开始前
  - task 完成后
  - 关键工具调用前
  - 关键工具调用后
  - 捕获异常后
- “关键工具”至少包括：
  - 会写文件的工具
  - 会改变状态的工具
  - 会触发下游分支决策的工具

### 防重复要求

- 同一 task 中若 3 秒内重复写入内容完全相同的 checkpoint，应去重或节流。

## 4.4 恢复状态机

### 必做

- 新增恢复状态机，建议状态包括：
  - `ready`
  - `running`
  - `failed_retryable`
  - `retrying`
  - `degraded_reassign`
  - `degraded_skip_stage`
  - `completed`
  - `failed_terminal`

### 状态转移规则

- `running -> failed_retryable`
  - 条件：出现 5xx、超时、上游短暂不可用
- `failed_retryable -> retrying`
  - 条件：失败次数小于阈值
- `failed_retryable -> degraded_reassign`
  - 条件：连续重试超过阈值，且当前 stage 允许换人
- `failed_retryable -> degraded_skip_stage`
  - 条件：stage 非关键，且已有足够摘要可供后续使用
- `retrying -> running`
  - 条件：重建上下文完成
- `retrying -> failed_terminal`
  - 条件：超过总失败次数阈值且无法降级

## 4.5 500 错误处理

### 必做

- 对 `500 InternalServiceError` 分类为“可恢复错误”。
- 第一次 500：
  - 记录 checkpoint
  - 增加 `failure_count`
  - 做 1 次短退避重试
- 第二次连续 500：
  - 再次记录 checkpoint
  - 启动降级判断
- 不允许在没有 checkpoint 的情况下直接重试整个 workflow。

### 退避策略建议

- 第 1 次：2 秒
- 第 2 次：5 秒
- 第 3 次：10 秒

上限 3 次，之后进入降级逻辑。

## 4.6 上下文恢复

### 必做

- 恢复时不要重新注入整个长日志。
- 恢复输入应只包含：
  - 当前 workflow 目标
  - 最近 stage/task 信息
  - 最近成功产出摘要
  - repo snapshot
  - todo snapshot
  - resume hint
- 对超过阈值的历史消息做摘要压缩。

### 恢复 prompt 要求

- 明确告诉 agent：
  - 当前是恢复执行，不是重做全流程
  - 已完成内容不要重复输出
  - 优先完成下一个未完成动作

## 4.7 Assignee 切换

### 必做

- 增加 `can_reassign(stage_id, task_id)` 规则。
- 定义切换顺序：
  - 架构类任务失败，可切到全栈
  - 测试类任务失败，可切到全栈或保留待后置
- 切换时必须写 checkpoint，并记录：
  - 原 assignee
  - 新 assignee
  - 切换原因

### 针对本事件的专项规则

- 若“技术方案与架构设计”阶段失败，且：
  - 用户需求已明确
  - 工作目录为空
  - 已有技术可行性摘要

则允许直接转交全栈开发专家，执行 greenfield MVP 初始化与实现。

## 4.8 阶段跳过

### 必做

- 增加 `is_stage_skippable(stage)` 判断。
- 以下条件同时满足时允许跳过 stage：
  - 当前 stage 不是硬前置
  - 已有足够摘要供下游执行
  - 下游执行者具备直接落地能力

### 推荐规则

- 可跳过：
  - 过细的架构文档补全
  - 重复的需求访谈
- 不可跳过：
  - 真正依赖产物的构建阶段
  - 必须产出代码文件的阶段

## 4.9 Greenfield 快速路径

### 必做

- 新增 `is_greenfield_request` 判断：
  - 工作目录为空
  - 用户请求是“新建/开发/实现”
  - 无需兼容现有代码
- 若命中 greenfield：
  - 不要把“空目录”解释为阻塞
  - 直接进入初始化模式

### 初始化动作

- 创建最小项目结构
- 写入首批必要文件
- 确保第一轮就至少生成一个可运行入口

### 强制规则

- 对 greenfield 请求，若连续两个阶段都没有 `Write/Edit` 类动作，则强制转实现模式。

## 4.10 幂等与防重复

### 必做

- 恢复时避免重复执行已成功的文件写入。
- 对关键动作生成 `action_fingerprint`。
- 若 fingerprint 已成功落盘，则恢复时跳过该动作。

### 适用动作

- 写文件
- 发送 agent dispatch
- 创建 workflow
- 变更 stage 状态

## 4.11 可观测性

### 必做

- 增加以下日志字段：
  - `workflow_id`
  - `stage_id`
  - `task_id`
  - `checkpoint_id`
  - `resume_mode`
  - `failure_count`
  - `degraded_from`
- 增加以下指标：
  - `checkpoint_write_total`
  - `checkpoint_resume_total`
  - `resume_success_total`
  - `resume_failure_total`
  - `stage_reassign_total`
  - `stage_skip_total`
  - `greenfield_fastpath_total`

### 告警建议

- 连续 2 个 stage 无代码写入
- 同一 workflow 创建超过 1 次
- 同一 human directive 被重复注入超过阈值

## 4.12 UI/产品表现

### 必做

- 在任务详情里展示：
  - 最近检查点时间
  - 最近失败原因
  - 当前恢复策略
- 对用户可见的状态文案要区分：
  - `正在重试`
  - `正在从断点恢复`
  - `已降级为直接实现`

## 5. 验收清单

## 5.1 功能验收

- 可以成功写入并查询最近检查点。
- 单次 500 后系统能自动重试当前 task。
- 连续 500 后系统能切换 assignee 或跳阶段。
- 空目录项目在恢复后能直接创建文件并进入实现。
- 恢复执行不会重复创建同一个 workflow。

## 5.2 回归验收

- 非失败路径下不影响现有 workflow 执行。
- checkpoint 写入不会显著拖慢正常链路。
- 恢复逻辑不会导致重复 dispatch。

## 5.3 本事件专项验收

模拟以下场景并通过：

1. 用户请求：`开发一个不一样的贪吃蛇游戏，仅前端即可，不需要后端`
2. 工作目录为空
3. 系统先生成创意和方案摘要
4. 架构阶段注入一次模拟 500
5. 系统从最近 checkpoint 恢复
6. 系统降级为全栈直接实现
7. 工作目录中产出代码文件

验收标准：

- 恢复后不再重复完整访谈
- 至少生成一个入口文件和一个游戏核心文件
- 日志中能看到 checkpoint、retry、reassign 或 skip-stage 记录

## 6. 测试用例清单

### 单元测试

- checkpoint 序列化/反序列化
- 最近 checkpoint 查询
- failure_count 累加
- stage 可跳过判断
- greenfield 判断

### 集成测试

- task 内工具调用后写 checkpoint
- 500 后重试
- 500 后 assignee 切换
- 500 后阶段跳过
- 恢复时上下文裁剪正确

### 故障注入测试

- 首次模型调用 500，第二次成功
- 连续三次 500，触发降级
- checkpoint 写入失败
- 恢复时 repo 状态已变化

## 7. 推荐开发顺序

1. 先做 checkpoint 数据模型和存储。
2. 再做写入时机和查询接口。
3. 再做 500 自动重试。
4. 再做 assignee 切换和阶段跳过。
5. 最后接入 greenfield 快速路径和监控指标。

## 8. 最小上线版本

如果需要先快速上线一个 MVP，建议只做以下能力：

- checkpoint 落盘
- 单 task 自动重试
- 连续失败切换 assignee
- greenfield 空目录直接初始化

先不要做：

- 复杂 UI 展示
- 全量恢复分析面板
- 高级策略编排

## 9. 实施完成定义

满足以下条件即视为完成：

- 工作流任一 stage 出现 500 时，系统可从最近 checkpoint 自动恢复
- 连续失败时，系统可降级到可执行的下一个路径
- 对空目录新项目请求，不会再出现“只产分析文档、不产代码”的情况
- 本次“贪吃蛇”场景的故障注入测试通过
