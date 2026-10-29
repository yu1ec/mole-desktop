use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tauri::State;
use walkdir::WalkDir;

use crate::models::{
    CleanCrumb, CleanDetail, CleanEntry, ConfigPathInfo, ConfigTool, EntryTrashResult,
    HistoryEntry, OperationResult,
};
use crate::rules::display_path;
use crate::state::AppState;
use crate::store::push_history;
use crate::util::{allocated_size, expand_tilde, home, move_to_trash, now_rfc3339, trash_entry};

struct ConfigPathSpec {
    path: PathBuf,
    kind: &'static str,
    /// 这条路径具体存了什么，展示在配置位置下方。
    description: &'static str,
}

struct Profile {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: &'static str,
    commands: &'static [&'static str],
    paths: Vec<ConfigPathSpec>,
}

fn mac_app_support() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| home.join("Library/Application Support"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn path_of(path: PathBuf, kind: &'static str, description: &'static str) -> ConfigPathSpec {
    ConfigPathSpec {
        path,
        kind,
        description,
    }
}

/// 删除后会直接破坏当前环境的位置：shell 框架被删会导致新终端报错，
/// 全局 Git 配置被删会影响所有仓库的身份、签名与别名。
fn is_protected(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".oh-my-zsh") | Some(".gitconfig")
    )
}

/// 删除风险分级：
/// - `safe`：缓存、临时文件或可重新下载的依赖，删除后会自动重建；
/// - `caution`：配置、凭证、会话或历史记录，删除后需要重新配置/登录；
/// - `keep`：受保护路径，界面禁止勾选。
fn safety_for(path: &Path, kind: &str) -> &'static str {
    if is_protected(path) {
        return "keep";
    }
    const CAUTION: [&str; 14] = [
        "配置", "凭证", "会话", "历史", "数据", "仓库", "全局", "环境", "插件", "扩展", "CLI",
        "SDK", "框架", "状态",
    ];
    const SAFE: [&str; 4] = ["缓存", "临时", "依赖", "下载"];
    if CAUTION.iter().any(|word| kind.contains(word)) {
        return "caution";
    }
    if SAFE.iter().any(|word| kind.contains(word)) {
        return "safe";
    }
    "caution"
}

