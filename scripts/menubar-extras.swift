// List (and optionally press) another app's menu bar extras through Accessibility.
// Used to reveal items a menu bar manager (e.g. Hidden Bar) has collapsed before taking
// screenshots. Usage: swift scripts/menubar-extras.swift "<App Name>" [press-index]
import AppKit
import ApplicationServices

let name = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "Hidden Bar"
guard let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == name }) else {
    print("not running: \(name)"); exit(1)
}
let ax = AXUIElementCreateApplication(app.processIdentifier)
var bar: CFTypeRef?
AXUIElementCopyAttributeValue(ax, "AXExtrasMenuBar" as CFString, &bar)
guard let bar else { print("no extras menu bar"); exit(1) }
var kids: CFTypeRef?
AXUIElementCopyAttributeValue(bar as! AXUIElement, kAXChildrenAttribute as CFString, &kids)
let items = (kids as? [AXUIElement]) ?? []
for (i, item) in items.enumerated() {
    var pos: CFTypeRef?, size: CFTypeRef?, title: CFTypeRef?, desc: CFTypeRef?
    AXUIElementCopyAttributeValue(item, kAXPositionAttribute as CFString, &pos)
    AXUIElementCopyAttributeValue(item, kAXSizeAttribute as CFString, &size)
    AXUIElementCopyAttributeValue(item, kAXTitleAttribute as CFString, &title)
    AXUIElementCopyAttributeValue(item, kAXDescriptionAttribute as CFString, &desc)
    var p = CGPoint.zero, s = CGSize.zero
    if let pos { AXValueGetValue(pos as! AXValue, .cgPoint, &p) }
    if let size { AXValueGetValue(size as! AXValue, .cgSize, &s) }
    print(i, "x=\(Int(p.x)) w=\(Int(s.width))", title as? String ?? "", desc as? String ?? "")
}
if CommandLine.arguments.count > 2, let idx = Int(CommandLine.arguments[2]), idx < items.count {
    let r = AXUIElementPerformAction(items[idx], kAXPressAction as CFString)
    print("pressed \(idx): \(r == .success ? "ok" : "error \(r.rawValue)")")
}
