import { reactive } from "vue";

/** 已支持的语言；后续新增语言：往 zh/en 同级加字典 + LOCALES 加一项即可 */
export type Locale = "zh" | "en";

export const LOCALES: { id: Locale; name: string }[] = [
  { id: "zh", name: "中文" },
  { id: "en", name: "English" },
];

const zh = {
  // ===== 导航 / 页头 =====
  "nav.group.hosting": "托管",
  "nav.group.app": "应用",
  "nav.console": "控制台",
  "nav.logs": "日　志",
  "nav.settings": "设　置",
  "head.console.t": "控制台",
  "head.console.s": "DSH 托管进程实时状态",
  "head.logs.t": "日志",
  "head.logs.s": "内存环形缓冲 + 滚动文件",
  "head.settings.t": "设置",
  "head.settings.s": "端口、行为与说明",

  // ===== 状态 =====
  "status.node-missing": "缺少 Node.js",
  "status.installing": "安装中",
  "status.installed-idle": "已安装 · 未启动",
  "status.starting": "启动中",
  "status.running": "运行中",
  "status.external": "运行中 · 外部实例",
  "status.stopped": "已停止",
  "status.crashed": "已崩溃",
  "status.port-in-use": "端口被占用",
  "status.error": "出错",

  // ===== 控制台 =====
  "dash.openConsole": "打开 DSH 控制台",
  "dash.restart": "重启",
  "dash.restarting": "重启中…",
  "dash.stop": "停止",
  "dash.stopping": "停止中…",
  "dash.start": "启动 DSH",
  "dash.starting": "启动中…",
  "dash.updateTo": "更新到 v{v}",
  "dash.upgradeSystem": "升级系统 DSH 到 v{v}",
  "dash.systemUpgraded": "系统 DSH 已升级到 v{v}",
  "dash.updating": "更新中…",
  "dash.forceStop": "强制停止外部实例",
  "dash.confirmStop": "确认强制停止？",
  "dash.forceStopDone": "已发送强制停止，端口释放后状态自动更新",
  "dash.externalHint":
    "检测到端口 {port} 上由外部启动的 DSH（未托管）：如需本应用托管，请先停止外部实例再启动；也可以直接「强制停止外部实例」结束它（按端口反查进程）。",

  // ===== 状态卡 =====
  "card.port": "DSH 端口",
  "card.controlPort": "控制端口",
  "card.version": "版本",
  "card.uptime": "运行时长",
  "card.port.external": "● 外部实例运行中",
  "card.port.listening": "● 本机可访问",
  "card.port.off": "● 未在监听",
  "card.cp.localhost": "● 仅 127.0.0.1",
  "card.cp.unbound": "● 未绑定",
  "card.notInstalled": "未安装",
  "card.systemVersion": "系统 v{v}（未托管）",
  "card.noNode": "未检测到 Node",
  "card.externalProcess": "外部进程",

  // ===== 回调卡 =====
  "cb.title": "回调重启",
  "cb.throttle": "1.5s 节流",
  "cb.copy": "拷贝",
  "cb.copied": "✓ 已复制",
  "cb.unavailable": "不可用：控制端口绑定失败，可在设置中更换端口",
  "cb.note":
    "双通道走同一条重启例程：优雅停止 → 启动 → 就绪探测 → 托盘通知；DSH 的 bash/pwsh 工具可直接执行 CLI 命令。控制端口默认 = DSH 端口 + 1，被占用时自动后移，也可在设置中指定。",
  "cb.noInfo": "回调信息不可用",

  // ===== 设置 =====
  "set.runtime": "运行设置",
  "set.language": "语言 / Language",
  "set.language.sub": "界面语言，保存后生效",
  "set.port": "DSH 端口",
  "set.port.sub": "修改后需重启 DSH 生效",
  "set.controlPort": "控制端口",
  "set.controlPort.sub": "HTTP 回调端点；留空 = DSH 端口 + 1，被占用时自动后移，保存后立即生效",
  "set.dshVersion": "DSH 版本",
  "set.dshVersion.sub": "latest 或指定版本号，如 0.1.0-rc.6",
  "set.save": "保存设置",
  "set.saved": "✓ 已保存",
  "set.behavior": "行为",
  "set.quitStops": "退出时停止 DSH",
  "set.quitStops.desc": "退出 DSH Start 时同时停止托管的 DSH 进程",
  "set.registerCli": "注册回调命令",
  "set.registerCli.desc": "DSH 的 bash/pwsh 工具可执行 dsh-start restart",
  "set.saveNote": "「开机启动」即时生效，其余选项随「保存设置」保存后生效。",
  "set.automation": "自动化",
  "set.autostart": "开机启动",
  "set.autostart.desc": "登录时自动拉起本应用并启动 DSH",
  "set.crashRestart": "崩溃自动重启",
  "set.crashRestart.desc": "意外退出时指数退避重启，最多 5 次",
  "set.about": "说明",
  "set.about.text":
    "DSH 由本应用托管安装到独立运行时目录，不影响全局 npm 环境。回调重启支持 HTTP（POST /api/restart）与 CLI（dsh-start restart）双通道，控制端点仅绑定 127.0.0.1，CORS 仅放行 DSH 网页源。关闭窗口最小化到托盘，退出请使用托盘菜单。",
  "set.err.portInvalid": "控制端口无效（1-65535）",
  "set.err.portSame": "控制端口不能与 DSH 端口相同",

  // ===== 关于 / 版本更新 =====
  "about.title": "关于",
  "about.version": "应用版本",
  "about.dshVersion": "DSH 版本",
  "about.dshLocation": "DSH 安装位置",
  "about.openDir": "打开",
  "about.repo": "源码仓库",
  "about.license": "许可证",
  "about.tech": "技术栈",
  "about.check": "检查更新",
  "about.checking": "检查中…",
  "about.upToDate": "✓ 已是最新版本",
  "about.newVersion": "发现新版本 v{v}",
  "about.openRelease": "打开 Release 页",
  "about.checkFailed": "检查更新失败，请检查网络后重试",
  "about.openRepo": "打开仓库",

  // ===== 日志 =====
  "logs.live": "实时日志 · {n} 条",
  "logs.open": "打开日志文件",
  "logs.clear": "清空显示",
  "logs.empty": "暂无日志",

  // ===== 首次向导 =====
  "wiz.title": "首次使用向导 · DSH 自动安装",
  "wiz.skip": "跳过",
  "wiz.env": "环境检测",
  "wiz.env.desc": "首次使用前先检查运行环境，确认后由你决定是否开始安装 DSH。",
  "wiz.checking": "检测中…",
  "wiz.nodeMissing": "✗ 未检测到",
  "wiz.installNode": "自动安装 Node.js",
  "wiz.installingNode": "安装中…",
  "wiz.dshInstalled": "✓ 已安装 v{v}",
  "wiz.dshMissing": "✗ 未安装",
  "wiz.dshSystem": "✓ 系统已存在 v{v}（将安装独立托管副本）",
  "wiz.env.note": "安装 DSH 将通过 npm 下载依赖，可能需要几分钟；不影响你的全局 npm 环境。",
  "wiz.begin": "开始安装 DSH",
  "wiz.recheck": "重新检测",
  "wiz.installing": "正在安装 DSH（npm install）",
  "wiz.installing.desc": "首次安装需要下载依赖，请稍候；进度实时显示在下方。",
  "wiz.installedOk": "dsh v{v} 安装成功",
  "wiz.done.desc": "DSH 尚未启动。你可以勾选「开机启动」让 DSH 随系统自启，或在控制台点击「启动 DSH」手动启动。",
  "wiz.autostart": "开启开机启动（注册系统自启动并立即启动 DSH）",
  "wiz.finish": "完成",
  "wiz.failed": "❌ 安装失败",
  "wiz.retry": "重试",
  "wiz.back": "返回",
};

