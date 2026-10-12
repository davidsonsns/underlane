// Print the on-screen windows of an app as JSON lines: {"id","x","y","w","h"}.
// Usage: swift scripts/window-bounds.swift <owner-name>
import CoreGraphics
import Foundation
let owner = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "Underlane"
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
for w in list where (w[kCGWindowOwnerName as String] as? String) == owner && (w[kCGWindowLayer as String] as? Int ?? 0) < 25 {
    guard let b = w[kCGWindowBounds as String] as? [String: Double], (b["Height"] ?? 0) > 100 else { continue }
    print("{\"id\":\(w[kCGWindowNumber as String] as? Int ?? 0),\"x\":\(Int(b["X"]!)),\"y\":\(Int(b["Y"]!)),\"w\":\(Int(b["Width"]!)),\"h\":\(Int(b["Height"]!))}")
}
