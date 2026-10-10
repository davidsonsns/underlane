//! Standalone build of the watchdog shim, used by the core's tests. The app binary embeds the
//! same shim behind `--cloudflared-shim`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match args.first().map(String::as_str) {
        Some(burrow_core::shim::SHIM_FLAG) => &args[1..],
        _ => &args[..],
    };
    std::process::exit(burrow_core::shim::run(args));
}
