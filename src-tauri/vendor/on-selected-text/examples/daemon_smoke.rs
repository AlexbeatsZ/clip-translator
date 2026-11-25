use std::{env, io, process::ExitCode};

use on_selected_text::{
    check_permissions, start_selection_daemon, DaemonOptions, ScreenRect, SelectionResult,
};

/// Push daemon smoke 命令入口，用于真实桌面选区事件的手动验证。
fn main() -> ExitCode {
    let opts = match parse_daemon_options(env::args().skip(1)) {
        Ok(opts) => opts,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    println!("permission: {:?}", check_permissions());
    println!("select text in another app; press Enter here to stop the daemon");

    let mut daemon = match start_selection_daemon(opts, |selection| {
        println!("--- selection changed ---");
        print_selection(&selection);
    }) {
        Ok(daemon) => daemon,
        Err(error) => {
            eprintln!("failed to start selection daemon: {error}");
            return ExitCode::from(1);
        }
    };

    let mut line = String::new();
    if let Err(error) = io::stdin().read_line(&mut line) {
        eprintln!("failed to wait for Enter: {error}");
        return ExitCode::from(1);
    }

    daemon.stop();
    ExitCode::SUCCESS
}

/// 解析 smoke 示例支持的 Push daemon 参数。
fn parse_daemon_options(args: impl IntoIterator<Item = String>) -> Result<DaemonOptions, String> {
    let mut opts = DaemonOptions::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bounds" => opts.get_text_options.fetch_bounds = true,
            "--no-clipboard-fallback" => {
                opts.get_text_options.allow_clipboard_fallback = false;
            }
            "--max-wait-ms" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --max-wait-ms".to_string())?;
                opts.get_text_options.max_wait_ms = parse_u64_flag("--max-wait-ms", &value)?;
            }
            "--trigger-delay-ms" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --trigger-delay-ms".to_string())?;
                opts.trigger_delay_ms = parse_u64_flag("--trigger-delay-ms", &value)?;
            }
            "--drag-threshold-px" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --drag-threshold-px".to_string())?;
                opts.drag_threshold_px = parse_f64_flag("--drag-threshold-px", &value)?;
            }
            "-h" | "--help" => return Err("daemon_smoke options:".to_string()),
            unknown => return Err(format!("unknown argument: {unknown}")),
        }
    }

    Ok(opts)
}

/// 解析 u64 类型命令行参数。
fn parse_u64_flag(name: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid value for {name}: {error}"))
}

/// 解析 f64 类型命令行参数。
fn parse_f64_flag(name: &str, value: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid value for {name}: {error}"))
}

/// 打印 smoke 示例用法。
fn print_usage() {
    eprintln!(
        "usage: cargo run --example daemon_smoke -- [--bounds] [--no-clipboard-fallback] [--max-wait-ms N] [--trigger-delay-ms N] [--drag-threshold-px N]"
    );
}

/// 输出选区文本与可选屏幕坐标。
fn print_selection(selection: &SelectionResult) {
    println!("text: {}", selection.text);
    match selection.bounds {
        Some(bounds) => print_bounds(bounds),
        None => println!("bounds: <none>"),
    }
}

/// 输出屏幕坐标。
fn print_bounds(bounds: ScreenRect) {
    println!(
        "bounds: x={} y={} width={} height={}",
        bounds.x, bounds.y, bounds.width, bounds.height
    );
}
