# Mole Desktop

基于 [tw93/Mole](https://github.com/tw93/mole) 的清理规则思路，用 **Tauri 2 + React + Rust** 实现的跨平台桌面清理与系统工具，支持 **macOS** 与 **Windows**。

所有扫描与删除均在本地完成，不会上传任何数据、不含任何付费订阅逻辑；唯一的联网行为是手动触发的版本检查与安装包下载。

## 功能

| 模块 | 能力 |
| --- | --- |
| 仪表盘 | CPU / 内存 / 磁盘 / 网络实时概览，系统信息，一键跳转清理 |
| 深度清理 | 按 mac / Windows 平台规则扫描缓存、日志、废纸篓、开发工具残留等，分类勾选后执行 |
| 应用卸载 | 读取已安装应用，扫描关联残留文件（缓存、配置、偏好设置等），可连同残留一并删除；应用正在运行时先询问是否结束进程 |
| 磁盘分析 | 目录占用递归统计 + 树图（Treemap），支持逐层下钻与快捷入口 |
| 系统监控 | 定时刷新的 CPU / 内存 / 磁盘 / 进程快照 |
| 系统优化 | DNS 缓存、Spotlight 索引、内存整理、Windows 临时文件等优化任务 |
| 项目清理 | 扫描常见构建产物（`node_modules`、`target`、`dist`、`build`、`.venv` 等），批量删除 |
| 安装包清理 | 扫描下载目录中的 `.dmg` / `.pkg` / `.exe` / `.msi` 等安装包 |
| 历史与设置 | 清理历史记录、白名单、默认删除方式（废纸篓 / 永久删除）等本地持久化配置 |
| 应用更新 | 检查 GitHub Releases 新版本，支持国内镜像加速下载；macOS 下载后原地替换并自动重启，其他平台启动官方安装程序 |
| 变更记录 | 内置各版本发布说明，更新后重新打开应用时自动展示一次，也可随时从侧边栏查看 |

## 技术栈

- **前端**：React 19 + TypeScript 5 + Vite 7 + lucide-react，纯手写深色主题
- **后端**：Tauri 2 + Rust 1.x
  - `sysinfo` 采集系统指标
  - `trash` 实现跨平台废纸篓
  - `rayon` 并行扫描目录
  - `plist` 解析 macOS 应用信息，`winreg` 读取 Windows 注册表安装信息
  - 自研 JSON 持久化，无外部数据库依赖

## 目录结构

```
.
├── src/                    # 前端
│   ├── api.ts              # invoke 封装
│   ├── types.ts            # 与后端 models 对应的类型
│   ├── components/         # Sidebar / Treemap / 通用 UI
│   └── pages/              # 11 个功能页面
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── commands/       # system / clean / apps / configs / brew / analyze / projects / optimize / misc
│   │   ├── rules.rs        # macOS 与 Windows 清理规则
│   │   ├── util.rs         # 路径校验、删除、废纸篓
│   │   ├── store.rs        # 历史与设置持久化
│   │   ├── state.rs        # 全局状态
│   │   └── models.rs       # 数据结构
│   ├── vendor/             # 本地修补的 sysinfo（见下方说明）
│   └── tauri.conf.json
└── package.json
```

## 开发

前置要求：

- Node.js 18+
- Rust stable（项目按 1.94 及以上验证）
- macOS：Xcode Command Line Tools
- Windows：Visual Studio C++ Build Tools + WebView2 Runtime

```bash
npm install
npm run app:dev      # 启动 Tauri 开发窗口（自动拉起 Vite）
```

仅调试前端：

```bash
npm run dev
```

## 构建

### macOS

```bash
npm run app:build
```

产物：`src-tauri/target/release/bundle/macos/Mole Desktop.app`
以及同目录下的 `.dmg`。

首次运行会触发通知、磁盘访问等系统授权，请按提示允许。

### macOS 权限

应用只在真正需要读取目录时请求权限，不会在启动时弹窗：

- **文件与文件夹**（桌面 / 文稿 / 下载 / 外接磁盘）：第一次扫描到这些目录时，macOS 会自动弹出授权提示。用途说明定义在 `src-tauri/Info.plist`。
- **完全磁盘访问**：macOS 不允许应用通过弹窗申请该权限。磁盘分析、项目清理等页面会在缺失权限时显示提示卡片，可一键打开「系统设置 → 隐私与安全性 → 完全磁盘访问」，把 Mole Desktop 拖入列表即可。
- **授权后必须重启应用**：macOS 会缓存当前进程的拒绝结果，勾选完全磁盘访问后当前进程仍会读到旧状态，请点提示卡片里的「重启应用」。从系统设置切回窗口时也会自动重新检测。
- 开发模式下系统会把权限记在终端或调试器名下；测试授权行为请使用打包后的 `.app`。
- 正式发布包使用固定证书签名，签名身份跨版本不变，升级后已授权的权限会保留。只有在你更换证书、改用 ad-hoc 签名的本地构建，或系统仍沿用旧的拒绝记录时，才需要执行：

```bash
tccutil reset SystemPolicyAllFiles com.moledesk.app
```

### Windows

```bash
npm run app:build
```

产物：`src-tauri\target\release\bundle\nsis\Mole Desktop_0.1.0_x64-setup.exe`

需要在装有 MSVC 工具链的环境中构建；Tauri 会自动使用 WebView2。

### Makefile

也可以用 `Makefile` 完成同样的构建、版本与发布操作：

```bash
make init                              # 安装依赖、tauri-cli、rustup 目标
make dev-app                           # Tauri 开发模式（热重载）
make build-app                         # 构建当前平台的安装包
make build-app OS=macos ARCH=universal BUNDLES=app,dmg
make build-app OS=windows ARCH=x86_64 BUNDLES=msi,nsis
make build-cli                         # 只构建可执行文件，不打安装包

make release TYPE=patch                # 自动升版本并提示后续 git 命令
make bump-version VERSION=0.1.2        # 同步 Cargo / tauri.conf / package.json 版本
make check-version TAG=v0.1.2          # 校验各处版本与 tag 一致
```

`make` 会用 `src-tauri/icons/app-icon.png` 作为图标源重新生成各平台图标。

### GitHub Actions 发布

`.github/workflows/release.yml` 在推送 `v*` tag 时自动打包并创建 Release，也可以在 Actions 页面手动触发（仅构建，产物作为 workflow artifact 下载）：

- macOS：Universal（Apple Silicon + Intel）`.dmg`
- Windows：x86_64 `.msi` 与 NSIS `-setup.exe`

发布前请确保版本一致：

```bash
make release TYPE=patch
git push origin HEAD --tags
```

### 发布说明（Release Notes）

`scripts/release-notes.sh` 会读取上一个 tag 到当前 HEAD 之间的提交，以及工作区里尚未提交/尚未跟踪的改动，生成面向用户的中文更新说明，写入 `release-notes/v<version>.md`：

```bash
make release TYPE=minor              # 升版本并自动生成 release-notes/v0.8.0.md
make notes VERSION=0.8.0             # 单独生成或重新生成
make notes VERSION=0.8.0 NO_AI=1     # 不调用 AI，直接按提交标题归类
make notes VERSION=0.8.0 PUBLISH=1   # 直接推送到已存在的 GitHub Release
make notes VERSION=0.8.0 EFFORT=medium  # 覆盖默认的 high effort
scripts/release-notes.sh 0.8.0 --dry-run   # 只打印，不写文件
```

默认调用 `claude -p --effort high` 生成说明——`--effort high` 只作用于这次的 AI 调用，不影响你交互式会话的 effort 设置。可以用 `AI_EFFORT` / `--effort` 改级别（`low|medium|high|xhigh|max`，传 `none` 则不传该参数、沿用 CLI 默认）；也可以用 `AI_CMD` 指定任意命令（从 stdin 读取 prompt，向 stdout 输出 Markdown），或用 `AI_MODEL` / `--model` 指定模型。AI 不可用时会退化为按提交标题生成。

生成的文件随 tag 一起提交，CI 在 Release 创建完成后会自动用 `release-notes/v<tag>.md` 覆盖 tauri-action 的默认正文。AI 生成的内容建议先人工检查再提交。

### macOS 代码签名（自签名）

没有 Apple Developer 账号（$99/年）时，项目使用一张固定的自签名证书，而不是 ad-hoc 签名。两者都过不了 Gatekeeper，但：

- ad-hoc 签名的 designated requirement 里是本次构建的 cdhash，**每次发版都会变**，用户必须重新授权完全磁盘访问；
- 固定证书的 requirement 是 `bundle id + 证书指纹`，**跨版本稳定**，权限不会重置。

签名**不会**让 Gatekeeper 放行。首次打开仍提示“无法验证开发者”，需到「系统设置 → 隐私与安全性」点击「仍要打开」。macOS 15 起已取消右键打开的绕过方式。

生成证书（只需做一次）：

```bash
bash scripts/macos-signing-cert.sh --import
```

脚本会在 `.signing/`（已 gitignore）生成证书和私钥，并把证书导入本机 keychain 供本地构建使用。随后把三个值写进 GitHub Secrets：

```bash
gh secret set MACOS_CERT_P12 < .signing/cert.p12.b64
gh secret set MACOS_CERT_PASSWORD --body "$(cat .signing/p12-password.txt)"
gh secret set MACOS_SIGNING_IDENTITY --body "Mole Desktop Signing"
```

CI 会用这些 secrets 创建临时 keychain 并签名。**请离线备份 `.signing/cert.pem` 和 `.signing/key.pem`**；证书丢失或更换会导致已安装用户的系统授权再次重置。

本地带签名构建：

```bash
APPLE_SIGNING_IDENTITY="Mole Desktop Signing" make build-app SIGN=1
```

验证签名身份是否稳定：

```bash
codesign -d -r- "/Applications/Mole Desktop.app"
# designated => identifier "com.moledesk.app" and certificate root = H"..."
```

只要这条 requirement 不变，TCC 授权就会跨版本保留。

从旧的 ad-hoc 版本升级上来的用户，系统里记录的还是旧的 cdhash，需要一次性重新授权：

```bash
tccutil reset SystemPolicyAllFiles com.moledesk.app
```

然后重新在「系统设置 → 隐私与安全性 → 完全磁盘访问」里勾选。仅此一次，之后的版本不需要再操作。

## 安全设计

- **白名单保护**：系统关键目录（`/System`、`/usr`、`C:\Windows`、`C:\Program Files` 等）硬编码拒绝删除。
- **默认进废纸篓**：所有删除默认调用系统回收站，只有显式选择"永久删除"才会真正 `remove`。
- **扫描与执行分离**：先扫描列出待删项，用户确认后才执行，不存在静默删除。
- **仅操作可重建内容**：缓存、日志、构建产物等规则均为可重新生成的目录。
- **完全离线**：无网络请求、无遥测、无账号体系。

## 关于 `vendor/sysinfo`

本机 crates.io 无法拉取更新版本时，`sysinfo` 通过本地 patch 引入（`src-tauri/vendor/`），仅调整了 `rust-version` 与少量 API 兼容性。若你的环境可以正常联网，可删除 `Cargo.toml` 中的 `[patch.crates-io]` 段与 `vendor/` 目录，改用官方版本。

## 已知限制

- 需要管理员权限的清理项（如系统级缓存）当前仅列出，不自动提权执行。
- Windows 卸载调用注册表中的卸载命令，部分应用可能弹出自带卸载向导。
- 磁盘分析对超大目录采用并行递归统计，首次扫描可能需要数秒。

## 许可证

本项目采用 [Mole Desktop License 1.0](LICENSE)（源码可见，非 OSI 开源协议）：

- **个人使用**：学习、自用、教学、科研、非营利场景，免费且不受限制。
- **商业使用**：允许，但必须在应用内「关于/致谢」、衍生项目 README、分发或商店页面显著保留项目名称 **Mole Desktop** 及仓库地址。
- 希望在商业使用中去除署名的，需事先取得作者书面授权。

完整条款见 [LICENSE](LICENSE)；[LICENSE.zh-CN.md](LICENSE.zh-CN.md) 为中文参考译文（以英文版为准）。

清理规则思路参考 [tw93/Mole](https://github.com/tw93/mole)（MIT），本项目为独立实现；第三方依赖仍遵循其各自的许可证。