/// Known developer tool config locations. Only entries that actually exist are
/// returned, so an uninstalled tool that left data behind still shows up.
fn profiles() -> Vec<Profile> {
    let home = home();
    let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
    let data = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
    let cache = dirs::cache_dir().unwrap_or_else(|| home.join(".cache"));
    // macOS 上 `config_dir` 是 ~/Library/Application Support，而很多 CLI 工具
    // 仍然遵循 XDG，把配置放在 ~/.config / ~/.local/share / ~/.cache。
    let xdg_config = home.join(".config");
    let xdg_data = home.join(".local/share");
    let xdg_cache = home.join(".cache");
    let support = mac_app_support();
    let app_cache = home.join("Library/Caches");
    let mut list = vec![
        /* ------------------------------ AI 编程 ------------------------------ */
        Profile {
            id: "codex",
            name: "Codex",
            description: "OpenAI Codex CLI 的登录信息、配置、会话与本地缓存",
            category: "AI 编程",
            commands: &["codex"],
            paths: vec![
                path_of(
                    home.join(".codex"),
                    "配置与会话",
                    "登录凭证、配置、会话历史与日志",
                ),
                path_of(config.join("codex"), "配置", "CLI 配置与本地数据"),
                path_of(xdg_config.join("codex"), "配置", "CLI 配置与本地数据"),
            ],
        },
        Profile {
            id: "claude-code",
            name: "Claude Code",
            description: "Claude Code CLI 的配置、会话记录、插件与缓存",
            category: "AI 编程",
            commands: &["claude"],
            paths: vec![
                path_of(
                    home.join(".claude"),
                    "配置与会话",
                    "项目历史、会话记录、插件与缓存",
                ),
                path_of(
                    home.join(".claude.json"),
                    "配置文件",
                    "账号信息、MCP 服务器与项目设置",
                ),
                path_of(config.join("claude"), "配置", "CLI 配置与本地数据"),
                path_of(xdg_config.join("claude"), "配置", "CLI 配置与本地数据"),
            ],
        },
        Profile {
            id: "opencode",
            name: "OpenCode",
            description: "OpenCode 的配置、会话数据库、插件与日志",
            category: "AI 编程",
            commands: &["opencode"],
            paths: vec![
                path_of(
                    xdg_config.join("opencode"),
                    "配置与插件",
                    "配置文件、自定义命令与插件",
                ),
                path_of(
                    xdg_data.join("opencode"),
                    "数据与会话",
                    "会话数据库、日志与本地模型数据",
                ),
                path_of(xdg_cache.join("opencode"), "缓存", "临时缓存与下载内容"),
                path_of(config.join("opencode"), "配置", "桌面 / CLI 配置"),
                path_of(data.join("opencode"), "数据", "桌面 / CLI 本地数据"),
                path_of(cache.join("opencode"), "缓存", "临时缓存与下载内容"),
            ],
        },
        Profile {
            id: "pi",
            name: "Pi",
            description: "Pi coding agent 的配置、模型、扩展与会话",
            category: "AI 编程",
            commands: &["pi"],
            paths: vec![
                path_of(
                    home.join(".pi"),
                    "配置与会话",
                    "登录凭证、模型清单、扩展与会话记录",
                ),
                path_of(xdg_config.join("pi"), "配置", "CLI 配置"),
                path_of(xdg_data.join("pi"), "数据", "本地数据与缓存"),
                path_of(config.join("pi"), "配置", "CLI 配置"),
                path_of(data.join("pi"), "数据", "本地数据与缓存"),
            ],
        },
        Profile {
            id: "gemini",
            name: "Gemini CLI",
            description: "Google Gemini CLI 的账号、配置与历史记录",
            category: "AI 编程",
            commands: &["gemini"],
            paths: vec![
                path_of(
                    home.join(".gemini"),
                    "配置与会话",
                    "登录凭证、配置与聊天历史",
                ),
                path_of(xdg_config.join("gemini"), "配置", "CLI 配置"),
                path_of(config.join("gemini"), "配置", "CLI 配置"),
            ],
        },
        Profile {
            id: "qwen",
            name: "Qwen Code",
            description: "Qwen Code CLI 的配置与本地数据",
            category: "AI 编程",
            commands: &["qwen"],
            paths: vec![
                path_of(home.join(".qwen"), "配置与会话", "登录凭证、配置与会话历史"),
                path_of(xdg_config.join("qwen"), "配置", "CLI 配置"),
            ],
        },
        Profile {
            id: "copilot",
            name: "GitHub Copilot CLI",
            description: "Copilot CLI 的配置、登录信息与缓存",
            category: "AI 编程",
            commands: &["copilot", "github-copilot"],
            paths: vec![
                path_of(
                    home.join(".copilot"),
                    "配置与会话",
                    "登录凭证、配置与会话历史",
                ),
                path_of(
                    xdg_config.join("github-copilot"),
                    "配置",
                    "CLI 配置与登录状态",
                ),
                path_of(config.join("github-copilot"), "配置", "CLI 配置与登录状态"),
            ],
        },
        Profile {
            id: "amp",
            name: "Amp",
            description: "Amp CLI 的配置与会话数据",
            category: "AI 编程",
            commands: &["amp"],
            paths: vec![
                path_of(home.join(".amp"), "配置与会话", "登录凭证、配置与会话历史"),
                path_of(xdg_config.join("amp"), "配置", "CLI 配置"),
            ],
        },
        Profile {
            id: "aider",
            name: "Aider",
            description: "Aider 的配置文件、聊天记录与输入历史",
            category: "AI 编程",
            commands: &["aider"],
            paths: vec![
                path_of(home.join(".aider"), "配置与会话", "本地配置与会话数据"),
                path_of(
                    home.join(".aider.conf.yml"),
                    "配置文件",
                    "模型、API 与行为配置",
                ),
                path_of(
                    home.join(".aider.chat.history.md"),
                    "聊天记录",
                    "历史对话内容",
                ),
                path_of(
                    home.join(".aider.input.history"),
                    "输入历史",
                    "命令行输入记录",
                ),
            ],
        },
        Profile {
            id: "continue",
            name: "Continue",
            description: "Continue 插件的配置、索引与聊天记录",
            category: "AI 编程",
            commands: &["continue"],
            paths: vec![
                path_of(
                    home.join(".continue"),
                    "配置与会话",
                    "模型配置、索引与聊天记录",
                ),
                path_of(xdg_config.join("continue"), "配置", "插件配置"),
                path_of(config.join("continue"), "配置", "插件配置"),
            ],
        },
        Profile {
            id: "trae",
            name: "Trae",
            description: "Trae AI IDE 的配置、扩展、工作区与会话缓存",
            category: "AI 编程",
            commands: &["trae"],
            paths: vec![
                path_of(
                    home.join(".trae"),
                    "配置与会话",
                    "CLI 配置、登录状态与本地数据",
                ),
                path_of(xdg_config.join("Trae"), "配置", "编辑器设置与快捷键"),
                path_of(config.join("Trae"), "配置", "编辑器设置与快捷键"),
            ],
        },
        /* ------------------------------- 编辑器 ------------------------------- */
        Profile {
            id: "cursor",
            name: "Cursor",
            description: "Cursor 编辑器的配置、扩展、CLI 数据与会话",
            category: "编辑器",
            commands: &["cursor"],
            paths: vec![
                path_of(
                    home.join(".cursor"),
                    "配置与会话",
                    "CLI 配置、登录状态与会话数据",
                ),
                path_of(
                    xdg_data.join("cursor-agent"),
                    "CLI 数据",
                    "Cursor Agent CLI 的配置与会话",
                ),
                path_of(xdg_config.join("Cursor"), "配置", "编辑器设置与快捷键"),
                path_of(config.join("Cursor"), "配置", "编辑器设置与快捷键"),
            ],
        },
        Profile {
            id: "zed",
            name: "Zed",
            description: "Zed 编辑器的配置、扩展与本地状态",
            category: "编辑器",
            commands: &["zed"],
            paths: vec![
                path_of(xdg_config.join("zed"), "配置", "编辑器设置、主题与快捷键"),
                path_of(home.join(".zed"), "配置与会话", "登录状态、会话与本地状态"),
                path_of(config.join("zed"), "配置", "编辑器设置"),
            ],
        },
        Profile {
            id: "windsurf",
            name: "Windsurf",
            description: "Windsurf 编辑器的配置、扩展与缓存",
            category: "编辑器",
            commands: &["windsurf"],
            paths: vec![
                path_of(
                    home.join(".codeium/windsurf"),
                    "配置与会话",
                    "登录状态、会话与本地数据",
                ),
                path_of(xdg_config.join("Windsurf"), "配置", "编辑器设置与快捷键"),
                path_of(config.join("Windsurf"), "配置", "编辑器设置与快捷键"),
            ],
        },
        Profile {
            id: "vscode",
            name: "Visual Studio Code",
            description: "VS Code 的配置、扩展、工作区状态与缓存",
            category: "编辑器",
            commands: &["code"],
            paths: vec![
                path_of(
                    home.join(".vscode"),
                    "扩展与 CLI",
                    "扩展目录、CLI 与命令行工具",
                ),
                path_of(xdg_config.join("Code"), "配置", "用户设置、快捷键与片段"),
                path_of(
                    config.join("Code"),
                    "应用数据",
                    "用户设置、扩展与工作区状态",
                ),
            ],
        },
        Profile {
            id: "jetbrains",
            name: "JetBrains IDEs",
            description: "IntelliJ IDEA、PyCharm 等 JetBrains IDE 的配置、插件与缓存",
            category: "编辑器",
            commands: &[],
            paths: vec![
                path_of(
                    support
                        .clone()
                        .unwrap_or_else(|| home.join("Library/Application Support"))
                        .join("JetBrains"),
                    "配置与插件",
                    "IDE 设置、插件与授权信息",
                ),
                path_of(
                    app_cache.join("JetBrains"),
                    "缓存",
                    "索引、日志与编译器缓存",
                ),
                path_of(xdg_config.join("JetBrains"), "配置", "IDE 设置与插件"),
                path_of(xdg_data.join("JetBrains"), "数据", "本地数据与索引"),
                path_of(xdg_cache.join("JetBrains"), "缓存", "索引与编译缓存"),
            ],
        },
        Profile {
            id: "neovim",
            name: "Neovim",
            description: "Neovim 的配置、插件、运行时状态与缓存",
            category: "编辑器",
            commands: &["nvim"],
            paths: vec![
                path_of(xdg_config.join("nvim"), "配置", "init.lua 与插件配置"),
                path_of(
                    xdg_data.join("nvim"),
                    "插件与数据",
                    "插件、shada 与运行时数据",
                ),
                path_of(
                    home.join(".local/state/nvim"),
                    "状态",
                    "会话、撤销历史与日志",
                ),
                path_of(xdg_cache.join("nvim"), "缓存", "LSP、treesitter 等缓存"),
            ],
        },
        Profile {
            id: "vim",
            name: "Vim",
            description: "Vim 的配置、插件与编辑历史",
            category: "编辑器",
            commands: &["vim"],
            paths: vec![
                path_of(home.join(".vim"), "配置与插件", "vimrc 片段、插件与配色"),
                path_of(home.join(".vimrc"), "配置文件", "Vim 主配置"),
                path_of(home.join(".viminfo"), "编辑历史", "搜索、命令与文件历史"),
            ],
        },
        Profile {
            id: "emacs",
            name: "Emacs",
            description: "Emacs 的配置、插件与本地数据",
            category: "编辑器",
            commands: &["emacs"],
            paths: vec![
                path_of(
                    home.join(".emacs.d"),
                    "配置与插件",
                    "init 配置、elpa 插件与缓存",
                ),
                path_of(xdg_config.join("emacs"), "配置", "init 配置"),
                path_of(xdg_data.join("emacs"), "数据", "elpa 插件与本地数据"),
            ],
        },
        /* ---------------------------- 终端与 Shell ---------------------------- */
        Profile {
            id: "zsh",
            name: "Zsh",
            description: "Zsh 的命令历史、Oh My Zsh 与补全缓存",
            category: "终端与 Shell",
            commands: &["zsh"],
            paths: vec![
                path_of(
                    home.join(".oh-my-zsh"),
                    "框架与插件",
                    "Oh My Zsh 主题与插件",
                ),
                path_of(home.join(".zsh_history"), "命令历史", "历史命令记录"),
                path_of(home.join(".zsh_sessions"), "会话记录", "终端会话与恢复数据"),
            ],
        },
        Profile {
            id: "fish",
            name: "Fish Shell",
            description: "Fish 的配置、函数、命令历史与通用变量",
            category: "终端与 Shell",
            commands: &["fish"],
            paths: vec![
                path_of(xdg_config.join("fish"), "配置", "config.fish、函数与补全"),
                path_of(xdg_data.join("fish"), "数据与会话", "命令历史与通用变量"),
            ],
        },
        Profile {
            id: "starship",
            name: "Starship",
            description: "Starship 提示符的配置与缓存",
            category: "终端与 Shell",
            commands: &["starship"],
            paths: vec![path_of(
                xdg_config.join("starship.toml"),
                "配置文件",
                "提示符模块与样式配置",
            )],
        },
        Profile {
            id: "tmux",
            name: "tmux",
            description: "tmux 的配置、插件与会话数据",
            category: "终端与 Shell",
            commands: &["tmux"],
            paths: vec![
                path_of(home.join(".tmux.conf"), "配置文件", "tmux 键位与行为配置"),
                path_of(xdg_config.join("tmux"), "配置", "tmux 配置与插件"),
                path_of(xdg_data.join("tmux"), "数据", "插件与 resurrect 会话数据"),
            ],
        },
        Profile {
            id: "atuin",
            name: "Atuin",
            description: "Atuin 的 Shell 历史数据库与同步配置",
            category: "终端与 Shell",
            commands: &["atuin"],
            paths: vec![
                path_of(xdg_config.join("atuin"), "配置", "同步、过滤与 UI 配置"),
                path_of(xdg_data.join("atuin"), "历史数据库", "加密的命令历史数据库"),
            ],
        },
        Profile {
            id: "direnv",
            name: "direnv",
            description: "direnv 的配置与按目录缓存的加载结果",
            category: "终端与 Shell",
            commands: &["direnv"],
            paths: vec![
                path_of(xdg_config.join("direnv"), "配置", "direnv.toml 与白名单"),
                path_of(xdg_data.join("direnv"), "缓存", "各目录的环境变量缓存"),
            ],
        },
        Profile {
            id: "kitty",
            name: "kitty",
            description: "kitty 终端的配置、会话与缓存",
            category: "终端与 Shell",
            commands: &["kitty"],
            paths: vec![
                path_of(xdg_config.join("kitty"), "配置", "主题、快捷键与会话配置"),
                path_of(xdg_cache.join("kitty"), "缓存", "字体与图片缓存"),
            ],
        },
        /* ------------------------------ 版本控制 ------------------------------ */
        Profile {
            id: "git",
            name: "Git",
            description: "Git 的全局配置、凭证与忽略规则",
            category: "版本控制",
            commands: &["git"],
            paths: vec![
                path_of(
                    home.join(".gitconfig"),
                    "配置文件",
                    "用户名、别名与全局选项",
                ),
                path_of(
                    home.join(".git-credentials"),
                    "凭证",
                    "HTTPS 远程仓库的登录凭证",
                ),
                path_of(xdg_config.join("git"), "配置", "忽略规则与附加配置"),
            ],
        },
        Profile {
            id: "gh",
            name: "GitHub CLI",
            description: "GitHub CLI 的登录凭证与配置",
            category: "版本控制",
            commands: &["gh"],
            paths: vec![
                path_of(
                    xdg_config.join("gh"),
                    "配置与凭证",
                    "账号登录状态与 hosts 配置",
                ),
                path_of(xdg_data.join("gh"), "数据", "本地缓存与扩展数据"),
            ],
        },
        /* ------------------------------ 开发工具 ------------------------------ */
        Profile {
            id: "docker",
            name: "Docker",
            description: "Docker CLI 的登录凭证、上下文与守护进程配置",
            category: "开发工具",
            commands: &["docker"],
            paths: vec![path_of(
                home.join(".docker"),
                "配置与凭证",
                "登录凭证、构建缓存与上下文配置",
            )],
        },
        Profile {
            id: "kubernetes",
            name: "Kubernetes",
            description: "kubectl 的集群凭证、上下文与缓存",
            category: "开发工具",
            commands: &["kubectl"],
            paths: vec![path_of(
                home.join(".kube"),
                "配置与凭证",
                "集群凭证、上下文与 discovery 缓存",
            )],
        },
        Profile {
            id: "aws",
            name: "AWS CLI",
            description: "AWS CLI 的账号凭证、区域配置与缓存",
            category: "开发工具",
            commands: &["aws"],
            paths: vec![
                path_of(
                    home.join(".aws"),
                    "配置与凭证",
                    "访问密钥、配置文件与 SSO 缓存",
                ),
                path_of(home.join(".aws/cli/cache"), "缓存", "临时凭证与分页缓存"),
            ],
        },
        Profile {
            id: "gcloud",
            name: "Google Cloud CLI",
            description: "gcloud 的账号凭证、项目配置与日志",
            category: "开发工具",
            commands: &["gcloud"],
            paths: vec![path_of(
                xdg_config.join("gcloud"),
                "配置与凭证",
                "账号、项目配置、凭证与运行日志",
            )],
        },
        Profile {
            id: "azure",
            name: "Azure CLI",
            description: "az 的登录状态、配置与命令历史",
            category: "开发工具",
            commands: &["az"],
            paths: vec![path_of(
                home.join(".azure"),
                "配置与凭证",
                "登录状态、订阅配置与命令历史",
            )],
        },
        Profile {
            id: "terraform",
            name: "Terraform",
            description: "Terraform 的插件缓存、凭证与 CLI 配置",
            category: "开发工具",
            commands: &["terraform"],
            paths: vec![
                path_of(
                    home.join(".terraform.d"),
                    "插件与凭证",
                    "Provider 插件缓存与凭证",
                ),
                path_of(xdg_config.join("terraform"), "配置", "Terraform CLI 配置"),
                path_of(xdg_data.join("terraform"), "数据", "插件与本地数据"),
            ],
        },
        Profile {
            id: "apifox",
            name: "Apifox",
            description: "Apifox 桌面客户端的项目缓存、登录状态与更新包",
            category: "开发工具",
            commands: &[],
            paths: vec![path_of(
                support
                    .clone()
                    .unwrap_or_else(|| home.join("Library/Application Support"))
                    .join("Apifox"),
                "应用数据",
                "项目缓存、登录状态与更新包",
            )],
        },
        /* ---------------------------- 语言与包管理 ---------------------------- */
        Profile {
            id: "npm",
            name: "npm",
            description: "npm 的全局配置、缓存与 npx 临时包",
            category: "语言与包管理",
            commands: &["npm"],
            paths: vec![
                path_of(home.join(".npm"), "缓存", "包缓存与 npx 临时安装"),
                path_of(
                    home.join(".npmrc"),
                    "配置文件",
                    "registry、token 与全局选项",
                ),
            ],
        },
        Profile {
            id: "pnpm",
            name: "pnpm",
            description: "pnpm 的全局包、存储与缓存",
            category: "语言与包管理",
            commands: &["pnpm"],
            paths: vec![
                path_of(home.join("Library/pnpm"), "全局包", "全局安装的 CLI"),
                path_of(
                    xdg_data.join("pnpm"),
                    "全局包与存储",
                    "全局包与内容寻址存储",
                ),
                path_of(xdg_config.join("pnpm"), "配置", "rc 配置与全局脚本"),
                path_of(app_cache.join("pnpm"), "缓存", "包元数据与下载缓存"),
                path_of(xdg_cache.join("pnpm"), "缓存", "包元数据与下载缓存"),
            ],
        },
        Profile {
            id: "yarn",
            name: "Yarn",
            description: "Yarn 的全局包、配置与缓存",
            category: "语言与包管理",
            commands: &["yarn"],
            paths: vec![
                path_of(home.join(".yarn"), "全局包与缓存", "全局包、离线镜像与缓存"),
                path_of(home.join(".yarnrc.yml"), "配置文件", "镜像源与插件配置"),
                path_of(app_cache.join("Yarn"), "缓存", "包下载缓存"),
                path_of(xdg_cache.join("yarn"), "缓存", "包下载缓存"),
            ],
        },
        Profile {
            id: "bun",
            name: "Bun",
            description: "Bun 的包缓存与全局配置",
            category: "语言与包管理",
            commands: &["bun"],
            paths: vec![
                path_of(home.join(".bun/install/cache"), "缓存", "包下载缓存"),
                path_of(home.join(".bunfig.toml"), "配置文件", "镜像源与运行配置"),
            ],
        },
        Profile {
            id: "cargo",
            name: "Cargo",
            description: "Cargo 的依赖源码、下载缓存与凭证",
            category: "语言与包管理",
            commands: &["cargo"],
            paths: vec![
                path_of(
                    home.join(".cargo/registry"),
                    "依赖缓存",
                    "crates 源码与下载缓存",
                ),
                path_of(home.join(".cargo/git"), "Git 依赖", "Git 依赖的本地检出"),
                path_of(
                    home.join(".cargo/config.toml"),
                    "配置文件",
                    "镜像源与构建配置",
                ),
                path_of(
                    home.join(".cargo/credentials.toml"),
                    "凭证",
                    "crates.io 发布凭证",
                ),
            ],
        },
        Profile {
            id: "rustup",
            name: "Rustup",
            description: "Rustup 的下载缓存、临时文件与工具链设置",
            category: "语言与包管理",
            commands: &["rustup"],
            paths: vec![
                path_of(home.join(".rustup/downloads"), "下载缓存", "工具链下载缓存"),
                path_of(home.join(".rustup/tmp"), "临时文件", "安装过程中的临时文件"),
                path_of(
                    home.join(".rustup/settings.toml"),
                    "配置文件",
                    "默认工具链与镜像设置",
                ),
            ],
        },
        Profile {
            id: "go",
            name: "Go",
            description: "Go 的模块缓存与工具链配置",
            category: "语言与包管理",
            commands: &["go"],
            paths: vec![
                path_of(
                    home.join("go/pkg/mod"),
                    "模块缓存",
                    "下载的依赖模块与校验数据",
                ),
                path_of(xdg_config.join("go"), "配置", "环境变量与工具配置"),
            ],
        },
        Profile {
            id: "pip",
            name: "pip",
            description: "pip 的下载缓存与镜像配置",
            category: "语言与包管理",
            commands: &["pip", "pip3"],
            paths: vec![
                path_of(app_cache.join("pip"), "缓存", "wheel 与源码包下载缓存"),
                path_of(xdg_cache.join("pip"), "缓存", "wheel 与源码包下载缓存"),
                path_of(xdg_config.join("pip"), "配置文件", "镜像源与全局选项"),
            ],
        },
        Profile {
            id: "uv",
            name: "uv",
            description: "uv 的包缓存、Python 版本与配置",
            category: "语言与包管理",
            commands: &["uv"],
            paths: vec![
                path_of(xdg_cache.join("uv"), "缓存", "wheel、源码包与构建缓存"),
                path_of(xdg_config.join("uv"), "配置文件", "镜像源与解析策略"),
                path_of(xdg_data.join("uv"), "数据", "本地 Python 安装与工具"),
            ],
        },
        Profile {
            id: "maven",
            name: "Maven",
            description: "Maven 的本地仓库与全局配置",
            category: "语言与包管理",
            commands: &["mvn"],
            paths: vec![path_of(
                home.join(".m2"),
                "本地仓库",
                "下载的依赖 jar 与 settings.xml",
            )],
        },
        Profile {
            id: "gradle",
            name: "Gradle",
            description: "Gradle 的依赖缓存、守护进程与构建缓存",
            category: "语言与包管理",
            commands: &["gradle"],
            paths: vec![path_of(
                home.join(".gradle"),
                "缓存与配置",
                "依赖缓存、wrapper 与构建缓存",
            )],
        },
        Profile {
            id: "composer",
            name: "Composer",
            description: "Composer 的包缓存、全局依赖与凭证",
            category: "语言与包管理",
            commands: &["composer"],
            paths: vec![
                path_of(home.join(".composer"), "缓存与全局包", "包缓存与全局依赖"),
                path_of(xdg_config.join("composer"), "配置文件", "镜像源与凭证"),
            ],
        },
        Profile {
            id: "rubygems",
            name: "RubyGems / Bundler",
            description: "Ruby 的 gem 安装、文档与 Bundler 配置",
            category: "语言与包管理",
            commands: &["gem", "bundle"],
            paths: vec![
                path_of(home.join(".gem"), "Gem 数据", "已安装 gem、文档与规格"),
                path_of(home.join(".bundle"), "配置文件", "Bundler 镜像源与路径设置"),
            ],
        },
        Profile {
            id: "conda",
            name: "Conda",
            description: "Conda 的环境、包缓存与全局配置",
            category: "语言与包管理",
            commands: &["conda"],
            paths: vec![
                path_of(home.join(".conda"), "环境与缓存", "环境列表、包缓存与配置"),
                path_of(home.join(".condarc"), "配置文件", "镜像源与环境选项"),
            ],
        },
        Profile {
            id: "dart",
            name: "Dart / Flutter",
            description: "Dart 与 Flutter 的包缓存和工具配置",
            category: "语言与包管理",
            commands: &["dart", "flutter"],
            paths: vec![
                path_of(home.join(".pub-cache"), "包缓存", "pub 依赖与编译缓存"),
                path_of(xdg_config.join("dart"), "配置", "Dart 工具配置"),
            ],
        },
        Profile {
            id: "android",
            name: "Android SDK",
            description: "Android 开发工具的 AVD、调试密钥与 SDK 配置",
            category: "语言与包管理",
            commands: &["adb"],
            paths: vec![path_of(
                home.join(".android"),
                "SDK 配置",
                "模拟器、调试密钥与 SDK 设置",
            )],
        },
    ];

    if let Some(support) = &support {
        for profile in list.iter_mut() {
            match profile.id {
                "codex" => profile.paths.push(path_of(
                    support.join("Codex"),
                    "桌面应用数据",
                    "桌面应用的配置与本地数据",
                )),
                "cursor" => profile.paths.push(path_of(
                    support.join("Cursor"),
                    "应用数据",
                    "扩展、工作区状态与本地缓存",
                )),
                "windsurf" => profile.paths.push(path_of(
                    support.join("Windsurf"),
                    "应用数据",
                    "扩展、工作区状态与本地缓存",
                )),
                "zed" => profile.paths.push(path_of(
                    support.join("Zed"),
                    "应用数据",
                    "工作区状态与本地缓存",
                )),
                _ => {}
            }
        }
    }

    // 同一台机器上 config_dir 可能正好等于 ~/.config，去重避免重复统计。
    for profile in &mut list {
        let mut seen = HashSet::new();
        profile
            .paths
            .retain(|spec| seen.insert(spec.path.to_string_lossy().to_string()));
    }

    list
}

