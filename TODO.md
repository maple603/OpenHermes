# OpenHermes 待完善功能清单

本文档列出了所有处于占位/框架状态的功能，按优先级和模块分类，方便后续逐步完善。

---

## 📊 状态说明

- 🔴 **未实现** - 完全占位，需要从零开发
- 🟡 **部分实现** - 框架已搭建，缺少核心逻辑
- 🟢 **待扩展** - 基础功能可用，需要增强

---

## 🔴 模块 1：记忆系统 (Memory System)

**当前状态**: 框架已搭建，核心功能未实现

### 1.1 SQLite 数据库
- [ ] SQLite 数据库初始化与连接池
- [ ] 数据库迁移脚本
- [ ] 会话记录表 (sessions)
- [ ] 记忆条目表 (memory_entries)
- [ ] 标签系统 (tags)
- [ ] 元数据存储 (metadata)

### 1.2 FTS5 全文搜索
- [ ] FTS5 虚拟表创建
- [ ] 全文索引同步
- [ ] 搜索 API（支持布尔运算、短语搜索）
- [ ] 搜索结果排序（TF-IDF + 相关性）

### 1.3 记忆操作
- [ ] `memory_read` 工具实现
  - 语义搜索
  - 关键词搜索
  - 时间范围过滤
- [ ] `memory_write` 工具实现
  - 自动去重
  - 相似记忆合并
  - 重要性评分
- [ ] `memory_search` 工具实现
  - 多条件查询
  - 分页支持
- [ ] `memory_forget` 工具实现
  - 过期记忆清理
  - 低重要性记忆删除

### 1.4 会话搜索
- [ ] 会话历史存储
- [ ] 按关键词搜索历史会话
- [ ] 按时间范围筛选
- [ ] 会话摘要生成

### 1.5 记忆提供者接口
- [ ] 外部记忆提供者支持
- [ ] Honcho 集成（如原项目）
- [ ] 记忆提供者热加载

**涉及文件**:
- `openhermes-memory/src/builtin_provider.rs`
- `openhermes-memory/src/memory_manager.rs`
- 需要新增: `database.rs`, `fts5.rs`, `session_store.rs`, `memory_tools.rs`

**优先级**: 🔥 高（Agent 长期记忆核心）

---

## 🔴 模块 2：工具系统扩展 (Tools)

**当前状态**: 仅实现 3 个基础工具，需要实现 40+ 工具

### 2.1 Web 工具
- [ ] `web_search` - 网络搜索（Tavily / DuckDuckGo / Google）
- [ ] `web_extract` - 网页内容提取
- [ ] `web_fetch` - 原始 HTML 获取
- [ ] `url_safety_check` - URL 安全检查
- [ ] `website_policy_check` - 网站策略检查

### 2.2 浏览器自动化
- [ ] `browser_navigate` - 导航到 URL
- [ ] `browser_click` - 点击元素
- [ ] `browser_type` - 输入文本
- [ ] `browser_screenshot` - 截图
- [ ] `browser_get_text` - 获取文本
- [ ] `browser_execute_js` - 执行 JavaScript
- [ ] 反指纹支持（Camofox 集成）

### 2.3 文件操作增强
- [ ] `search_files` - 文件搜索（glob + 内容）
- [ ] `list_directory` - 目录列表
- [ ] `copy_file` - 复制文件
- [ ] `move_file` - 移动文件
- [ ] `delete_file` - 删除文件
- [ ] `create_directory` - 创建目录
- [ ] `file_edit` - 精准文件编辑（search/replace）
- [ ] `diff_apply` - 应用 diff/patch

### 2.4 终端工具增强
- [ ] 交互式终端支持（tmux 集成）
- [ ] 进程管理（启动/停止/监控）
- [ ] 后台任务管理
- [ ] 输出流式处理
- [ ] ANSI 转义码过滤
- [ ] 沙箱执行支持