export type MsgKey = keyof typeof zh;

const en: Record<MsgKey, string> = {
  "nav.group.hosting": "Hosting",
  "nav.group.app": "App",
  "nav.console": "Console",
  "nav.logs": "Logs",
  "nav.settings": "Settings",
  "head.console.t": "Console",
  "head.console.s": "Live status of the managed DSH process",
  "head.logs.t": "Logs",
  "head.logs.s": "In-memory ring buffer + rolling file",
  "head.settings.t": "Settings",
  "head.settings.s": "Ports, behavior and notes",

  "status.node-missing": "Node.js Missing",
  "status.installing": "Installing",
  "status.installed-idle": "Installed · Idle",
  "status.starting": "Starting",
  "status.running": "Running",
  "status.external": "Running · External",
  "status.stopped": "Stopped",
  "status.crashed": "Crashed",
  "status.port-in-use": "Port In Use",
  "status.error": "Error",

  "dash.openConsole": "Open DSH Console",
  "dash.restart": "Restart",
  "dash.restarting": "Restarting…",
  "dash.stop": "Stop",
  "dash.stopping": "Stopping…",
  "dash.start": "Start DSH",
  "dash.starting": "Starting…",
  "dash.updateTo": "Update to v{v}",
  "dash.upgradeSystem": "Upgrade system DSH to v{v}",
  "dash.systemUpgraded": "System DSH upgraded to v{v}",
  "dash.updating": "Updating…",
  "dash.forceStop": "Force Stop External",
  "dash.confirmStop": "Confirm force stop?",
  "dash.forceStopDone": "Force stop sent; status updates when the port frees",
  "dash.externalHint":
    "An externally started DSH was detected on port {port} (not managed): to let this app manage DSH, stop the external instance first, or use \"Force Stop External\" to kill it (looked up by port).",

  "card.port": "DSH Port",
  "card.controlPort": "Control Port",
  "card.version": "Version",
  "card.uptime": "Uptime",
  "card.port.external": "● External instance running",
  "card.port.listening": "● Reachable locally",
  "card.port.off": "● Not listening",
  "card.cp.localhost": "● 127.0.0.1 only",
  "card.cp.unbound": "● Not bound",
  "card.notInstalled": "Not installed",
  "card.systemVersion": "System v{v} (unmanaged)",
  "card.noNode": "Node not detected",
  "card.externalProcess": "External process",

  "cb.title": "Callback Restart",
  "cb.throttle": "1.5s throttle",
  "cb.copy": "Copy",
  "cb.copied": "✓ Copied",
  "cb.unavailable": "Unavailable: control port bind failed — change it in Settings",
  "cb.note":
    "Both channels run the same restart routine: graceful stop → start → readiness probe → tray notification; DSH's bash/pwsh tools can run the CLI command directly. Control port defaults to DSH port + 1, shifts automatically when occupied, or set it in Settings.",
  "cb.noInfo": "Callback info unavailable",

  "set.runtime": "Runtime",
  "set.language": "Language / 语言",
  "set.language.sub": "UI language, applied on save",
  "set.port": "DSH Port",
  "set.port.sub": "Takes effect after restarting DSH",
  "set.controlPort": "Control Port",
  "set.controlPort.sub":
    "HTTP callback endpoint; empty = DSH port + 1, shifts automatically when occupied, applies immediately on save",
  "set.dshVersion": "DSH Version",
  "set.dshVersion.sub": "latest or a pinned version like 0.1.0-rc.6",
  "set.save": "Save Settings",
  "set.saved": "✓ Saved",
  "set.behavior": "Behavior",
  "set.quitStops": "Stop DSH on Quit",
  "set.quitStops.desc": "Also stop the managed DSH process when DSH Start quits",
  "set.registerCli": "Register Callback Command",
  "set.registerCli.desc": "DSH's bash/pwsh tools can run `dsh-start restart`",
  "set.saveNote": "\"Auto Start\" applies immediately; everything else applies after \"Save Settings\".",
  "set.automation": "Automation",
  "set.autostart": "Auto Start",
  "set.autostart.desc": "Launch this app on login and start DSH automatically",
  "set.crashRestart": "Auto-restart on Crash",
  "set.crashRestart.desc": "Restart with exponential backoff on unexpected exit, up to 5 times",
  "set.about": "About",
  "set.about.text":
    "DSH is installed by this app into an isolated runtime directory and never touches your global npm environment. Callback restart works over two channels — HTTP (POST /api/restart) and CLI (dsh-start restart); the control endpoint binds 127.0.0.1 only, with CORS restricted to the DSH web origin. Closing the window minimizes to tray; quit from the tray menu.",
  "set.err.portInvalid": "Invalid control port (1-65535)",
  "set.err.portSame": "Control port cannot equal the DSH port",

  "about.title": "About",
  "about.version": "App Version",
  "about.dshVersion": "DSH Version",
  "about.dshLocation": "DSH Install Location",
  "about.openDir": "Open",
  "about.repo": "Repository",
  "about.license": "License",
  "about.tech": "Tech Stack",
  "about.check": "Check for Updates",
  "about.checking": "Checking…",
  "about.upToDate": "✓ Up to date",
  "about.newVersion": "New version v{v} available",
  "about.openRelease": "Open Releases",
  "about.checkFailed": "Update check failed — check your network and retry",
  "about.openRepo": "Open Repository",

  "logs.live": "Live Logs · {n}",
  "logs.open": "Open Log File",
  "logs.clear": "Clear View",
  "logs.empty": "No logs yet",

  "wiz.title": "First-run Wizard · DSH Auto Install",
  "wiz.skip": "Skip",
  "wiz.env": "Environment Check",
  "wiz.env.desc": "Before first use we check the runtime environment; you decide whether to install DSH.",
  "wiz.checking": "Checking…",
  "wiz.nodeMissing": "✗ Not detected",
  "wiz.installNode": "Install Node.js",
  "wiz.installingNode": "Installing…",
  "wiz.dshInstalled": "✓ Installed v{v}",
  "wiz.dshMissing": "✗ Not installed",
  "wiz.dshSystem": "✓ Found on system v{v} (an isolated managed copy will be installed)",
  "wiz.env.note": "Installing DSH downloads dependencies via npm and may take a few minutes; your global npm environment is untouched.",
  "wiz.begin": "Install DSH",
  "wiz.recheck": "Re-check",
  "wiz.installing": "Installing DSH (npm install)",
  "wiz.installing.desc": "The first install downloads dependencies — please wait; progress shows below in real time.",
  "wiz.installedOk": "dsh v{v} installed",
  "wiz.done.desc": "DSH is not started yet. Enable \"Auto Start\" to launch it with the system, or click \"Start DSH\" on the console.",
  "wiz.autostart": "Enable Auto Start (register system autostart and start DSH now)",
  "wiz.finish": "Finish",
  "wiz.failed": "❌ Installation failed",
  "wiz.retry": "Retry",
  "wiz.back": "Back",
};

const dicts: Record<Locale, Record<MsgKey, string>> = { zh, en };

/** 当前界面语言（响应式：模板里用 t() 会自动跟随切换） */
export const i18n = reactive({ locale: "zh" as Locale });

export function setLocale(l: string) {
  i18n.locale = l === "en" ? "en" : "zh";
}

/** 取文案；{param} 形式插值；缺 key 回退中文，再回退 key 本身 */
export function t(key: MsgKey, params?: Record<string, string | number>): string {
  let s: string = dicts[i18n.locale][key] ?? zh[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replaceAll(`{${k}}`, String(v));
    }
  }
  return s;
}