fn executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// 把 `base/*/{suffix}` 形式的目录加入探测列表，用于 fnm / nvm / asdf 这类
/// 按版本分目录的包管理器。
fn push_version_bins(dirs: &mut Vec<PathBuf>, base: &Path, suffix: &[&str]) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let mut path = entry.path();
        for part in suffix {
            path.push(part);
        }
        if path.is_dir() {
            dirs.push(path);
        }
    }
}

fn command_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    let home = home();
    for extra in [
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".deno/bin"),
        home.join(".npm-global/bin"),
        home.join(".cargo/bin"),
        home.join("go/bin"),
        home.join("Library/pnpm"),
        home.join(".local/share/pnpm"),
        home.join(".volta/bin"),
        home.join(".asdf/shims"),
        home.join(".local/share/mise/shims"),
        home.join(".local/share/fnm/aliases/default/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/local/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ] {
        dirs.push(extra);
    }

    // fnm / nvm / asdf / mise 按 Node 版本隔离的全局 bin 目录。
    push_version_bins(
        &mut dirs,
        &home.join(".local/share/fnm/node-versions"),
        &["installation", "bin"],
    );
    push_version_bins(
        &mut dirs,
        &home.join("Library/Application Support/fnm/node-versions"),
        &["installation", "bin"],
    );
    push_version_bins(&mut dirs, &home.join(".nvm/versions/node"), &["bin"]);
    push_version_bins(&mut dirs, &home.join(".asdf/installs/nodejs"), &["bin"]);
    push_version_bins(
        &mut dirs,
        &home.join(".local/share/mise/installs/node"),
        &["bin"],
    );

    // 编辑器 / IDE 自带的命令行工具。
    for app_bin in [
        "/Applications/Visual Studio Code.app/Contents/Resources/app/bin",
        "/Applications/Cursor.app/Contents/Resources/app/bin",
        "/Applications/Windsurf.app/Contents/Resources/app/bin",
        "/Applications/Trae.app/Contents/Resources/app/bin",
    ] {
        dirs.push(PathBuf::from(app_bin));
    }

    let mut seen = HashSet::new();
    dirs.retain(|dir| seen.insert(dir.to_string_lossy().to_string()));
    dirs
}