### 2.5 MCP (Model Context Protocol)
- [ ] MCP 客户端实现
- [ ] MCP 服务器连接
- [ ] MCP 工具发现与注册
- [ ] MCP OAuth 认证
- [ ] 多 MCP 服务器支持

### 2.6 记忆工具
- [ ] 与记忆系统集成
- [ ] `session_search` - 会话历史搜索

### 2.7 技能系统工具
- [ ] `skills_install` - 安装技能
- [ ] `skills_list` - 列出可用技能
- [ ] `skills_sync` - 同步技能
- [ ] `skills_hub_search` - 技能市场搜索

### 2.8 多媒体工具
- [ ] `image_generation` - 图像生成（DALL-E / Stability）
- [ ] `tts` - 文本转语音（NeuTTS）
- [ ] `transcription` - 语音转文字（Whisper）
- [ ] `vision` - 图像理解

### 2.9 辅助工具
- [ ] `todo` - 待办事项管理
- [ ] `clarify` - 请求用户澄清
- [ ] `delegate` - 任务委托给子 Agent
- [ ] `mixture_of_agents` - 多模型投票
- [ ] `checkpoint` - 检查点管理
- [ ] `cronjob_tools` - Cron 任务管理

### 2.10 安全工具
- [ ] `approval` - 用户审批工作流
- [ ] `tirith_security` - 安全策略检查
- [ ] `osv_check` - 漏洞检查
- [ ] 凭证文件管理

### 2.11 集成工具
- [ ] `homeassistant` - 智能家居控制
- [ ] 消息发送工具（跨平台）

**涉及文件**:
- 需要新增 40+ 工具文件在 `openhermes-tools/src/tools/`
- `openhermes-tools/src/lib.rs` 需要更新工具注册

**优先级**: 🔥 高（工具是 Agent 能力的核心）

---

## 🔴 模块 3：消息平台网关 (Gateway)

**当前状态**: 完全占位

### 3.1 核心架构
- [ ] GatewayRunner 实现
- [ ] 消息路由系统
- [ ] 会话管理
- [ ] 状态管理
- [ ] Hook 系统
- [ ] 消息交付队列
- [ ] 流式消息处理

### 3.2 平台适配器（17+ 平台）

#### 即时通讯
- [ ] **Telegram** - Telegram Bot API
- [ ] **Discord** - Discord.py 等效实现
- [ ] **Slack** - Slack Events API + RTM
- [ ] **WhatsApp** - WhatsApp Business API / Baileys
- [ ] **Signal** - signal-cli 集成
- [ ] **Matrix** - Matrix Client-Server API
- [ ] **Mattermost** - Mattermost API
- [ ] **Feishu/Lark** - 飞书开放平台
- [ ] **DingTalk** - 钉钉开放平台
- [ ] **WeCom** - 企业微信 API
- [ ] **SMS** - Twilio / Vonage

#### 其他渠道
- [ ] **Email** - IMAP/SMTP
- [ ] **Webhook** - 通用 Webhook 接收
- [ ] **Home Assistant** - HA 集成
- [ ] **API Server** - OpenAI 兼容 API
- [ ] **ACP (Agent Communication Protocol)** - ACP 协议支持

### 3.3 平台特性
- [ ] 配对系统（设备绑定）
- [ ] 状态贴纸/表情
- [ ] 消息镜像（多平台同步）
- [ ] 平台特定功能（按钮、卡片等）
- [ ] 速率限制处理
- [ ] 断线重连
- [ ] Webhook 验证

### 3.4 OpenAI API 服务器
- [ ] `/v1/chat/completions` 端点
- [ ] 流式响应 (SSE)
- [ ] 工具调用支持
- [ ] 认证管理
- [ ] 请求限流

**涉及文件**:
- `openhermes-gateway/src/lib.rs`
- 需要新增: `gateway/`, `platforms/`, `session.rs`, `delivery.rs`, `hooks.rs`, `mirror.rs`, `pairing.rs`, `stream_consumer.rs` 等

**优先级**: 🔥 高（多平台接入核心）

---

## 🔴 模块 4：定时任务系统 (Cron)

