// Screenshot one window of a running app (no desktop, no other apps).
// Usage: swift scripts/capture-window.swift <owner-name> <out.png> [min-width] [max-width] [pid]
// Picks the largest on-screen window owned by <owner-name> at least min-width points wide
// (use a small min-width for the popover). With a pid, only that process's windows count.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 3 else { print("usage: capture-window <owner> <out.png> [min-width] [max-width]"); exit(2) }
let owner = args[1], out = args[2]
let minW = args.count > 3 ? Double(args[3]) ?? 0 : 0
let maxW = args.count > 4 ? Double(args[4]) ?? 1e9 : 1e9
let pid = args.count > 5 ? Int(args[5]) : nil
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]

let candidates = list.filter { w in
    guard (w[kCGWindowOwnerName as String] as? String) == owner,
          let b = w[kCGWindowBounds as String] as? [String: Double],
          (w[kCGWindowLayer as String] as? Int ?? 0) < 1000,
          pid == nil || (w[kCGWindowOwnerPID as String] as? Int) == pid else { return false }
    return (b["Width"] ?? 0) >= minW && (b["Width"] ?? 0) <= maxW && (b["Height"] ?? 0) > 100
}.sorted { a, b in
    let ba = a[kCGWindowBounds as String] as! [String: Double], bb = b[kCGWindowBounds as String] as! [String: Double]
    return ba["Width"]! * ba["Height"]! > bb["Width"]! * bb["Height"]!
}
guard let win = candidates.first, let id = win[kCGWindowNumber as String] as? Int else {
    print("no window for \(owner)" + (pid.map { " (pid \($0))" } ?? ""))
    // Say what was there, so a failed capture can be diagnosed.
    for w in list where (w[kCGWindowOwnerName as String] as? String) == owner {
        print("  seen: pid \(w[kCGWindowOwnerPID as String] ?? "?") layer \(w[kCGWindowLayer as String] ?? "?") bounds \(w[kCGWindowBounds as String] ?? "?")")
    }
    exit(1)
}
let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
p.arguments = ["-x", "-o", "-l", String(id), out]
try! p.run()
p.waitUntilExit()
print("captured window \(id) -> \(out)")