/// 只给有图形界面的工具用：即使没装 CLI，也能通过应用包判断是否仍在使用。
fn app_names(id: &str) -> &'static [&'static str] {
    match id {
        "cursor" => &["Cursor"],
        "zed" => &["Zed"],
        "windsurf" => &["Windsurf"],
        "vscode" => &["Visual Studio Code", "VSCodium"],
        "trae" => &["Trae", "Trae CN", "Trae ACP Gateway"],
        "apifox" => &["Apifox"],
        "kitty" => &["kitty"],
        "jetbrains" => &[
            "IntelliJ IDEA",
            "IntelliJ IDEA CE",
            "PyCharm",
            "PyCharm CE",
            "WebStorm",
            "GoLand",
            "CLion",
            "DataGrip",
            "Rider",
            "RubyMine",
            "PhpStorm",
            "Android Studio",
        ],
        _ => &[],
    }
}

/// 检查 macOS 的 /Applications 与 ~/Applications 中是否存在对应应用包。
fn app_installed(names: &[&str]) -> bool {
    if names.is_empty() {
        return false;
    }
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    for root in roots {
        for name in names {
            if root.join(format!("{name}.app")).is_dir() {
                return true;
            }
        }
    }
    false
}

fn command_available(dirs: &[PathBuf], names: &[&str]) -> bool {
    let extensions: &[&str] = if cfg!(windows) {
        &["exe", "cmd", "bat", ""]
    } else {
        &[""]
    };
    for name in names {
        for dir in dirs {
            for extension in extensions {
                let candidate = if extension.is_empty() {
                    dir.join(name)
                } else {
                    dir.join(format!("{name}.{extension}"))
                };
                if executable(&candidate) {
                    return true;
                }
            }
        }
    }
    false
}