**当前状态**: 完全占位

### 4.1 核心功能
- [ ] Cron 表达式解析
- [ ] 任务调度器
- [ ] 定时任务执行
- [ ] 任务队列管理
- [ ] 错误重试机制
- [ ] 任务日志

### 4.2 任务类型
- [ ] Agent 自反思任务
- [ ] 定期记忆整理
- [ ] 技能同步任务
- [ ] 数据备份任务
- [ ] 自定义 HTTP 调用
- [ ] 飞书/Slack 定时提醒

### 4.3 管理接口
- [ ] 任务添加/删除/暂停/恢复
- [ ] 任务列表查询
- [ ] 下次执行时间计算
- [ ] 执行历史查询

**涉及文件**:
- `openhermes-cron/src/lib.rs`
- 需要新增: `scheduler.rs`, `jobs.rs`, `cron_parser.rs`, `job_store.rs`

**优先级**: 🟡 中（增强功能，非核心）

---

## 🟡 模块 5：上下文压缩 (Context Compression)

**当前状态**: 框架存在，核心逻辑未实现

### 5.1 Token 估算
- [ ] Tiktoken 集成（或 Rust 等效实现）
- [ ] 精确 token 计数
- [ ] 多模型支持

### 5.2 压缩策略
- [ ] 早期消息摘要化
- [ ] 工具调用结果精简
- [ ] 关键信息保留策略
- [ ] 自动触发压缩

### 5.3 压缩算法
- [ ] LLM 辅助摘要生成
- [ ] 关键实体提取
- [ ] 对话主题聚类
- [ ] 增量压缩

**涉及文件**:
- `openhermes-core/src/context_compressor.rs`

**优先级**: 🟡 中（长对话必需）

---

## 🟡 模块 6：技能系统 (Skills)

**当前状态**: 基础加载可用，缺少完整功能

### 6.1 技能解析
- [ ] SKILL.md 元数据解析（YAML frontmatter）
- [ ] 触发器解析
- [ ] 工具依赖声明
- [ ] 技能版本管理

### 6.2 技能匹配
- [ ] 基于关键词的自动匹配
- [ ] 用户意图识别
- [ ] 技能激活/去激活
- [ ] 技能冲突解决

### 6.3 技能市场
- [ ] ClawHub 集成
- [ ] 技能搜索
- [ ] 技能安装/卸载
- [ ] 技能更新检查
- [ ] 技能安全扫描

### 6.4 技能管理命令
- [ ] `skills list` - 列出已安装技能
- [ ] `skills install <name>` - 安装技能
- [ ] `skills uninstall <name>` - 卸载技能
- [ ] `skills update` - 更新所有技能
- [ ] `skills search <query>` - 搜索技能市场

**涉及文件**:
- `openhermes-skills/src/skill_manager.rs`
- 需要新增: `skill_parser.rs`, `skill_matcher.rs`, `skills_hub.rs`, `skill_security.rs`

**优先级**: 🟡 中（扩展能力）

---

## 🟢 模块 7：CLI 增强

**当前状态**: 基础 REPL 可用，需要增强

### 7.1 TUI (终端用户界面)
- [ ] Ratatui 集成
- [ ] 多面板布局（对话、工具、状态）
- [ ] 实时状态显示
- [ ] 彩色输出
- [ ] 进度条
- [ ] 表格/列表组件

### 7.2 命令完善
- [ ] `/model` - 模型切换完整实现
- [ ] `/tools` - 工具配置
- [ ] `/setup` - 安装向导
- [ ] `/gateway` - 网关管理命令
- [ ] `/cron` - Cron 任务管理
- [ ] `/memory` - 记忆管理命令
- [ ] `/skills` - 技能管理命令
- [ ] `/export` - 导出对话
- [ ] `/import` - 导入对话

### 7.3 交互增强
- [ ] 命令自动补全
- [ ] 命令历史
- [ ] 多行输入支持
- [ ] 粘贴支持
- [ ] 输入验证

