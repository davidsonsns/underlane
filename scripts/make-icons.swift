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

/// The arch mark: an arch opening with a ground line, drawn in a unit square (origin bottom-left).
func archPath(in r: CGRect, stroke: CGFloat) -> CGPath {
    let p = CGMutablePath()
    let w = r.width, h = r.height
    let ground = r.minY + h * 0.18
    let cx = r.midX
    let radius = w * 0.30
    let shoulder = ground + h * 0.30
    // Arch: two legs and a half circle.
    p.move(to: CGPoint(x: cx - radius, y: ground))
    p.addLine(to: CGPoint(x: cx - radius, y: shoulder))
    p.addArc(center: CGPoint(x: cx, y: shoulder), radius: radius, startAngle: .pi, endAngle: 0, clockwise: true)
    p.addLine(to: CGPoint(x: cx + radius, y: ground))
    // Ground line, wider than the arch.
    p.move(to: CGPoint(x: r.minX + w * 0.08, y: ground))
    p.addLine(to: CGPoint(x: r.maxX - w * 0.08, y: ground))
    return p.copy(strokingWithWidth: stroke, lineCap: .round, lineJoin: .round, miterLimit: 4)
}

// Tray template: black on transparent; macOS tints it. 18pt -> 36px @2x (the only size tray.rs loads).
for (px, name) in [(36, "tray-template@2x.png")] {
    png(px, "\(out)/\(name)") { ctx, s in
        ctx.setFillColor(NSColor.black.cgColor)
        ctx.addPath(archPath(in: CGRect(x: 0, y: 0, width: s, height: s), stroke: s * 0.11))
        ctx.fillPath()
        // A small "light at the end of the tunnel" dot.
        let d = s * 0.14
        ctx.fillEllipse(in: CGRect(x: s / 2 - d / 2, y: s * 0.30, width: d, height: d))
    }
}

// App icon source (1024px): macOS squircle-ish plate, soil-to-dusk gradient, warm light in the arch.
png(1024, "\(out)/app-icon.png") { ctx, s in
    let inset = s * 0.09
    let plate = CGRect(x: inset, y: inset, width: s - inset * 2, height: s - inset * 2)
    let path = CGPath(roundedRect: plate, cornerWidth: plate.width * 0.225, cornerHeight: plate.width * 0.225, transform: nil)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -s * 0.012), blur: s * 0.03, color: NSColor(white: 0, alpha: 0.35).cgColor)
    ctx.addPath(path)
    ctx.setFillColor(NSColor.black.cgColor)
    ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(path)
    ctx.clip()
    let colors = [NSColor(srgbRed: 0.10, green: 0.13, blue: 0.20, alpha: 1).cgColor,
                  NSColor(srgbRed: 0.20, green: 0.25, blue: 0.36, alpha: 1).cgColor] as CFArray
    let g = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: colors, locations: [0, 1])!
    ctx.drawLinearGradient(g, start: CGPoint(x: 0, y: plate.minY), end: CGPoint(x: 0, y: plate.maxY), options: [])
    // Glow inside the arch.
    let glow = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB),
                          colors: [NSColor(srgbRed: 1.0, green: 0.86, blue: 0.55, alpha: 0.95).cgColor,
                                   NSColor(srgbRed: 1.0, green: 0.70, blue: 0.35, alpha: 0.0).cgColor] as CFArray,
                          locations: [0, 1])!
    let c = CGPoint(x: s / 2, y: plate.minY + plate.height * 0.40)
    ctx.drawRadialGradient(glow, startCenter: c, startRadius: 0, endCenter: c, endRadius: plate.width * 0.26, options: [])
    ctx.setFillColor(NSColor(white: 0.97, alpha: 1).cgColor)
    ctx.addPath(archPath(in: plate.insetBy(dx: plate.width * 0.06, dy: plate.height * 0.06), stroke: plate.width * 0.075))
    ctx.fillPath()
    ctx.restoreGState()
}
print("icons written to \(out)")