/// One traversal per path that returns size, file count and the newest mtime.
fn path_stats(path: &Path) -> (u64, u64, i64) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (0, 0, 0);
    };
    let root_modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    if !meta.is_dir() {
        return (allocated_size(&meta), 1, root_modified);
    }

    let mut size = 0u64;
    let mut files = 0u64;
    let mut modified = root_modified;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.depth() == 0 {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            size = size.saturating_add(allocated_size(&meta));
            files += 1;
            if let Ok(time) = meta.modified() {
                if let Ok(duration) = time.duration_since(std::time::UNIX_EPOCH) {
                    modified = modified.max(duration.as_secs() as i64);
                }
            }
        }
    }
    (size, files, modified)
}

/// Scans all known tools in parallel; only tools with at least one existing
/// path are returned, sorted by disk usage.
pub fn scan() -> Vec<ConfigTool> {
    let dirs = command_dirs();
    let mut tools: Vec<ConfigTool> = profiles()
        .into_par_iter()
        .filter_map(|profile| {
            let mut found: Vec<(ConfigPathInfo, i64)> = profile
                .paths
                .into_iter()
                .filter(|spec| spec.path.exists())
                .map(|spec| {
                    let (size, file_count, modified) = path_stats(&spec.path);
                    (
                        ConfigPathInfo {
                            display: display_path(&spec.path),
                            path: spec.path.to_string_lossy().to_string(),
                            kind: spec.kind.to_string(),
                            description: spec.description.to_string(),
                            safety: safety_for(&spec.path, spec.kind).to_string(),
                            is_dir: spec.path.is_dir(),
                            size,
                            file_count,
                            modified,
                        },
                        modified,
                    )
                })
                .collect();
            if found.is_empty() {
                return None;
            }
            found.sort_by(|a, b| b.0.size.cmp(&a.0.size));
            let modified = found.iter().map(|(_, time)| *time).max().unwrap_or(0);
            let paths: Vec<ConfigPathInfo> = found.into_iter().map(|(item, _)| item).collect();
            Some(ConfigTool {
                id: profile.id.to_string(),
                name: profile.name.to_string(),
                description: profile.description.to_string(),
                category: profile.category.to_string(),
                has_cli: !profile.commands.is_empty(),
                command_available: command_available(&dirs, profile.commands),
                app_installed: app_installed(app_names(profile.id)),
                total_size: paths.iter().map(|item| item.size).sum(),
                total_files: paths.iter().map(|item| item.file_count).sum(),
                modified,
                paths,
            })
        })
        .collect();
    tools.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    tools
}