**涉及文件**:
- `openhermes-cli/src/main.rs`
- 需要新增 TUI 相关模块

**优先级**: 🟢 低（体验优化）

---

## 🟢 模块 8：配置与优化

### 8.1 配置完善
- [ ] 配置验证
- [ ] 配置热重载
- [ ] 配置文件自动生成
- [ ] 配置模板
- [ ] 环境检测自动化

### 8.2 性能优化
- [ ] 并行工具执行（使用 `tokio::task::JoinSet`）
- [ ] 连接池复用
- [ ] 缓存策略
- [ ] 内存使用优化

### 8.3 日志与监控
- [ ] 结构化日志完善
- [ ] 性能指标收集
- [ ] 错误追踪
- [ ] 使用统计

**优先级**: 🟢 低（优化项）

---

## 📦 模块 9：Docker 与部署

### 9.1 Docker 支持
- [ ] Dockerfile 编写
- [ ] docker-compose.yml
- [ ] 多阶段构建
- [ ] 镜像优化（Alpine 基础镜像）
- [ ] 健康检查
- [ ] 卷挂载配置

### 9.2 部署文档
- [ ] 本地部署指南
- [ ] 云服务器部署
- [ ] Kubernetes 部署（可选）
- [ ] 环境变量配置示例

**优先级**: 🟡 中（生产部署必需）

---

## 🧪 模块 10：测试

### 10.1 单元测试
- [ ] 工具系统测试
- [ ] 配置系统测试
- [ ] Agent 循环测试
- [ ] 内存系统测试

### 10.2 集成测试
- [ ] 端到端 Agent 测试
- [ ] 工具链测试
- [ ] 消息平台模拟测试
- [ ] 配置集成测试

### 10.3 性能测试
- [ ] 并发工具执行测试
- [ ] 内存泄漏检测
- [ ] 响应时间基准测试

**优先级**: 🟡 中（质量保证）

---

## 🎯 建议实施顺序

### Phase 1: 核心工具完善 (2-3 周)
1. Web 搜索工具
2. 文件操作增强
3. 终端工具增强
4. MCP 客户端

### Phase 2: 记忆系统 (2-3 周)
1. SQLite 数据库
2. FTS5 全文搜索
3. 记忆工具实现
4. 会话搜索

### Phase 3: 上下文压缩 (1 周)
1. Token 估算
2. 压缩策略
3. 自动触发

### Phase 4: 消息平台网关 (3-4 周)
1. 核心架构
2. Telegram 适配器
3. Discord 适配器
4. Slack 适配器
5. API 服务器

### Phase 5: 技能系统增强 (1-2 周)
1. 技能解析完善
2. 技能匹配
3. ClawHub 集成

### Phase 6: CLI TUI (1-2 周)
1. Ratatui 集成
2. 命令完善
3. 交互增强

### Phase 7: 其他工具与优化 (持续)
1. 浏览器自动化
2. 多媒体工具
3. 安全工具
4. Docker 部署

---

## 📝 开发注意事项

1. **工具开发规范**:
   - 每个工具实现 `Tool` trait
   - 提供完整的 JSON Schema
   - 包含错误处理
   - 编写单元测试

2. **数据库注意事项**:
   - 使用 `sqlx` 进行编译时 SQL 检查
   - 实现迁移脚本
   - 注意并发安全

3. **消息平台开发**:
   - 每个平台独立模块
   - 统一的 `PlatformAdapter` trait
   - 处理平台特定的速率限制
   - 实现断线重连

4. **性能考虑**:
   - 大量 I/O 操作使用异步
   - CPU 密集型任务使用 `tokio::task::spawn_blocking`
   - 合理设置连接池大小

---

## 🔗 参考资源

- Python 原版实现: `../hermes-agent/tools/`
- Python 原版网关: `../hermes-agent/gateway/`
- Python 原版记忆: `../hermes-agent/agent/memory_*.py`

---

**最后更新**: 2025-04-06
**总计待实现**: ~60 个功能点
**预估工作量**: 10-15 周
