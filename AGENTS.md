# browser-cli — 项目说明

面向命令行和 AI 的浏览器会话操作工具。提供显式会话管理、结构化页面视图和基于元素 ID 的交互，
不做通用网页抓取，不做传统的无状态浏览器自动化。

---

## 目录

1. [项目结构](#项目结构)
2. [整体架构](#整体架构)
3. [设计理念](#设计理念)
4. [通信协议](#通信协议)
5. [CLI 详解](#cli-详解)
6. [Relay 详解](#relay-详解)
7. [浏览器扩展详解](#浏览器扩展详解)
8. [页面结构化与 XML 输出](#页面结构化与-xml-输出)
9. [插件机制](#插件机制)
10. [当前实现状态](#当前实现状态)
11. [待完成事项（TODO）](#待完成事项todo)

---

## 项目结构

```
browser-cli/
├── Cargo.toml
├── src/
│   ├── main.rs                  # 入口，clap 命令分发
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── commands.rs          # 各命令实现（open/close/page/click/type 等）
│   │   └── output.rs            # 输出格式化（XML / JSON）
│   ├── relay/
│   │   ├── mod.rs
│   │   ├── server.rs            # Relay 模式：TCP Server + SessionCache + chunk 组装
│   │   └── native_msg.rs        # Native Messaging 协议编解码（4 字节长度前缀）
│   ├── transport/
│   │   ├── mod.rs
│   │   └── client.rs            # CLI 模式：连接 Relay 的 TCP 客户端
│   ├── page/
│   │   ├── mod.rs
│   │   ├── structure.rs         # 页面结构化：raw nodes → PageData + XML（含分页过滤）
│   │   └── xml.rs               # XML 序列化
│   ├── plugin/
│   │   ├── mod.rs
│   │   ├── loader.rs            # TOML 规则文件加载
│   │   └── runner.rs            # 规则执行
│   └── protocol/
│       ├── mod.rs
│       └── messages.rs          # 所有消息类型（Request, Response, RawNode, PageChunk 等）
└── extension/
    ├── manifest.json
    ├── src/
    │   ├── shared/types.ts      # 所有 TypeScript 类型定义（与 Rust 侧对称）
    │   ├── background/
    │   │   └── service-worker.ts  # Native Messaging 通信、会话管理、消息路由
    │   └── content/
    │       └── content-script.ts  # DOM 遍历、分段发送 chunk、交互执行
    └── dist/                    # 构建产物（esbuild）
```

---

## 整体架构

```
用户/AI
  │
  ▼
browser-cli <命令>          ← CLI 模式（每次命令短连接）
  │ TCP 127.0.0.1:12899 (JSON lines)
  ▼
browser-cli relay           ← Relay 模式（由 Chrome 通过 Native Messaging 拉起）
  │ stdin/stdout Native Messaging（4 字节长度前缀 + JSON）
  ▼
Chrome / Firefox 后台脚本  ← 浏览器扩展后台
  │ chrome.tabs.sendMessage
  ▼
Content Script              ← 注入到目标页面
```

同一个 Rust 二进制，两种运行模式：

| 模式 | 启动方式 | 职责 |
|------|---------|------|
| **CLI 模式** | 用户在终端执行命令 | 发送请求、处理响应、输出结果 |
| **Relay 模式** | Chrome 通过 Native Messaging 拉起 | TCP 桥接 + SessionCache 缓存 |

---

## 设计理念

### 1. 计算职责分层：浏览器做最少的事

**浏览器 Content Script — 薄数据采集器**

- 只做 DOM 遍历 + `offsetParent` 廉价可见性判断 + 读取 tag/textContent/attributes/rect
- 不做文本规范化、截断、合并、交互分类、分页过滤
- 不生成 CSS 选择器，用内部递增 `refId`（`r1`, `r2`, ...）标识元素，存 `WeakRef` 供后续交互
- 分段发送：每 100 个节点一个 chunk，chunk 之间 sleep 8ms，避免阻塞页面渲染

**Relay — 缓存 + 快照组装**

- 接收浏览器分段 chunks，组装为完整的 `RawSnapshot`
- 按 session_id 在内存中缓存最新快照（`SessionCache`）
- 读操作（`get_page`/`search`/`get_text`）命中缓存直接返回，不经浏览器
- 写操作（`click`/`type`）转发给浏览器，浏览器执行后推送新快照，Relay 覆盖缓存
- 保持轻量，不做任何节点处理

**CLI — 计算大头**

- 从 Relay 获取 `RawSnapshot`（原始节点列表）
- 节点分类（交互 / 文本 / 标题）、文本规范化 / 合并 / 截断
- 分页过滤（按 viewport + rect 筛选）、元素上限裁剪（200）、短 ID 分配（e1, e2...）
- 文本检索（search）、完整文本查看（text）
- 页面结构化编码为 XML 或 JSON 输出
- 声明式插件执行

### 2. 显式会话

会话是用户显式打开和关闭的。只要会话存活，浏览器页面状态（登录、输入、跳转历史）完整保留。
这是区别于无状态抓取的核心差异。

### 3. 结构化页面视图，而非原始 HTML

网页返回精简的 XML，而不是 HTML 源码：

- 保留可交互元素（链接、按钮、输入框等），每个分配短 ID
- 保留可见文本，非交互文本压缩 / 截断
- 过滤不可见元素、装饰性元素、脚本、样式
- 内容过多时分页，支持按条件检索

### 4. 操作前需先获取页面

元素 ID（`e1`, `e2`, ...）每次 `get_page` 时从 e1 重新编号。这是故意的：
全局递增会给出"旧元素还能操作"的假象，但翻页或导航后旧元素可能已不在 DOM 中。
操作前必须先 `page` 获取当前状态，语义清晰。

### 5. 本地通信，安全隔离

整条通信链路全部在本机：Native Messaging 是浏览器内置机制，Relay 监听 `127.0.0.1:12899`。
Content Script 不向页面注入全局变量，不修改页面原始 DOM 结构。

---

## 通信协议

### 消息格式

#### CLI → Relay（JSON lines over TCP）

```json
{ "id": "uuid", "action": "open", "params": { "url": "https://example.com" } }
```

#### Relay → Extension（Native Messaging：4 字节 little-endian 长度前缀 + JSON）

同上格式，经 Relay 透传。

#### Extension → Relay（page_chunk 消息）

浏览器不直接返回完整页面，而是分段推送 chunk：

```json
{
  "type": "page_chunk",
  "session_id": "s1",
  "request_id": "uuid",
  "meta": { "url": "...", "title": "...", "viewport": {...}, "scroll": {...} },
  "nodes": [
    { "ref": "r1", "parent": null, "tag": "a", "text": "Sign In", "attrs": {"href": "/login"}, "rect": {...} }
  ],
  "chunk_index": 0,
  "done": true
}
```

首个 chunk（`chunk_index == 0`）携带 `meta`，后续 chunk 省略。最后一个 chunk `done: true`。

#### Relay → CLI（JSON lines over TCP）

```json
{ "id": "uuid", "ok": true, "data": { ... } }
{ "id": "uuid", "ok": false, "error": "session not found" }
```

### 动作类型

| action | 触发者 | 参数 | 说明 |
|--------|--------|------|------|
| `open` | CLI | `{ url, wait_after_load? }` | 创建 tab，等待加载，并可额外等待 DOM 稳定后发起首次快照 |
| `close` | CLI | `{ session_id }` 或 `{ all: true }` | 关闭 tab，清理缓存 |
| `list` | CLI | — | 返回所有活跃会话列表 |
| `get_page` | CLI | `{ session_id }` | Relay 优先走缓存；未命中则触发浏览器快照 |
| `search` | CLI | `{ session_id, query }` | Relay 返回缓存快照，CLI 做文本匹配 |
| `get_text` | CLI | `{ session_id, text_id }` | Relay 返回缓存快照，CLI 取完整文本 |
| `click` | CLI | `{ session_id, ref }` | 转发给 Content Script，执行后推送新快照 |
| `type` | CLI | `{ session_id, ref, text }` | 同上 |
| `wait` | CLI | `{ session_id, timeout? }` | 转发给 Content Script，等待页面稳定；CLI 的 `--for <text>` 走本地轮询快照匹配 |

补充：`click --new-session` 是 CLI 层的显式分支，不会修改协议。CLI 若发现目标元素是带 `href` 的链接，
会直接把链接解析成绝对 URL 并走 `open` 创建新会话；只有普通 `click` 才会继续转发给浏览器当前 tab。

### Relay 缓存逻辑

```
GET_PAGE / SEARCH / GET_TEXT
  └─ 缓存命中（complete == true）→ 直接返回 RawSnapshot
  └─ 未命中 → 转发给浏览器 → 等待 chunk 组装完成 → 返回

CLICK / TYPE
  └─ 转发给浏览器 → Content Script 执行动作 → 推送新快照 chunks → Relay 更新缓存 → 返回

CLOSE
  └─ 透传 + sessions.remove(session_id)
```

---

## CLI 详解

### 技术栈

| 组件 | 选型 |
|------|------|
| 异步运行时 | tokio |
| 本地 Socket 通信 | tokio TCP |
| 命令行解析 | clap（derive 模式）|
| 序列化 | serde_json |
| 配置/插件解析 | toml |
| 错误处理 | anyhow |
| 唯一 ID | uuid v4 |

### 命令列表

```
browser-cli setup [--browser chrome|firefox] [--extension-id <id>]
    生成 Native Messaging host 注册 JSON 文件

browser-cli relay
    以 Relay 模式运行（由 Chrome 拉起，用户一般不直接调用）

browser-cli open <url> [--wait <ms>] [--quiet] [--json]
    打开网页，创建会话；默认直接输出当前页面，`--wait` 控制打开后的稳定等待时长（默认 3000ms，0 表示跳过），`--quiet` 仅输出会话摘要，`--json` 返回结构化结果

browser-cli --version
    显示构建时注入的版本号；若构建时未提供环境变量，则显示 `unknown`

browser-cli list [--json]
    列出所有活跃会话；`--json` 返回结构化会话列表

browser-cli close <session-id> [--json]
browser-cli close --all [--json]
    关闭会话；`--json` 返回 `{ closed: n }`

browser-cli page <session-id> [-p <page-num>] [--next] [--prev] [--fresh] [--json]
    获取结构化页面内容（XML 或 JSON）；--next/--prev 相对当前滚动位置翻页，`--fresh` 强制绕过 Relay 缓存

browser-cli click <session-id> <target> [-p <page-num>] [--new-session] [--fresh] [--quiet] [--json]
    点击元素；`target` 既可以是数字元素 ID（如 1 对应 e1），也可以是当前页交互元素的文本查询；`--fresh` 先强制刷新快照再解析目标，`--quiet` 仅输出成功摘要，`--json` 返回结构化结果；默认会附带更新后的页面；若指定 `--new-session` 且目标为链接，则新开会话访问该 URL，保持原页面不变

browser-cli type <session-id> <target> <text> [-p <page-num>] [--fresh] [--quiet] [--json]
    向输入框输入文本；`target` 既可以是数字元素 ID，也可以是当前页交互元素的文本查询；`--fresh` 先强制刷新快照再解析目标，`--quiet` 仅输出成功摘要，`--json` 返回结构化结果；默认会附带更新后的页面

browser-cli search <session-id> <query> [--fresh] [--json]
    在页面中检索文本和关键属性；结果包含 page、tag、context，以及命中交互元素时可直接操作的 element_id

browser-cli wait <session-id> [--for <text>] [--timeout <ms>] [--quiet] [--json]
    等待页面稳定，或通过 `--for` 等待页面中出现匹配文本的元素；默认返回最新页面，`--quiet` 仅输出成功/超时摘要，`--json` 返回结构化 wait 结果

browser-cli text <session-id> <text-id> [-p <page-num>] [--fresh] [--json]
    查看被截断的完整文本（text-id 为 t1, t2...）；`--fresh` 强制绕过缓存

browser-cli block <session-id> <block-id> [--source-page <page-num>] [-p <block-page-num> | --all] [--fresh] [--json]
    查看被分页的长列表或表格块（block-id 为 b1, b2...）；`--source-page` 指定块 ID 来源页面，`-p/--page` 读取块内部单页，`--all` 一次展开整个块

browser-cli view <session-id> <target> [-p <page-num>] [--fresh] [--json]
    查看单个元素、长文本或长块的聚焦视图；`target` 支持 `e3` / `3` / `t1` / `b1` / 文本查询

browser-cli plugin run <name> <session-id> [--json]
    手动执行指定插件规则；`--json` 返回执行摘要

browser-cli plugin list [--json]
    列出所有已安装插件；`--json` 返回结构化插件列表
```

### setup 命令生成的注册文件

Chrome：`~/.config/google-chrome/NativeMessagingHosts/com.browser_cli.relay.json`

```json
{
  "name": "com.browser_cli.relay",
  "description": "Browser CLI relay",
  "path": "/path/to/browser-cli",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://<extension-id>/"]
}
```

Firefox：`~/.mozilla/native-messaging-hosts/com.browser_cli.relay.json`（`allowed_extensions` 替代 `allowed_origins`）

### click / type 的元素定位流程

CLI 在发送 `click`/`type` 前，先通过 `get_page` 获取快照并解析出 `element_refs` 映射表
（`e1 → r3`, `e2 → r7`, ...）。

- 普通 `click` / `type`：若 `target` 是数字，则按 `eN → ref` 直接解析；否则 CLI 会在当前页交互元素中按文本、`href`、`placeholder`、`value` 等字段做首个匹配，再将对应 `ref` 传给浏览器。浏览器 Content Script 用 `elementMap.get(refId)?.deref()` 取出 WeakRef 直接定位元素，无需 CSS 选择器。
- `click --new-session`：CLI 不把请求发给当前 tab，而是检查目标元素是否为带 `href` 的链接；若是，则按当前页面 URL 解析为绝对地址，再直接调用 `open` 新建一个 session。

---

## Relay 详解

Relay 监听固定地址 `127.0.0.1:12899`，单实例。端口被占用时拒绝启动。

### SessionCache 结构

```rust
struct SessionCache {
    snapshot: Option<RawSnapshot>,  // 最新完整快照
    complete: bool,                  // chunk 是否全部收齐
}
```

### chunk 组装规则

- `chunk_index == 0` 或 `meta` 存在：重建快照（清空旧节点）
- 每个 chunk 的 nodes 追加到 `snapshot.nodes`
- `done == true`：标记 `complete = true`

### 多客户端并发

多个 CLI 进程可同时连接同一个 Relay。通过消息 `id` 字段匹配请求-响应（`PendingMap`）。
每个 CLI 进程独立完成计算，Relay 不成为瓶颈。

---

## 浏览器扩展详解

### 技术栈

| 组件 | 选型 |
|------|------|
| 目标浏览器 | Chrome Manifest V3，架构兼容 Firefox |
| 后台脚本 | Chrome Service Worker / Firefox background script |
| DOM 操作 | Content Script，原生 DOM API |
| 构建 | esbuild（无框架，原生 TypeScript）|

### 后台脚本职责

- 通过 `chrome.runtime.connectNative("com.browser_cli.relay")` 连接 Relay
- 接收 Relay 发来的请求，路由到对应 tab 的 Content Script
- 接收 Content Script 发来的 `page_chunk` 事件，通过 Native Messaging 转发给 Relay
- 管理会话表（`session_id ↔ tab_id`）
- 监听 `tabs.onRemoved`（自动清理会话）、`tabs.onUpdated`（同步 URL/title/status）
- Native Messaging 断线时记录警告，下次收到消息时重连

### Content Script 职责

**数据采集（`collectSnapshot`）：**

1. 清空 `elementMap` 和 `refCounter`
2. 从 `document.body` 开始递归 `walkElement`
3. 跳过 `script/style/noscript/svg/path` 标签、`aria-hidden="true"` 元素、零尺寸元素
4. 可见性判断：`offsetParent !== null`；`position:fixed/sticky` 元素 fallback 到 `getBoundingClientRect`
5. 为每个元素分配 `refId`（`r1`, `r2`, ...），存入 `elementMap`（WeakRef）
6. 收集：tag、textContent、功能性 attrs（href/type/placeholder/name/role/aria-label/onclick/disabled/checked/selected/value）、绝对坐标 rect
7. 递归进入 open Shadow DOM（`element.shadowRoot`）

**分段发送（`streamSnapshot`）：**

每 100 个节点一个 chunk，通过 `chrome.runtime.sendMessage` 发给后台脚本，
chunk 间隔 8ms sleep。首个 chunk 携带 meta，最后一个 chunk `done: true`。

**交互执行：**

- `click`：`scrollIntoView` → 等下一帧 → 模拟鼠标轨迹（3-6 步 `mousemove`，随机起点逐步逼近目标）
  → 派发 `mouseover/mousedown`（随机持续 50-150ms）`/mouseup/click` 事件序列，点击坐标在元素中央 40% 区域随机偏移
  → 等待页面稳定 → 推送新快照
- `type`：`focus` → 清空值（通过原型 setter，绕过 React 等框架的 value 检测）
  → 逐字符派发 `keydown/input/keyup`，字符间随机延迟 40-120ms → 触发 `change/blur` → 等待页面稳定 → 推送新快照
- `wait`：MutationObserver 监听全局 DOM 变更，连续 500ms 无变更视为稳定，超时上限由调用方指定

**可视鼠标光标（调试用）：**

`click` 操作时在页面注入一个固定定位的箭头光标 SVG overlay（`id="__browser_cli_cursor__"`）：
- `pointer-events:none`，不干扰页面交互
- `z-index: 2147483647`，挂在 `<html>` 下避免被 overflow 裁剪
- 随鼠标轨迹模拟实时移动，操作结束后停留在最后点击位置

**页面稳定判定：**

- `MutationObserver` 监听 `subtree/childList/characterData/attributes`
- 连续 `STABILITY_WINDOW_MS = 500ms` 无变更 → 稳定
- 如果指定了 `selector`，检测到元素出现立即返回
- 超时拒绝 Promise

### Firefox 兼容

Relay 和 CLI 无需修改。扩展差异：
- API 命名空间：`chrome.*` → `browser.*`（Promise-based），可用 polyfill 兼容
- Native Messaging 注册：`allowed_origins` → `allowed_extensions`（CLI `setup --browser firefox` 已处理）
- 后台脚本：Manifest 同时声明 `background.scripts` 和 `background.service_worker`，Firefox 走 background script，Chrome 走 Service Worker

---

## 页面结构化与 XML 输出

### DOM → XML 映射

| HTML 原始 | XML 输出 | 保留属性 |
|-----------|---------|---------|
| `<a>` | `<link>` | href |
| `<button>`, `[role="button"]`, `[onclick]`, `input[submit/button/reset]` | `<button>` | — |
| `<input type="text/email/...">` | `<input>` | type, value, placeholder, disabled |
| `<input type="checkbox">` | `<checkbox>` | checked |
| `<input type="radio">` | `<radio>` | name, selected |
| `<select>` | `<select>` | selected（当前值）, disabled |
| `<textarea>` | `<textarea>` | placeholder, disabled |
| `<h1>`-`<h6>` | `<heading level="N">` | — |
| 纯文本节点 | `<text>` | id（仅截断时分配）|
| `<ul>`/`<ol>` | `<list><item>...</item></list>` | id / truncated / shown / total_items / current / total（仅长列表分页时） |
| `<table>` | `<table><row><cell>` | id / truncated / shown / total_items / current / total（仅长表格分页时） |

**属性保留策略**：只保留功能性属性（href/type/value/placeholder/disabled/checked/selected/name），
丢弃 class/style/data-*/原始 DOM id 等表现性属性。

### 输出示例

```xml
<page url="https://example.com" title="Example" current="1" total="3">
  <heading level="1">Welcome</heading>
  <text id="t1">This is the beginning of a long article[...truncated]</text>
  <link id="e1" href="/login">Sign In</link>
  <button id="e2">Get Started</button>
  <input id="e3" type="text" placeholder="Search..."/>
  <checkbox id="e4" checked/>
  <list id="b1" truncated="true" shown="18" total_items="42" current="1" total="3">
    <item>Item one</item>
    <item>Item two</item>
  </list>
  <table>
    <row><cell>Name</cell><cell>Value</cell></row>
    <row><cell>Foo</cell><cell>Bar</cell></row>
  </table>
</page>
```

截断时额外属性：`truncated="true" shown="200" total_items="347"`

### 元素 ID 分配规则

- 交互元素：`e1`, `e2`, ... — 每次 `get_page` 从 e1 重新编号
- 文本节点（超过 200 字符时截断）：`t1`, `t2`, ... — 同样每次重新编号
- 长列表/表格块（超过单块渲染行预算时分页）：`b1`, `b2`, ... — 每次 `get_page` 从 b1 重新编号，用 `block` 命令继续读取
- 未截断的短文本不分配 id

### 分页策略

由 CLI 按 viewport + rect 过滤，而非浏览器侧滚动：

- `total_pages = ceil(scroll.height / viewport.height)`
- 当前页 = `requested_page` 或从 `scroll.top` 推断
- 只返回 rect 与当前页区间有交叠的节点

### 元素数量上限

单页最多输出 **200** 个元素。超过时优先保留交互元素，再填充文本节点。
输出标注 `truncated="true" shown="200" total="347"`。

### 块级分页

- 长文本超过 200 字符时会在页面中显示为 `[...,truncated]`，并分配 `tN`
- 长 `list` / `table` 超过单块渲染行预算时，会先输出首个块分页并分配 `bN`
- 表格中如果某个 `row` 只有一个 `cell`，且整行足够短，会压缩成单行 XML；太长时再展开为多行
- 后续分页通过 `browser-cli block <session-id> <block-id> --source-page <page-num> -p <block-page-num>` 读取单页，也可用 `--all` 一次展开整个块

### 复杂 DOM 处理

- **嵌套交互元素**：只保留最外层（`has_interactive_ancestor` 检测）
- **Shadow DOM**：open 模式递归进入，closed 模式跳过
- **元素文本 fallback**：无可见文本时用 `aria-label` 或 `placeholder` 作为标签

### search 实现

CLI 在 `RawSnapshot` 的节点文本和关键属性（`href`、`placeholder`、`value`、`aria-label`、`name`）上做大小写不敏感匹配，
按“交互元素优先、文本命中优先、位置靠前优先”排序，返回最多 50 条结果。
每条结果包含：

- `page`
- `element_id`（若命中的是当前页可操作的交互元素）
- `ref_id`
- `tag`
- `text`
- `context`

---

## 插件机制

规则文件为 TOML 格式，放在 `~/.config/browser-cli/plugins/` 下。

### 规则文件格式

```toml
name = "skip-cookie-banner"
description = "自动关闭 cookie 同意弹窗"
match = "*.example.com/*"
trigger = "on_load"   # on_load | on_navigate | manual

[[steps]]
wait = "Accept"
timeout = 3000
action = "click"

[[steps]]
wait = 500            # 固定等待 ms
```

### 字段说明

| 字段 | 说明 |
|------|------|
| `name` | 规则名称 |
| `match` | URL 通配符匹配模式 |
| `trigger` | 触发时机：`on_load` / `on_navigate` / `manual` |
| `steps[].wait` | 等待条件：页面文本 / 元素 ID 查询（字符串）或固定毫秒数（数字）|
| `steps[].timeout` | 等待超时，超时则跳过该步骤（默认 5000ms）|
| `steps[].action` | `click` / `type` / `scroll` |
| `steps[].value` | `type` 动作的输入文本 |

### 执行逻辑

1. `open` 或页面导航后，CLI 检查所有规则文件
2. 按 `match` 匹配当前 URL
3. 按 `trigger` 判断是否执行
4. 顺序执行 steps；`wait` 为字符串时，CLI 轮询当前 `RawSnapshot`，在本地结构化页面中做匹配
5. `click` / `type` 命中目标后，由 CLI 将目标解析成当前页元素 ID / `ref`，再发给浏览器执行
6. 某步超时则跳过继续，全部完成后返回执行摘要；`plugin run --json` 会额外给出结构化统计

### 查询匹配规则

- 若 `wait` 为 `e3` 这类元素 ID，按当前页元素 ID 精确匹配
- 否则按大小写不敏感子串匹配交互元素文本、placeholder、href、当前值等
- 兼容旧写法 `button:contains('Accept')`，CLI 会提取其中的 `Accept` 作为查询词

### 管理命令

```
browser-cli plugin run <name> <session-id> [--json]   # 手动执行指定规则
browser-cli plugin list [--json]                      # 列出已安装插件
```

---

## 当前实现状态

### 已完成

**Rust CLI：**

- `main.rs`：全部命令的 clap 定义（relay / setup / open / close / list / page / click / type / search / wait / text / plugin）
- `protocol/messages.rs`：全部消息类型（`Request`, `Response`, `RawNode`, `RawSnapshot`, `PageChunk`），含完整单元测试
- `relay/server.rs`：TCP Server、`SessionCache`、chunk 组装、缓存读写、写操作缓存更新、`close` 缓存清理，含单元测试
- `relay/native_msg.rs`：Native Messaging 协议编解码
- `transport/client.rs`：连接 Relay 的 TCP 客户端
- `cli/commands.rs`：全部命令实现（`setup` 生成注册 JSON 已实现）
- `page/structure.rs`：`parse_page_from_snapshot`（raw nodes → PageData）、`search_snapshot`，含完整单元测试
- `plugin/loader.rs`：TOML 规则文件加载
- `plugin/runner.rs`：CLI 侧规则执行（轮询快照、本地匹配目标、再发 `ref` 给浏览器）

**浏览器扩展：**

- `shared/types.ts`：全部类型定义（与 Rust 侧对称）
- `background/service-worker.ts`：Native Messaging 连接管理、会话表（open/close/list）、消息路由、chunk 转发、tab 生命周期监听
- `content/content-script.ts`：`collectSnapshot`（DOM 采集）、`streamSnapshot`（分段发送）、`handleClick`（点击）、`handleType`（输入）、`handleWait`（稳定等待）、Shadow DOM 递归、可见性判断、元素 WeakRef 映射

### 已实现但未在 TODO 中标注

- `generateSelector` 已移除，Content Script 已改为 `refId + WeakRef` 定位
- Content Script 的 `click`/`type`/`wait` 实现（TODO 中标记未完成，但代码已存在）
- Relay 的 `SessionCache` 结构及 chunk 组装（TODO 中标记未完成，但代码已存在）
- `page_chunk` 协议类型已在 Rust / Extension 两侧定义并打通
- CLI 中 `search` 和 `text` 命令本地化（已走缓存快照，不回浏览器）
- `setup` 命令（已实现，支持 `--browser` 和 `--extension-id`）

---

## 待完成事项（TODO）

以下条目来自 TODO.md，结合代码确认后的真实状态：

### 架构重构（计划中，尚未完全落地）

Content Script 目前仍保留了部分旧逻辑（`getComputedStyle` 可见性 fallback 已存在），
以下是尚未完成的精细化改造：

- [ ] **分段发送细节** — 当前实现已有 chunk 机制（CHUNK_SIZE=100，CHUNK_DELAY_MS=8），但 Relay 侧对"新快照覆盖旧缓存"的时序处理需验证

### Rust CLI（低优先级）

- [x] **`page --next` / `page --prev`** — 已实现，相对当前滚动位置计算翻页
- [x] **`plugin list`** — 已实现，`browser-cli plugin list`
- [ ] **`--verbose` 全局选项** — debug 输出

### 浏览器扩展（低优先级）

- [x] **native messaging 断线重连** — `onDisconnect` 时触发 exponential backoff 重连（初始 1s，最大 30s）；收到第一条消息后重置 delay；`ensureNativePort` 在 timer 期间不重复触发
- [x] **高保真人类操作模拟** — click 前模拟鼠标移动轨迹（3-6 步 mousemove），事件间加随机延迟（mousedown→mouseup 50-150ms），点击坐标落在元素中央 40% 区域内随机偏移；type 逐字符延迟 40-120ms
- [x] **可视鼠标光标 overlay** — click 操作时在页面渲染一个跟随光标图标，供调试观察操作位置

---

## 边界

**项目关注点：**
- 显式会话 + 浏览器内持久状态
- 面向命令行 / AI 的网页结构化表示
- 基于结构化页面的交互（点击、输入）
- 可复用的自动化规则（插件）

**不在范围内：**
- 完整网页源码输出
- 无状态抓取
- 截图 / 视觉渲染
- 多 Chrome Profile 同时使用（当前 Relay 单实例）