fn find_profile(id: &str) -> Option<Profile> {
    profiles().into_iter().find(|profile| profile.id == id)
}

/// Directory browsing for the detail modal. `path = None` lists the tool's
/// config roots; a directory below a root can then be drilled into.
pub fn detail(tool_id: &str, path: Option<&str>) -> Result<CleanDetail, String> {
    let profile = find_profile(tool_id).ok_or_else(|| format!("未知的配置项：{tool_id}"))?;
    let roots: Vec<PathBuf> = profile
        .paths
        .into_iter()
        .map(|spec| spec.path)
        .filter(|path| path.exists())
        .collect();
    if roots.is_empty() {
        return Err("该工具没有检测到配置文件".into());
    }
    let root_paths: Vec<String> = roots
        .iter()
        .map(|root| root.to_string_lossy().to_string())
        .collect();

    let Some(target) = path else {
        let mut entries: Vec<CleanEntry> = roots
            .iter()
            .map(|root| {
                let (size, file_count, _) = path_stats(root);
                CleanEntry {
                    name: display_path(root),
                    path: root.to_string_lossy().to_string(),
                    is_dir: root.is_dir(),
                    size,
                    file_count,
                }
            })
            .collect();
        entries.sort_by(|a, b| b.size.cmp(&a.size));
        let total_size = entries.iter().map(|entry| entry.size).sum();
        let file_count = entries.iter().map(|entry| entry.file_count).sum();
        return Ok(CleanDetail {
            item_id: profile.id.to_string(),
            label: profile.name.to_string(),
            current: None,
            current_display: "全部位置".into(),
            roots: root_paths,
            roots_display: roots.iter().map(|root| display_path(root)).collect(),
            crumbs: vec![CleanCrumb {
                label: "全部位置".into(),
                path: None,
            }],
            total_size,
            file_count,
            entries,
            truncated: false,
            errors: 0,
        });
    };

    let target = expand_tilde(target.trim());
    if !roots.iter().any(|root| target.starts_with(root)) {
        return Err("路径不在该工具的配置范围内".into());
    }
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }

    let root = roots
        .iter()
        .filter(|root| target.starts_with(root))
        .max_by_key(|root| root.components().count())
        .cloned()
        .expect("target is inside at least one root");

    // 单文件配置（如 ~/.claude.json）直接作为一条明细返回，避免点击根路径出错。
    if !target.is_dir() {
        let (size, file_count, _) = path_stats(&target);
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| display_path(&target));
        return Ok(CleanDetail {
            item_id: profile.id.to_string(),
            label: profile.name.to_string(),
            current_display: display_path(&target),
            current: Some(target.to_string_lossy().to_string()),
            roots: root_paths,
            roots_display: roots.iter().map(|root| display_path(root)).collect(),
            crumbs: vec![
                CleanCrumb {
                    label: "全部位置".into(),
                    path: None,
                },
                CleanCrumb {
                    label: display_path(&root),
                    path: Some(root.to_string_lossy().to_string()),
                },
            ],
            total_size: size,
            file_count,
            entries: vec![CleanEntry {
                name,
                path: target.to_string_lossy().to_string(),
                is_dir: false,
                size,
                file_count,
            }],
            truncated: false,
            errors: 0,
        });
    }

    let (total_size, file_count, entries, truncated, errors) =
        crate::rules::dir_detail_entries(&target);

    let mut crumbs = vec![
        CleanCrumb {
            label: "全部位置".into(),
            path: None,
        },
        CleanCrumb {
            label: display_path(&root),
            path: Some(root.to_string_lossy().to_string()),
        },
    ];
    if target != root {
        if let Ok(relative) = target.strip_prefix(&root) {
            let mut accumulated = root.clone();
            for part in relative.components() {
                accumulated.push(part.as_os_str());
                crumbs.push(CleanCrumb {
                    label: part.as_os_str().to_string_lossy().to_string(),
                    path: Some(accumulated.to_string_lossy().to_string()),
                });
            }
        }
    }

    Ok(CleanDetail {
        item_id: profile.id.to_string(),
        label: profile.name.to_string(),
        current_display: display_path(&target),
        current: Some(target.to_string_lossy().to_string()),
        roots: root_paths,
        roots_display: roots.iter().map(|root| display_path(root)).collect(),
        crumbs,
        total_size,
        file_count,
        entries,
        truncated,
        errors,
    })
}

