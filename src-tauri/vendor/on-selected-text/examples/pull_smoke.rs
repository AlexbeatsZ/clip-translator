use std::{env, process::ExitCode};

use on_selected_text::{
    check_permissions, get_selected_text_with_options, GetTextError, GetTextOptions, ScreenRect,
    SelectionResult,
};

/// Pull smoke 命令入口，便于在真实应用里手动验证当前选区读取。
fn main() -> ExitCode {
    let opts = match parse_get_text_options(env::args().skip(1)) {
        Ok(opts) => opts,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    println!("permission: {:?}", check_permissions());
    match get_selected_text_with_options(opts) {
        Ok(selection) => {
            print_selection(&selection);
            ExitCode::SUCCESS
        }
        Err(GetTextError::NoTextSelected) => {
            println!("no text selected");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to read selected text: {error}");
            ExitCode::from(1)
        }
    }
}

/// 解析 smoke 示例支持的 Pull API 参数。
fn parse_get_text_options(
    args: impl IntoIterator<Item = String>,
) -> Result<GetTextOptions, String> {
    let mut opts = GetTextOptions::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bounds" => opts.fetch_bounds = true,
            "--no-clipboard-fallback" => opts.allow_clipboard_fallback = false,
            "--max-wait-ms" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --max-wait-ms".to_string())?;
                opts.max_wait_ms = parse_u64_flag("--max-wait-ms", &value)?;
            }
            "-h" | "--help" => return Err("pull_smoke options:".to_string()),
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

/// 打印 smoke 示例用法。
fn print_usage() {
    eprintln!(
        "usage: cargo run --example pull_smoke -- [--bounds] [--no-clipboard-fallback] [--max-wait-ms N]"
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
