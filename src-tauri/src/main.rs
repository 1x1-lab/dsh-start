// 所有构建（含 debug）均为 GUI 子系统：开机自启 / 直接运行都不弹黑控制台窗口。
// 日志走文件 + UI 事件，不依赖控制台输出。
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    dsh_start_lib::run()
}