#[tauri::command(async)]
pub fn scan_configs() -> Vec<ConfigTool> {
    scan()
}

#[tauri::command(async)]
pub fn config_detail(tool_id: String, path: Option<String>) -> Result<CleanDetail, String> {
    detail(&tool_id, path.as_deref())
}

/// Only paths from the known catalog can be removed. Everything goes to the
/// system trash so a mistaken click stays recoverable.
#[tauri::command(async)]
pub fn delete_config_paths(
    paths: Vec<String>,
    state: State<AppState>,
) -> Result<OperationResult, String> {
    if paths.is_empty() {
        return Err("没有选择要清理的配置".into());
    }
    let allowed: HashMap<String, &'static str> = profiles()
        .into_iter()
        .flat_map(|profile| profile.paths)
        .map(|spec| {
            let safety = safety_for(&spec.path, spec.kind);
            (spec.path.to_string_lossy().to_string(), safety)
        })
        .collect();

    let mut result = OperationResult::default();
    for raw in paths {
        let path = expand_tilde(&raw);
        let key = path.to_string_lossy().to_string();
        let Some(safety) = allowed.get(&key) else {
            result
                .errors
                .push(format!("跳过不在配置目录清单中的路径：{raw}"));
            continue;
        };
        if *safety == "keep" {
            result.errors.push(format!("该路径受保护，已跳过：{raw}"));
            continue;
        }
        if !path.exists() {
            result.errors.push(format!("路径不存在：{raw}"));
            continue;
        }
        let (freed, files, errors) = move_to_trash(&[path]);
        result.freed = result.freed.saturating_add(freed);
        result.files += files;
        // 空目录的 files 为 0，但同样已经移入废纸篓，也要告诉前端立即移除。
        let trashed = errors.is_empty();
        result.errors.extend(errors);
        if trashed {
            result.moved_to_trash = true;
            result.removed.push(raw);
        }
    }

    if !result.removed.is_empty() {
        push_history(
            &state.data_dir,
            HistoryEntry {
                id: String::new(),
                timestamp: now_rfc3339(),
                kind: "config".into(),
                title: format!("配置清理 · {} 项", result.removed.len()),
                freed: result.freed,
                files: result.files,
                detail: result.removed.join("、"),
            },
        );
    }
    Ok(result)
}

