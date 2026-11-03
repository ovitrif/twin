import AppKit
import QuartzCore

// Style the borderless mirror window like QuickTime's iPhone mirror, as awb's
// Android mirror does: rounded corners, a shadow, and a title bar with traffic
// lights over the top of the screen while the pointer is over the window.
// Dragging that bar, or Cmd + dragging anywhere, moves the window.

private let titleBarHeight: CGFloat = 28
private let hoverPollInterval = 1.0 / 30
private let fadeDuration = 0.15
private let lightSize: CGFloat = 12
private let lightSpacing: CGFloat = 8
private let lightMargin: CGFloat = 8

final class MirrorWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

private final class TitleBarView: NSView {
    private(set) var dragging = false
    private var dragStartMouse = NSPoint.zero
    private var dragStartOrigin = NSPoint.zero

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    // Drag from anywhere but the buttons, the title label included
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit is NSButton ? hit : (hit == nil ? nil : self)
    }

    override func mouseDown(with event: NSEvent) {
        dragging = true
        dragStartMouse = NSEvent.mouseLocation
        dragStartOrigin = window?.parent?.frame.origin ?? .zero
    }

    override func mouseUp(with event: NSEvent) { dragging = false }

    override func mouseDragged(with event: NSEvent) {
        // The bar is a child window, so it follows its parent
        guard let parent = window?.parent else { return }
        let mouse = NSEvent.mouseLocation
        parent.setFrameOrigin(NSPoint(x: dragStartOrigin.x + mouse.x - dragStartMouse.x,
                                      y: dragStartOrigin.y + mouse.y - dragStartMouse.y))
    }
}

final class WindowStyler: NSObject {
    private let window: NSWindow
    private let bar: NSPanel
    private let material = NSVisualEffectView()
    private var lights: [NSButton] = []
    private let titleLabel: NSTextField
    private var timer: Timer?
    private var barShown = false
    private lazy var ring = ResizeRing(mirror: window)
    /// Share of the window width the corners round by.
    var cornerRatio: CGFloat { didSet { applyStyle() } }

