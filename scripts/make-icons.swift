// Draws Underlane's icons with CoreGraphics: the template tray glyph and the app icon source.
// Usage: swift scripts/make-icons.swift <out-dir>
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "src-tauri/icons"

func png(_ size: Int, _ path: String, _ draw: (CGContext, CGFloat) -> Void) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    let ctx = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = ctx
    draw(ctx.cgContext, CGFloat(size))
    NSGraphicsContext.current = nil
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

/// Lane Cut: two circular masses separated by a route dipping beneath the surface.
func lanePath(in r: CGRect) -> CGPath {
    let p = CGMutablePath()
    func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
        CGPoint(x: r.minX + r.width * x / 100, y: r.maxY - r.height * y / 100)
    }
    p.move(to: point(1.588, 37.5))
    p.addCurve(to: point(50, 0), control1: point(7.139, 16.006), control2: point(26.34, 0))
    p.addCurve(to: point(98.412, 37.5), control1: point(73.66, 0), control2: point(92.861, 16.006))
    p.addLine(to: point(76, 37.5))
    p.addCurve(to: point(50, 59), control1: point(64, 37.5), control2: point(65, 59))
    p.addCurve(to: point(24, 37.5), control1: point(35, 59), control2: point(36, 37.5))
    p.closeSubpath()
    p.move(to: point(0, 50))
    p.addLine(to: point(24, 50))
    p.addCurve(to: point(50, 71), control1: point(36, 50), control2: point(35, 71))
    p.addCurve(to: point(76, 50), control1: point(65, 71), control2: point(64, 50))
    p.addLine(to: point(100, 50))
    p.addCurve(to: point(50, 100), control1: point(100, 77.614), control2: point(77.614, 100))
    p.addCurve(to: point(0, 50), control1: point(22.386, 100), control2: point(0, 77.614))
    p.closeSubpath()
    return p
}

// Tray template: black on transparent; macOS tints it. 18pt -> 36px @2x (the only size tray.rs loads).
for (px, name) in [(36, "tray-template@2x.png")] {
    png(px, "\(out)/\(name)") { ctx, s in
        ctx.setFillColor(NSColor.black.cgColor)
        ctx.addPath(lanePath(in: CGRect(x: s * 0.06, y: s * 0.06, width: s * 0.88, height: s * 0.88)))
        ctx.fillPath()
    }
}

// App icon source: the monochrome mark on a warm ivory macOS plate.
png(1024, "\(out)/app-icon.png") { ctx, s in
    let inset = s * 0.09
    let plate = CGRect(x: inset, y: inset, width: s - inset * 2, height: s - inset * 2)
    let path = CGPath(roundedRect: plate, cornerWidth: plate.width * 0.225, cornerHeight: plate.width * 0.225, transform: nil)
    ctx.addPath(path)
    ctx.setFillColor(NSColor(srgbRed: 238 / 255, green: 236 / 255, blue: 229 / 255, alpha: 1).cgColor)
    ctx.fillPath()
    ctx.setFillColor(NSColor(srgbRed: 32 / 255, green: 33 / 255, blue: 31 / 255, alpha: 1).cgColor)
    ctx.addPath(lanePath(in: plate.insetBy(dx: plate.width * 0.19, dy: plate.height * 0.19)))
    ctx.fillPath()
}
print("icons written to \(out)")
