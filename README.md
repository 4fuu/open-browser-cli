# browser-cli

面向命令行和 AI 的浏览器会话操作工具。通过 Chrome 扩展 + Native Messaging 协议，将浏览器页面结构化为 XML/JSON 输出，并支持点击、输入等交互操作。

## 工作原理

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

## 构建

**CLI（Rust）：**

```bash
cargo build --release
# 产物：target/release/browser-cli
```

**浏览器扩展：**

```bash
cd extension
npm install
npm run build        # 仅编译，产物：extension/dist/
npm run pack         # 编译并打包，产物：extension/dist/browser-cli-extension.zip
```

## 安装

### 1. 加载扩展

**Chrome：**

在 Chrome 打开 `chrome://extensions`，开启「开发者模式」，点击「加载已解压的扩展程序」，选择 `extension/` 目录（开发模式）；或从 [Releases](../../releases) 下载 `.zip` 后以同样方式加载。

记录扩展 ID（形如 `abcdefghijklmnopabcdefghijklmnop`）。

**Firefox：**

从 [Releases](../../releases) 下载 `.xpi` 文件，在 Firefox 打开 `about:addons`，点击齿轮图标 → 「从文件安装附加组件」，选择 `.xpi` 文件完成安装。扩展 ID 已固定为 `4fu@browser-cli`，无需手动记录。

### 2. 注册 Native Messaging Host

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

## 使用

### 基本流程

```bash
# 打开网页，返回 session-id
browser-cli open https://example.com
# → Session s1234567890 opened: https://example.com

# 查看页面结构
browser-cli page s1234567890

# 点击元素（e1 为页面输出中的元素 ID）
browser-cli click s1234567890 1

# 向输入框输入文本
browser-cli type s1234567890 3 "hello world"

# 关闭会话
browser-cli close s1234567890
```

### 命令速查

```
browser-cli open <url>
browser-cli list
browser-cli close <session-id>
browser-cli close --all

browser-cli page <session-id> [-p <页码>] [--next] [--prev] [--fresh] [--json]
browser-cli click <session-id> <元素ID> [-p <页码>]
browser-cli type <session-id> <元素ID> <文本> [-p <页码>]
browser-cli search <session-id> <关键词>
browser-cli text <session-id> <文本ID> [-p <页码>]
browser-cli wait <session-id> [--selector <CSS选择器>] [--timeout <毫秒>]

browser-cli plugin list
browser-cli plugin run <名称> <session-id>

browser-cli setup [--browser chrome] [--extension-id <ID>] [--manifest-path <路径>]
browser-cli teardown [--browser chrome] [--manifest-path <路径>]
```

### 页面输出格式

```xml
<page url="https://example.com" title="Example" current="1" total="3">
  <heading level="1">Welcome</heading>
  <text id="t1">这是一段较长的文本...</text>
  <link id="e1" href="/login">Sign In</link>
  <button id="e2">Get Started</button>
  <input id="e3" type="text" placeholder="Search..."/>
  <list>
    <item>Item one</item>
    <item>Item two</item>
  </list>
</page>
```

- `e1`, `e2`, ... — 交互元素 ID，用于 `click` / `type`
- `t1`, `t2`, ... — 被截断的长文本 ID，用 `text` 命令查看完整内容
- `--next` / `--prev` 按当前滚动位置相对翻页
- `--fresh` 跳过缓存，强制从浏览器获取最新快照

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

## 注意事项

- Relay 监听固定端口 `127.0.0.1:12899`，同一时间只运行一个实例
- 元素 ID（`e1`, `e2`, ...）每次 `page` 后重新编号，操作前需先获取当前页面
- `page --fresh` 用于动态页面需要绕过缓存的场景