    init(window: NSWindow, title: String, cornerRatio: CGFloat) {
        self.window = window
        self.cornerRatio = cornerRatio
        let frame = NSRect(x: 0, y: 0, width: 200, height: titleBarHeight)
        bar = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel],
                      backing: .buffered, defer: true)
        titleLabel = NSTextField(labelWithString: title)
        super.init()
        makeBar(frame: frame)
        applyStyle()

        let center = NotificationCenter.default
        center.addObserver(self, selector: #selector(exitedFullScreen), name: NSWindow.didExitFullScreenNotification, object: window)
        for name in [NSWindow.didResizeNotification, NSWindow.didExitFullScreenNotification,
                     NSWindow.didEnterFullScreenNotification, NSWindow.didDeminiaturizeNotification] {
            center.addObserver(self, selector: #selector(windowChanged), name: name, object: window)
        }
        for name in [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            center.addObserver(self, selector: #selector(updateLights), name: name, object: window)
        }
        // Poll the pointer so the bar follows it over a covered window too.
        // It runs in the common modes so it keeps firing during a drag.
        let timer = Timer(timeInterval: hoverPollInterval, repeats: true) { [weak self] _ in self?.tick() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    var title: String {
        get { titleLabel.stringValue }
        set { titleLabel.stringValue = newValue }
    }

    // QuickTime-style title bar: an overlay across the top of the screen,
    // square at the bottom, its top corners following the window's rounding
    private func makeBar(frame: NSRect) {
        bar.isReleasedWhenClosed = false
        bar.isOpaque = false
        bar.backgroundColor = .clear
        bar.hasShadow = false
        bar.hidesOnDeactivate = false
        bar.becomesKeyOnlyIfNeeded = true
        bar.collectionBehavior = [.transient, .ignoresCycle]
        bar.alphaValue = 0

        let view = TitleBarView(frame: frame)
        view.wantsLayer = true
        view.layer?.masksToBounds = true
        bar.contentView = view

        // As tall as the window, so its rounded top matches the window's
        // corners exactly; the bar shows only its top strip
        material.material = .hudWindow
        material.blendingMode = .behindWindow
        material.state = .active
        material.wantsLayer = true
        material.layer?.cornerCurve = .continuous
        material.layer?.masksToBounds = true
        view.addSubview(material)

        // AppKit's standard buttons stay gray outside a real title bar, so
        // draw the traffic lights here
        let actions = [#selector(closeClicked), #selector(minimizeClicked), #selector(zoomClicked)]
        for (i, action) in actions.enumerated() {
            let button = NSButton(frame: NSRect(x: 0, y: (titleBarHeight - lightSize) / 2,
                                                width: lightSize, height: lightSize))
            button.isBordered = false
            button.title = ""
            button.tag = i
            button.wantsLayer = true
            button.layer?.cornerRadius = lightSize / 2
            button.target = self
            button.action = action
            view.addSubview(button)
            lights.append(button)
        }

        titleLabel.font = .titleBarFont(ofSize: 0)
        titleLabel.textColor = .secondaryLabelColor
        titleLabel.alignment = .center
        titleLabel.lineBreakMode = .byTruncatingTail
        view.addSubview(titleLabel)
    }

    private var isFullscreen: Bool { window.styleMask.contains(.fullScreen) }

    private var cornerRadius: CGFloat {
        isFullscreen ? 0 : (min(window.frame.width, window.frame.height) * cornerRatio).rounded()
    }

    // Fit the bar to the window's current width and corners
    private func layoutBar() {
        let parent = window.frame
        bar.setFrame(NSRect(x: parent.minX, y: parent.maxY - titleBarHeight,
                            width: parent.width, height: titleBarHeight), display: false)
        let radius = cornerRadius
        material.frame = NSRect(x: 0, y: titleBarHeight - parent.height,
                                width: parent.width, height: parent.height)
        material.layer?.cornerRadius = radius
        // Keep the lights clear of the rounded corner at the bar's mid-height
        let mid = titleBarHeight / 2
        let corner = radius > mid ? radius - (radius * radius - (radius - mid) * (radius - mid)).squareRoot() : 0
        var x = max(lightMargin, corner + lightMargin)
        for light in lights {
            light.setFrameOrigin(NSPoint(x: x, y: light.frame.origin.y))
            x += lightSize + lightSpacing
        }
        let height = titleLabel.intrinsicContentSize.height
        let inset = x + 4
        titleLabel.frame = NSRect(x: inset, y: (titleBarHeight - height) / 2,
                                  width: max(0, parent.width - 2 * inset), height: height)
    }

    func applyStyle() {
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        guard let content = window.contentView else { return }
        content.wantsLayer = true
        content.layer?.cornerRadius = cornerRadius
        content.layer?.cornerCurve = .continuous
        content.layer?.masksToBounds = true
        // The shadow follows the window's alpha; recompute it for the new corners
        window.invalidateShadow()
        ring.update(radius: cornerRadius, shown: window.isVisible && !isFullscreen && !window.isMiniaturized)
    }

    @objc private func exitedFullScreen() { window.styleMask.remove(.resizable) }

    @objc private func windowChanged() {
        applyStyle()
        if barShown { layoutBar() }
    }

    // Colored while the mirror window is active, gray otherwise, like AppKit's
    @objc private func updateLights() {
        let colors = [NSColor(srgbRed: 1.00, green: 0.37, blue: 0.34, alpha: 1),
                      NSColor(srgbRed: 1.00, green: 0.74, blue: 0.18, alpha: 1),
                      NSColor(srgbRed: 0.16, green: 0.78, blue: 0.25, alpha: 1)]
        let active = window.isKeyWindow || forceBar
        for light in lights {
            light.layer?.backgroundColor = (active ? colors[light.tag] : NSColor(white: 0.5, alpha: 0.5)).cgColor
        }
    }

    /// Keeps the bar shown and its lights lit, for screenshots in a test VM
    /// with no pointer
    private let forceBar = ProcessInfo.processInfo.environment["IMB_SHOW_BAR"] != nil

    private func tick() {
        let canShow = window.isVisible && !window.isMiniaturized && !isFullscreen
        var hovered = false
        if canShow {
            let mouse = NSEvent.mouseLocation
            // Over the mirror only where no other window covers it
            let top = NSWindow.windowNumber(at: mouse, belowWindowWithWindowNumber: 0)
            let onTop = top == window.windowNumber || (barShown && top == bar.windowNumber)
            // Keep the bar while it drags the window
            let dragging = barShown && (NSEvent.pressedMouseButtons & 1) != 0
                && (bar.contentView as? TitleBarView)?.dragging == true
            hovered = (onTop && window.frame.contains(mouse)) || dragging || forceBar
        }
        if barShown && bar.level != window.level { bar.level = window.level }
        if hovered && !barShown { showBar() } else if !hovered && barShown { hideBar() }
    }

    private func showBar() {
        barShown = true
        layoutBar()
        bar.level = window.level
        bar.ignoresMouseEvents = false
        updateLights()
        if bar.parent == nil { window.addChildWindow(bar, ordered: .above) }
        bar.orderFront(nil)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = fadeDuration
            bar.animator().alphaValue = 1
        }
    }

    private func hideBar() {
        barShown = false
        // Let clicks through to the phone while the bar fades out
        bar.ignoresMouseEvents = true
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = fadeDuration
            bar.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self, !self.barShown else { return }
            // Detach the hidden bar so it takes no clicks and window managers
            // only ever see the mirror window
            self.window.removeChildWindow(self.bar)
            self.bar.orderOut(nil)
        })
    }

    @objc private func closeClicked() { NSApp.terminate(nil) }
    @objc private func minimizeClicked() { window.miniaturize(nil) }
    // Full screen needs a resizable window; the ring does all other resizing
    @objc private func zoomClicked() {
        if !isFullscreen { window.styleMask.insert(.resizable) }
        window.toggleFullScreen(nil)
    }
}
