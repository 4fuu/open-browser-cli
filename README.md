# browser-cli

> 用你的真实浏览器——会话、登录态、Cookie 全部保留

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)]()
[![English](https://img.shields.io/badge/lang-English-informational)](README.en.md)

面向命令行和 AI 的浏览器会话操作工具。通过 Chrome/Firefox 扩展 + Native Messaging，将你的真实浏览器页面结构化为 XML/JSON 输出，并支持点击、输入等交互操作。

**核心特性：**
- **有状态会话** — 登录态、Cookie、跳转历史全部保留
- **体积极小** — CLI 二进制约 2 MB，内存占用很低，易于安装和使用
- **结构化 XML 输出** — token 消耗低，AI/Agent 直接可读
- **短 ID 操作** — 无需 CSS 选择器，稳定可靠
- **高仿真交互** — 模拟真实鼠标轨迹和键盘输入，绕过反爬检测
- **性能优异** — 读操作命中内存缓存，DOM 分段采集不阻塞渲染，结构化计算在 CLI 侧完成
- **声明式插件** — TOML 规则文件，复用自动化操作

---

## 对比

| | browser-cli | [opencli](https://github.com/jackwener/opencli) | Playwright / Selenium | curl / requests |
|---|---|---|---|---|
| 浏览器实例 | 真实浏览器（Chrome/Firefox） | 真实浏览器（Chrome） | 新建独立实例 | 无浏览器 |
| 会话保留 | ✅ 登录态 / Cookie | ✅ 复用 Chrome 会话 | ❌ 每次重置 | ❌ |
| 支持范围 | 任意页面 | 50+ 预置站点 | 任意页面 | 任意 URL |
| 交互方式 | 通用（click / type） | 站点专属命令 | 编程 API | HTTP 请求 |
| 反爬检测 | 真实指纹 | 防检测增强 | 易被识别 | 易被识别 |
| 页面输出 | 精简 XML/JSON | 确定性结果 JSON | HTML / DOM | HTML |
| AI/Agent 消费 | 低 token，结构化 | 低 token，结构化 JSON | 高 token，原始 | 高 token |

---

## 目录

1. [安装](#安装)
2. [使用](#使用)
3. [开发](#开发)
4. [为什么选择 browser-cli](#为什么选择-browser-cli)
5. [贡献](#贡献)

---

## 安装

### 1. 加载扩展

**Chrome：**

在 Chrome 打开 `chrome://extensions`，开启「开发者模式」，点击「加载已解压的扩展程序」，选择 `extension/dist/chrome/` 目录（开发模式）；或从 [Releases](../../releases) 下载 `.zip`、解压后以同样方式加载。

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
# 打开网页，创建会话并返回当前页面结构
browser-cli open https://example.com

# 查看页面结构
browser-cli page s1234567890

# 点击元素（支持 e1 / 1 / 文本查询）
browser-cli click s1234567890 "Sign In"

# 向输入框输入文本
browser-cli type s1234567890 "Search" "hello world"

# 等待页面稳定，或等待指定文本出现
browser-cli wait s1234567890
browser-cli wait s1234567890 --for "Continue"

# 查看被截断的长文本 / 分页块
browser-cli text s1234567890 t1
browser-cli block s1234567890 b1 --source-page 1 --all

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

browser-cli page <session-id> [-p <页码>] [--next] [--prev] [--all] [--settle <毫秒>] [--fresh] [--json] [--verbose]
browser-cli click <session-id> <目标> [-p <页码>] [--new-session] [--fresh] [--quiet] [--json]
browser-cli type <session-id> <目标> <文本> [-p <页码>] [--fresh] [--quiet] [--json]
browser-cli search <session-id> <关键词> [--fresh] [--json] [--verbose]
browser-cli text <session-id> <文本ID|数字> [-p <页码>] [--fresh] [--json]
browser-cli block <session-id> <块ID|数字> [--source-page <页码>] [(-p <块页码>)|--all] [--fresh] [--json] [--verbose]
browser-cli view <session-id> <目标> [-p <页码>] [--fresh] [--json] [--verbose]
browser-cli wait <session-id> [--for <文本>] [--timeout <毫秒>] [--quiet] [--json]
browser-cli screenshot <session-id> [--output <路径>] [--full-page] [--quality <0-100>] [--json]
browser-cli download <session-id> <元素ID|URL> [--output <路径>] [--json]

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
- `--all` 会按逻辑页自动滚动、等待懒加载渲染并聚合成单页阅读视图；输出固定为 `current=1 total=1`
- `--settle <毫秒>` 仅配合 `page --all` 使用，控制每次滚动后的固定等待时间，默认 `500`
- `--fresh` 跳过缓存，强制从浏览器获取最新快照
- `--version` 显示构建时注入的版本号；若未注入则显示 `unknown`
- `open` 默认会在创建会话后直接输出当前页；用 `--quiet` 只看会话信息，用 `--wait 0` 可跳过打开后的稳定等待
- `open` / `close` / `list` / `search` / `wait` / `plugin` / `view` 全部支持 `--json`
- `page` / `search` / `block` / `view` 支持 `--verbose`：主要用于拿完整 JSON 细节；不带时，`--json` 默认返回更紧凑的数据
- `page --all` 更偏全量阅读视图，方便 AI/用户一次拿完整内容；如果后续要点击或输入，建议回到普通 `page` / `page -p N` 重新获取当前页元素 ID
- `click` / `type` 的 `<目标>` 既可以是带前缀 ID（如 `e1`）、数字 ID（如 `1` 对应 `e1`），也可以是当前页交互元素的文本查询；查询会匹配按钮文本、链接文本、输入框 placeholder/value 等
- `click` / `type` 默认会输出更新后的整页 XML；可用 `--quiet` 只看成功结果，用 `--json` 获取结构化摘要
- `wait` 默认等待页面稳定并返回最新页面；`--for <文本>` 会轮询最新快照，直到页面里出现匹配该文本的元素
- `screenshot` 可保存当前页面视口截图；指定 `--quality` 时会输出 JPEG，否则默认 PNG；`--full-page` 当前会提示并回退为视口截图
- `download` 可下载浏览器当前会话可访问的资源；目标既可以是页面元素 ID（自动取其 `href` / `src`），也可以是直接 URL
- `search` 在 XML/纯文本模式下仍返回 `page`、`tag`、上下文摘要，以及命中交互元素时的 `element_id`；在 `--json` 下默认返回紧凑结果，`--verbose` 才返回完整匹配结构
- 长文本截断会明确显示为 `[...truncated]`
- 超长 `list` / `table` 会在页面中先显示首段，并带上块级分页属性；分页按渲染后的 XML 行数预算切分，而不是按条目数量硬切；可用 `browser-cli block <session-id> <块ID或数字> --source-page <页码> -p <块页码>` 读取单页，或用 `--all` 一次展开整个块
- `view` 会返回某个元素、长文本或长块的聚焦视图；目标支持 `e3` / `3` / `t1` / `b1` / 文本查询
- `view` 命中列表或表格中的元素时，默认只返回包含该目标的单条 `item` / `row`；加 `--verbose` 才返回完整列表/表格上下文
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
npm run build   # 产物：extension/dist/chrome/ 和 extension/dist/firefox/
npm run pack    # 打包为 extension/dist/browser-cli-extension.zip 和 browser-cli-extension.xpi
```

---

## 为什么选择 browser-cli

browser-cli 操作的是你本机正在运行的真实浏览器。会话由用户（Agent）显式打开和关闭，只要会话存活，登录状态、Cookie、表单输入、跳转历史全部保留。

页面以精简 XML 返回，只保留可交互元素和可见文本，过滤所有无关噪声，token 消耗低，语义清晰，AI/Agent 直接可读。

每次 `page` 后交互元素获得短 ID（`e1`, `e2`, ...），直接用 ID 操作，无需构造易碎的 CSS 选择器或 XPath。点击模拟真实鼠标轨迹，输入逐字符派发并带随机延迟，通过原型 setter 绕过 React/Vue 等框架的 value 检测。

`page`/`search`/`text` 等只读命令优先命中 Relay 内存缓存，无需每次往返浏览器；只有 `click`/`type` 等写操作才触发浏览器执行并更新快照。

Content Script 采用分段推送（每 100 个节点、8ms 间隔），不阻塞页面渲染，繁重的结构化计算全部在 CLI 侧完成。重复操作可用 TOML 插件描述，一次编写，所有会话自动触发。

---

## 贡献

欢迎提交 Issue 和 Pull Request。贡献前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。