/// Removes one entry from the config detail browser. Protected paths (`keep`)
/// stay read-only, and everything goes to the trash so it can be restored.
#[tauri::command(async)]
pub fn delete_config_entry(tool_id: String, path: String) -> Result<EntryTrashResult, String> {
    let profile = find_profile(&tool_id).ok_or_else(|| format!("未知的配置项：{tool_id}"))?;
    let target = expand_tilde(path.trim());
    let spec = profile
        .paths
        .iter()
        .filter(|spec| target.starts_with(&spec.path))
        .max_by_key(|spec| spec.path.components().count())
        .ok_or_else(|| "路径不在该工具的配置范围内".to_string())?;
    if safety_for(&spec.path, spec.kind) == "keep" {
        return Err("该路径受保护，已跳过".into());
    }
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }
    let restorable = trash_entry(&target)?;
    Ok(EntryTrashResult { path, restorable })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safety_of(suffix: &str) -> &'static str {
        profiles()
            .into_iter()
            .flat_map(|profile| profile.paths)
            .find(|spec| spec.path.to_string_lossy().ends_with(suffix))
            .map(|spec| safety_for(&spec.path, spec.kind))
            .unwrap_or_else(|| panic!("没有找到路径后缀 {suffix}"))
    }

    #[test]
    fn classifies_config_path_safety() {
        assert_eq!(safety_of(".oh-my-zsh"), "keep");
        assert_eq!(safety_of(".gitconfig"), "keep");
        assert_eq!(safety_of(".npm"), "safe");
        assert_eq!(safety_of(".cargo/registry"), "safe");
        assert_eq!(safety_of(".aws/cli/cache"), "safe");
        assert_eq!(safety_of(".aws"), "caution");
        assert_eq!(safety_of(".conda"), "caution");
        assert_eq!(safety_of(".gradle"), "caution");
        assert_eq!(safety_of(".zsh_history"), "caution");
        assert_eq!(safety_of(".claude.json"), "caution");
    }
}
