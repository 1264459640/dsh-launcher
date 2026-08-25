# ⚡ DSH Launcher

多版本、多实例的 [DeepSeek Harness (DSH)](https://github.com/deepseek-ai/deepseek-harness) 桌面启动器。

Tauri 2 + Vue 3 + TypeScript + Sass + vue-router + vue-i18n + Arco Design Vue。

## 功能

- **多版本安装**：从 npm registry 查询并安装多个 `@deepseek-ai/dsh` 版本到隔离目录，互不干扰。
- **多实例管理**：同一版本可创建多个实例，每个实例拥有独立的名称、Profile 与运行时环境变量。
- **DSH_HOME 三种模式**：
  - 复用/共用已有的 DSH_HOME；
  - 为实例新建专属 DSH_HOME（自动在数据目录下创建并注册）。
- **一键启动**：主页选择实例 + Profile 后一键启动；启动后解析 `dsh web` 输出的 URL，在独立 Webview 窗口中打开 DSH Web GUI。
- **环境变量复写**：实例设置页可增删运行时环境变量，启动时注入子进程（`DSH_HOME` 为保留项，由启动器按所选 DSH_HOME 注入）。
- **系统托盘**：
  - 双击托盘：仅一个运行实例时打开其 Webview 窗口，否则打开启动器；
  - 右键菜单：每个运行中实例提供「打开窗口 / 退出（Profile）」；另有「打开启动器 / 退出启动器」。
  - 退出启动器时自动终止所有实例进程，避免孤儿进程。
- **关闭最小化到托盘**（可在设置关闭）。
- **开机自启**（设置页开关，经 autostart 插件真正注册）。
- **i18n**：简体中文 / English，JSON 语言文件由 `@intlify/unplugin-vue-i18n` 经 Vite 发现、热重载并预编译。

## 界面

- **启动页**：左侧面板（实例状态 → 实例/Profile 联动下拉 → 大启动按钮 → 实例列表/实例设置），右侧预留新闻区域。
- **下载页**：侧边栏「实例创建 / 插件下载」；实例创建页按正式版/预览版分组展示可装版本，点击版本进入命名页（输入实例名、选择 DSH_HOME，底部「开始下载」）。
- **实例列表**：名称、版本、DSH_HOME、Profile、运行状态与 URL、设置/删除。
- **设置页**：语言、关闭到托盘、开机自启、DSH_HOME 管理。

## 开发

前置：Node ≥ 20、pnpm、Rust stable（含 MSVC 工具链）、WebView2。

```bash
pnpm install
pnpm tauri dev      # 开发模式（前端 Vite + 后端 debug）
pnpm tauri build    # 打包（生成 exe / msi / nsis 安装包）
```

前端无后端时可在浏览器预览（mock 层，数据存 localStorage）：

```bash
pnpm dev            # 打开 http://localhost:1420
```

## 运行数据

- 启动器配置与数据：`%APPDATA%\in.dsh-plug.dsh-launcher\`
  - `config.json`：DSH_HOME / 版本 / 实例 / 设置
  - `versions/<版本>/`：各版本隔离安装目录
  - `homes/<实例名>/`：专属 DSH_HOME（如选择）
  - `logs/<实例id>.log`：实例运行日志

## 架构

- `src/`：Vue 3 前端（页面、store、API 封装、i18n）
- `src/api/index.ts`：统一 API 层——Tauri 环境走 `invoke`，浏览器环境走 localStorage mock
- `src-tauri/src/`
  - `config.rs`：配置模型与原子持久化
  - `commands.rs`：全部 Tauri 命令（CRUD / 版本安装 / 实例启停 / 设置）
  - `process.rs`：实例进程管理（spawn / kill / URL 解析 / 环境注入 / 日志）
  - `tray.rs`：系统托盘与动态菜单
  - `windows.rs`：实例 Webview 窗口管理

## License

MIT
