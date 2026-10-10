// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `Burrow --cloudflared-shim <cloudflared> <args...>`: the watchdog that ties cloudflared's
    // lifetime to the app's (see burrow_core::shim). Runs before any UI code.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(burrow_core::shim::SHIM_FLAG) {
        std::process::exit(burrow_core::shim::run(&args[2..]));
    }
    burrow_lib::run();
}
