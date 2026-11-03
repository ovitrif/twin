// Draws the mirror apps' icons: a phone on a dark squircle, its screen lit.
// Usage: swift scripts/make-icon.swift iphone|android|pair OUT.icns
// iOS Mirror uses `iphone`, awb's Android Mirror `android` and Android v iOS
// `pair`, both phones side by side.
import AppKit

let args = CommandLine.arguments
guard args.count == 3, ["iphone", "android", "pair"].contains(args[1]) else {
    print("usage: make-icon.swift iphone|android|pair OUT.icns"); exit(2)
}

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: alpha)
}


/// A phone at `center`, 380 × 660 at scale 1, its screen lit green or blue
func drawPhone(center: NSPoint, scale s: CGFloat, android: Bool) {
    let phone = NSRect(x: center.x - 190 * s, y: center.y - 330 * s, width: 380 * s, height: 660 * s)
    let body = NSBezierPath(roundedRect: phone, xRadius: 78 * s, yRadius: 78 * s)
    NSGradient(starting: color(0xE9EBF0), ending: color(0x9AA0AD))!.draw(in: body, angle: -90)
    let screenRect = phone.insetBy(dx: 18 * s, dy: 18 * s)
    let screen = NSBezierPath(roundedRect: screenRect, xRadius: 62 * s, yRadius: 62 * s)
    let glow = android ? (color(0x3DDC84), color(0x0B6B3A)) : (color(0x5AC8FA), color(0x0A3D91))
    NSGradient(starting: glow.0, ending: glow.1)!.draw(in: screen, angle: -60)
    // A soft sheen across the screen, the mirror's reflection
    NSGraphicsContext.saveGraphicsState()
    screen.addClip()
    let sheen = NSBezierPath()
    sheen.move(to: NSPoint(x: screenRect.minX, y: screenRect.midY + 40 * s))
    sheen.line(to: NSPoint(x: screenRect.maxX, y: screenRect.maxY - 40 * s))
    sheen.line(to: NSPoint(x: screenRect.maxX, y: screenRect.maxY))
    sheen.line(to: NSPoint(x: screenRect.minX, y: screenRect.maxY))
    sheen.close()
    NSColor(white: 1, alpha: 0.16).setFill()
    sheen.fill()
    NSGraphicsContext.restoreGraphicsState()
    // The camera: a Dynamic Island or a punch hole
    NSColor(white: 0.04, alpha: 1).setFill()
    if android {
        NSBezierPath(ovalIn: NSRect(x: center.x - 15 * s, y: screenRect.maxY - 52 * s, width: 30 * s, height: 30 * s)).fill()
    } else {
        NSBezierPath(roundedRect: NSRect(x: center.x - 62 * s, y: screenRect.maxY - 58 * s, width: 124 * s, height: 36 * s),
                     xRadius: 18 * s, yRadius: 18 * s).fill()
    }
}

func draw(_ size: CGFloat) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size), bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let s = size / 1024
    // macOS icon grid: an 824 pt squircle with room for its shadow
    let tile = NSRect(x: 100 * s, y: 100 * s, width: 824 * s, height: 824 * s)
    let squircle = NSBezierPath(roundedRect: tile, xRadius: 185 * s, yRadius: 185 * s)
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor(white: 0, alpha: 0.35)
    shadow.shadowBlurRadius = 24 * s
    shadow.shadowOffset = NSSize(width: 0, height: -10 * s)
    shadow.set()
    color(0x15171C).setFill()
    squircle.fill()
    NSGraphicsContext.restoreGraphicsState()
    NSGradient(starting: color(0x343946), ending: color(0x101216))!.draw(in: squircle, angle: -90)

    if args[1] == "pair" {
        drawPhone(center: NSPoint(x: 512 * s - 128 * s, y: 512 * s), scale: 0.62 * s, android: true)
        drawPhone(center: NSPoint(x: 512 * s + 128 * s, y: 512 * s), scale: 0.62 * s, android: false)
    } else {
        drawPhone(center: NSPoint(x: 512 * s, y: 512 * s), scale: s, android: args[1] == "android")
    }
    NSGraphicsContext.restoreGraphicsState()
    return rep
}

let out = URL(fileURLWithPath: args[2])
let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("mirror-\(UUID()).iconset")
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let name = scale == 1 ? "icon_\(base)x\(base).png" : "icon_\(base)x\(base)@2x.png"
        try draw(CGFloat(base * scale)).representation(using: .png, properties: [:])!
            .write(to: iconset.appendingPathComponent(name))
    }
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", out.path]
try iconutil.run()
iconutil.waitUntilExit()
try? FileManager.default.removeItem(at: iconset)
exit(iconutil.terminationStatus)
