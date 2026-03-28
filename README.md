# browser-cli

面向命令行和 AI 的浏览器会话操作工具。通过 Chrome/Firefox 扩展 + Native Messaging 协议，将浏览器页面结构化为 XML/JSON 输出，并支持点击、输入等交互操作。

English version: [README.en.md](README.en.md)

**核心优势：**
- 有状态会话——登录态、Cookie、跳转历史全部保留
- 结构化 XML 输出——token 消耗低，AI/Agent 直接可读
- 短 ID 操作——无需 CSS 选择器，稳定可靠
- 高仿真交互——模拟真实鼠标轨迹和键盘输入，绕过反爬检测
- 声明式插件——TOML 规则文件，复用自动化操作
- 本地通信——全链路在本机，完全隔离

---

## 目录

1. [安装](#安装)
2. [使用](#使用)
3. [开发](#开发)
4. [为什么选择 browser-cli](#为什么选择-browser-cli)

---

## 安装

### 1. 加载扩展

**Chrome：**

在 Chrome 打开 `chrome://extensions`，开启「开发者模式」，点击「加载已解压的扩展程序」，选择 `extension/` 目录（开发模式）；或从 [Releases](../../releases) 下载 `.zip` 后以同样方式加载。

记录扩展 ID（形如 `abcdefghijklmnopabcdefghijklmnop`），后续注册时需要用到。

**Firefox：**

从 [Releases](../../releases) 下载 `.xpi` 文件，在 Firefox 打开 `about:addons`，点击齿轮图标 → 「从文件安装附加组件」，选择 `.xpi` 文件完成安装。

### 2. 安装 CLI

**macOS / Linux：**

```sh
curl -fsSL https://raw.githubusercontent.com/4fuu/open-browser-cli/main/install.sh | sh
```

**Homebrew：**

```sh
brew tap 4fuu/open-browser-cli https://github.com/4fuu/open-browser-cli
brew install browser-cli
```

**Windows（Scoop）：**

```powershell
scoop bucket add open-browser-cli https://github.com/4fuu/open-browser-cli
scoop install browser-cli
```

<details>
<summary>Windows（PowerShell 脚本）</summary>

```powershell
irm https://raw.githubusercontent.com/4fuu/open-browser-cli/main/install.ps1 | iex
```

</details>

### 3. 注册 Native Messaging Host

**Chrome：**

```bash
browser-cli setup --extension-id <扩展ID>
```

**Firefox：**

```bash
browser-cli setup --browser firefox
```

注册文件写入后，**重启浏览器** 使配置生效。如需卸载：

```bash
browser-cli teardown --browser chrome   # 或 --browser firefox
```

---

## 使用

### 基本流程

```bash
# 打开网页，创建会话并直接返回当前页面
browser-cli open https://example.com

# 只看会话信息，不附带页面内容
browser-cli open https://example.com --quiet

# 也可以直接拿结构化结果；必要时可调整打开后的稳定等待时间
browser-cli open https://example.com --json
browser-cli open https://example.com --wait 5000

# 查看页面结构
browser-cli page s1234567890

# 点击元素；目标既可以是 `e1` / `1`，也可以是页面上的文本查询
browser-cli click s1234567890 1
browser-cli click s1234567890 e1
browser-cli click s1234567890 "Sign In"

# 自动化场景中可只返回成功摘要
browser-cli click s1234567890 1 --quiet
browser-cli click s1234567890 1 --json

# 如果目标是链接，也可以新开一个会话访问，保持原页面不变
browser-cli click s1234567890 1 --new-session

# 向输入框输入文本；同样支持 `e3` / `3` 或文本查询
browser-cli type s1234567890 3 "hello world"
browser-cli type s1234567890 e3 "hello world"
browser-cli type s1234567890 "Search" "hello world"

# 查看被截断长文本；支持 `t1` / `1`
browser-cli text s1234567890 t1
browser-cli text s1234567890 1

# 查看分页块；支持 `b1` / `1`
browser-cli block s1234567890 b1 --source-page 1 -p 2
browser-cli block s1234567890 1 --source-page 1 --all

# search 结果会直接给出 page 和可操作的 element_id
browser-cli search s1234567890 "search" --json

# wait 默认等待页面稳定；也可以等待指定文本出现
browser-cli wait s1234567890 --timeout 5000
browser-cli wait s1234567890 --for "Continue" --json

# 查看单个元素/长文本/长列表块的聚焦视图
browser-cli view s1234567890 e3
browser-cli view s1234567890 "pricing"

# 关闭会话
browser-cli close s1234567890
```

### 命令速查

```
browser-cli open <url> [--wait <毫秒>] [--quiet] [--json]
browser-cli list [--json]
browser-cli close <session-id> [--json]
browser-cli close --all [--json]
browser-cli --version

browser-cli page <session-id> [-p <页码>] [--next] [--prev] [--fresh] [--json]
browser-cli click <session-id> <目标> [-p <页码>] [--new-session] [--fresh] [--quiet] [--json]
browser-cli type <session-id> <目标> <文本> [-p <页码>] [--fresh] [--quiet] [--json]
browser-cli search <session-id> <关键词> [--fresh] [--json]
browser-cli text <session-id> <文本ID|数字> [-p <页码>] [--fresh] [--json]
browser-cli block <session-id> <块ID|数字> [--source-page <页码>] [(-p <块页码>)|--all] [--fresh] [--json]
browser-cli view <session-id> <目标> [-p <页码>] [--fresh] [--json]
browser-cli wait <session-id> [--for <文本>] [--timeout <毫秒>] [--quiet] [--json]

browser-cli plugin list [--json]
browser-cli plugin run <名称> <session-id> [--json]

browser-cli setup [--browser chrome|firefox] [--extension-id <ID>]
browser-cli teardown [--browser chrome|firefox]
```

### 页面输出

```xml
<page url="https://example.com" title="Example" current="1" total="3">
  <heading level="1">Welcome</heading>
  <text id="t1">这是一段较长的文本[...truncated]</text>
  <link id="e1" href="/login">Sign In</link>
  <button id="e2">Get Started</button>
  <input id="e3" type="text" placeholder="Search..."/>
  <checkbox id="e4" checked/>
  <list id="b1" truncated="true" shown="18" total_items="42" current="1" total="3">
    <item>Item one</item>
    <item>Item two</item>
  </list>
</page>
```

- `e1`, `e2`, ... — 交互元素 ID，用于 `click` / `type`；参数支持 `e1` 或 `1`
- `t1`, `t2`, ... — 被截断的长文本 ID，用 `text` 命令查看完整内容；参数支持 `t1` 或 `1`
- `b1`, `b2`, ... — 被分页的长 `list` / `table` 块 ID，用 `block` 命令继续查看后续分页；参数支持 `b1` 或 `1`
- `--next` / `--prev` 按当前滚动位置相对翻页
- `--fresh` 跳过缓存，强制从浏览器获取最新快照
- `--version` 显示构建时注入的版本号；若未注入则显示 `unknown`
- `open` 默认会在创建会话后直接输出当前页；用 `--quiet` 只看会话信息，用 `--wait 0` 可跳过打开后的稳定等待
- `open` / `close` / `list` / `search` / `wait` / `plugin` / `view` 全部支持 `--json`
- `click` / `type` 的 `<目标>` 既可以是带前缀 ID（如 `e1`）、数字 ID（如 `1` 对应 `e1`），也可以是当前页交互元素的文本查询；查询会匹配按钮文本、链接文本、输入框 placeholder/value 等
- `click` / `type` 默认会输出更新后的整页 XML；可用 `--quiet` 只看成功结果，用 `--json` 获取结构化摘要
- `wait` 默认等待页面稳定并返回最新页面；`--for <文本>` 会轮询最新快照，直到页面里出现匹配该文本的元素
- `search` 会返回 `page`、`tag`、上下文摘要，以及命中交互元素时的 `element_id`
- 长文本截断会明确显示为 `[...truncated]`
- 超长 `list` / `table` 会在页面中先显示首段，并带上块级分页属性；分页按渲染后的 XML 行数预算切分，而不是按条目数量硬切；可用 `browser-cli block <session-id> <块ID或数字> --source-page <页码> -p <块页码>` 读取单页，或用 `--all` 一次展开整个块
- `view` 会返回某个元素、长文本或长块的聚焦视图；目标支持 `e3` / `3` / `t1` / `b1` / 文本查询
- `click --new-session` 仅对带 `href` 的链接生效；CLI 会把链接解析成绝对 URL，并直接创建一个新的 session，原页面保持不变

### 插件

规则文件（TOML）放在 `~/.config/browser-cli/plugins/`：

```toml
name = "skip-cookie-banner"
description = "自动关闭 cookie 弹窗"
match = "*.example.com/*"
trigger = "on_load"

[[steps]]
wait = "Accept"
timeout = 3000
action = "click"
```

- `browser-cli plugin list --json` 返回结构化插件列表
- `browser-cli plugin run <名称> <session-id> --json` 返回执行摘要（总步数、完成/跳过/失败数量、页面是否更新）

### 注意事项

- Relay 监听固定端口 `127.0.0.1:12899`，同一时间只运行一个实例
- 元素 ID（`e1`, `e2`, ...）每次 `page` 后重新编号，操作前需先获取当前页面
- `page --fresh`、`search --fresh`、`text --fresh`、`block --fresh`、`view --fresh`，以及 `click` / `type` 的 `--fresh`，都用于动态页面需要绕过缓存的场景
- `wait` 默认就会在成功后返回最新页面；自动化链路里如果只需要成功/超时结果，用 `--quiet`
- `click --new-session` 是显式行为，不会自动套用到普通点击；如果目标元素不是链接，命令会直接报错

---

## 开发

**CLI（Rust）：**

```bash
cargo build --release
# 产物：target/release/browser-cli
```

**浏览器扩展：**

```bash
cd extension
npm install
npm run build   # 产物：extension/dist/
npm run pack    # 打包为 extension/dist/browser-cli-extension.zip
```

---

## 为什么选择 browser-cli

### 有状态会话，而非无状态抓取

会话由用户显式打开和关闭。只要会话存活，浏览器中的登录状态、Cookie、表单输入、跳转历史全部保留——这是 `curl`/`requests` 等无状态工具根本做不到的事。

```bash
browser-cli open https://github.com/login   # 打开会话
browser-cli type s1 1 "your-username"       # 填写用户名（登录态持续保留）
browser-cli type s1 2 "your-password"
browser-cli click s1 3                      # 提交登录
browser-cli page s1                         # 查看登录后的页面
```

### 结构化页面视图，而非原始 HTML

页面以精简 XML 返回，而不是几千行 HTML 源码。只保留可交互元素和可见文本，过滤掉 `class`/`style`/`script` 等所有无关噪声。极度适合 AI/Agent 消费：token 消耗低，语义清晰，无歧义。

### 短 ID 直接操作，无需 CSS 选择器

每次 `page` 后，交互元素获得短 ID（`e1`, `e2`, ...），直接用 ID 操作，无需构造易碎的 CSS 选择器或 XPath。底层通过 `WeakRef` 直接持有 DOM 元素引用，稳定可靠，不受页面样式变化影响。

### 读操作走缓存，响应极快

`page`/`search`/`text` 等只读命令优先命中 Relay 内存缓存，无需每次往返浏览器。只有 `click`/`type` 等写操作才触发浏览器执行并更新快照。

### 本地通信，完全隔离

整条链路全部在本机：Native Messaging 是浏览器内置机制，Relay 仅监听 `127.0.0.1:12899`。Content Script 不向页面注入全局变量，不修改页面 DOM 结构。

### 高仿真交互，绕过反爬检测

- **点击**：模拟真实鼠标轨迹（3-6 步 `mousemove`，随机起点逐步逼近），点击坐标在元素中央 40% 区域随机偏移，`mousedown → mouseup` 间随机延迟 50-150ms
- **输入**：逐字符派发 `keydown/input/keyup`，字符间随机延迟 40-120ms，通过原型 setter 绕过 React/Vue 等框架的 value 检测

### 声明式插件，复用自动化规则

用 TOML 文件描述重复操作（如关闭 Cookie 弹窗、自动登录），一次编写，所有会话自动触发：

```toml
name = "skip-cookie-banner"
match = "*.example.com/*"
trigger = "on_load"

[[steps]]
wait = "Accept"
timeout = 3000
action = "click"
```

### 架构设计：计算职责分层

```
browser-cli <命令>
  │ TCP 127.0.0.1:12899
  ▼
browser-cli relay          ← 由浏览器扩展通过 Native Messaging 拉起
  │ stdin/stdout
  ▼
浏览器扩展后台脚本
  │
  ▼
Content Script（注入目标页面）
```

Content Script 只做轻量 DOM 采集（每 100 个节点分段推送，8ms 间隔，不阻塞页面渲染），Relay 负责缓存与快照组装，繁重的结构化计算（节点分类、文本规范化、分页过滤）全部在 CLI 侧完成。
