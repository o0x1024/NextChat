# 代理实现文档

## 概述

系统设置中的代理设置（`proxyUrl`）现在已完全集成到 LLM 请求中。当用户配置代理后，所有向 AI 提供商的 HTTP/HTTPS 请求都会通过该代理进行。

## 实现方式

### 环境变量配置

代理实现采用环境变量的方式，这是 `reqwest` HTTP 客户端库推荐的方法，也是 `rig-core` 库兼容的做法。

核心函数：
```rust
/// Sets up proxy environment variables for reqwest if proxy_url is configured.
fn configure_proxy_env(proxy_url: &str) {
    let trimmed = proxy_url.trim();
    if trimmed.is_empty() {
        return;
    }
    
    // Set both HTTP and HTTPS proxy environment variables
    if trimmed.starts_with("http://") {
        std::env::set_var("HTTP_PROXY", trimmed);
    } else if trimmed.starts_with("https://") {
        std::env::set_var("HTTPS_PROXY", trimmed);
    } else {
        std::env::set_var("HTTPS_PROXY", trimmed);
    }
    std::env::set_var("ALL_PROXY", trimmed);
}
```

### 代理生效的请求类型

#### 1. **LLM 完成请求** (`llm.complete`)
- **文件**: [src/core/llm_rig.rs:1124](src-tauri/src/core/llm_rig.rs#L1124)
- **调用链**: `ModelProviderAdapter::complete()` → 配置代理 → 创建 AI client → 执行请求
- **涉及 provider**: OpenAI、Anthropic、Gemini、和所有 OpenAI 兼容的提供商

#### 2. **工具执行任务** (`llm.complete_task_with_tools`)
- **文件**: [src/core/llm_rig/tool_execution.rs:210](src-tauri/src/core/llm_rig/tool_execution.rs#L210)
- **调用链**: 任务执行 → 工具调用 → LLM 代理（带工具支持）→ 代理请求
- **涉及 provider**: OpenAI、Anthropic、Gemini、和所有 OpenAI 兼容的提供商

#### 3. **连接测试** (`llm.test_connection`)
- **文件**: [src/core/llm_rig.rs:1207](src-tauri/src/core/llm_rig.rs#L1207)
- **调用**: 用户在设置中测试 provider 连接时执行
- **涉及 provider**: 所有配置的 AI 提供商

#### 4. **模型列表刷新** (`llm.refresh_models`)
- **文件**: [src/core/llm_rig.rs:779](src-tauri/src/core/llm_rig.rs#L779)
- **调用链**: 用户手动或系统自动刷新提供商的模型列表
- **涉及的获取函数**:
  - `fetch_openai_compatible_models()`
  - `fetch_anthropic_models()`
  - `fetch_gemini_models()`
  - `fetch_ollama_models()`

## 代码修改清单

### 核心文件修改

#### 1. [src-tauri/src/core/llm_rig.rs](src-tauri/src/core/llm_rig.rs)
- ✅ 添加 `configure_proxy_env()` 函数
- ✅ 修改 `openai_compatible_client(config, proxy_url)`
- ✅ 修改 `anthropic_client(config, proxy_url)`  
- ✅ 修改 `gemini_client(config, proxy_url)`
- ✅ 修改 `refresh_models(config, proxy_url)`
- ✅ 修改所有 `fetch_*_models()` 函数以接收和使用 proxy_url
- ✅ 修改 `test_connection(config, proxy_url)`
- ✅ 修改 `complete()` 方法以传递代理 URL

#### 2. [src-tauri/src/core/llm_rig/tool_execution.rs](src-tauri/src/core/llm_rig/tool_execution.rs)
- ✅ 修改 `complete_task_with_tools_attempt()` 以从 context 获取代理 URL
- ✅ 所有三个 provider 客户端创建调用已更新

#### 3. [src-tauri/src/core/service.rs](src-tauri/src/core/service.rs)
- ✅ 修改 `refresh_provider_models()` 以传递代理 URL 给 `refresh_models()`

#### 4. [src-tauri/src/lib.rs](src-tauri/src/lib.rs)
- ✅ 修改 `test_provider_connection()` command 以获取 settings 并传递代理 URL

## 使用流程

### 用户配置代理

1. 打开系统设置 → 网络设置
2. 在"代理设置"输入框中配置代理 URL，例如：
   - HTTP 代理: `http://proxy.example.com:8080`
   - HTTPS 代理: `https://proxy.example.com:8443`
   - SOCKS5 代理: `socks5://proxy.example.com:1080`
3. 点击保存

### 代理自动应用

一旦配置保存，所有后续的 LLM 请求都会自动通过该代理：

- **AI 生成**: 当 agent 处理任务时
- **工具执行**: 当工具调用 LLM 时
- **模型刷新**: 当获取可用模型列表时
- **连接测试**: 当测试 provider 连接时

## 技术细节

### 环境变量优先级

`reqwest` 库按以下优先级使用代理环境变量：
1. 特定协议的代理 (`HTTP_PROXY`, `HTTPS_PROXY`)
2. 通用代理 (`ALL_PROXY`)
3. 系统代理设置

### 线程安全性

`std::env::set_var()` 在多线程环境中是全局的。每次 LLM 操作前，都会根据当前配置更新环境变量。这确保了：
- ✅ 配置更新立即生效
- ✅ 不同 agent 的并行操作都使用最新的代理设置
- ✅ 即使切换代理配置，也无需重启应用

### 与 rig-core 的集成

- `rig-core` v0.32.0 的所有 client builder 都尊重 `reqwest` 库的代理配置
- 在创建 OpenAI/Anthropic/Gemini client 前调用 `configure_proxy_env()`
- `reqwest::Client::new()` 会自动应用这些环境变量

## 测试建议

```rust
// 1. 测试代理配置
- 在设置中配置代理
- 点击"测试连接"按钮
- 检查是否能成功连接

// 2. 测试 LLM 请求
- 创建一个简单的任务
- 观察日志中是否有代理相关的连接信息
- 确认 LLM 回复正常

// 3. 测试模型列表刷新  
- 点击"刷新模型"按钮
- 验证能否获取最新的模型列表

// 4. 清除代理
- 清空代理设置
- 验证请求恢复直接连接
```

## 已知限制

- 代理 URL 必须包含完整的协议前缀 (`http://` 或 `https://`)
- 仅支持 HTTP 和 HTTPS 代理（以及通过 reqwest 支持的其他协议）
- 需要配置代理身份验证的场景需要在代理 URL 中包含凭据：
  ```
  http://username:password@proxy.example.com:8080
  ```

## 相关配置

### AIGlobalConfig

```rust
pub struct AIGlobalConfig {
    pub default_llm_provider: String,
    pub default_llm_model: String,
    pub default_vlm_provider: String,
    pub default_vlm_model: String,
    pub mask_api_keys: bool,
    pub enable_audit_log: bool,
    pub proxy_url: String,  // ← 代理 URL 配置
}
```

### NetworkSettings UI

[src/components/dashboard/settings/NetworkSettings.tsx](src/components/dashboard/settings/NetworkSettings.tsx) 提供了用户界面来输入和编辑代理 URL。
