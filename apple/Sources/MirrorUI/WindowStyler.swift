import AppKit
import QuartzCore

// Style the borderless mirror window like QuickTime's iPhone mirror: rounded
// corners, a shadow, and a title bar with traffic lights just above the phone
// while Cmd is held over the window, so the phone's screen stays whole and its
// top edge stays the phone's for pull-downs. Dragging that bar, or Cmd +
// dragging anywhere, moves the window.

private let titleBarHeight: CGFloat = 28
private let hoverPollInterval = 1.0 / 30
private let fadeDuration = 0.15
private let lightSize: CGFloat = 12
private let lightSpacing: CGFloat = 8
private let lightMargin: CGFloat = 8
/// The bar floats this far above the phone, its own corners this round
private let barGap: CGFloat = 6
private let barRadius: CGFloat = 9

final class MirrorWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
    // Window managers (Raycast, Mission Control) find windows through
    // Accessibility, which skips a borderless one unless it says it is standard
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .window }
    override func accessibilitySubrole() -> NSAccessibility.Subrole? { .standardWindow }
    override func accessibilityIsIgnored() -> Bool { false }
}

private final class TitleBarView: NSView {
    private(set) var dragging = false
    private var dragStartMouse = NSPoint.zero
    private var dragStartOrigin = NSPoint.zero

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    // The bar is chrome you click, so it keeps macOS's arrow, never the touch pointer
    override func resetCursorRects() { addCursorRect(bounds, cursor: .arrow) }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.cursorUpdate, .activeAlways, .inVisibleRect], owner: self))
    }

    override func cursorUpdate(with event: NSEvent) { NSCursor.arrow.set() }

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

/// One phone in a window that shows several (Twin's pair): its frame in
/// the window's coordinates and its title.
struct Pane {
    var frame: NSRect
    var title: String
}

final class WindowStyler: NSObject {
    /// The phones a window shows side by side; nil for a window of one phone,
    /// which then rounds itself
    var panes: (() -> [Pane])?
    private var barPane = 0
    private let window: NSWindow
    private let bar: NSPanel
    private let material = NSVisualEffectView()
    private var lights: [NSButton] = []
    private let titleLabel: NSTextField
    private var timer: Timer?
    private var barShown = false
    /// The bar shows for a moment when the window opens, so it can be found
    private var introUntil = Date.distantPast
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

    // The title bar: a rounded strip just above the phone, as wide as it
    private func makeBar(frame: NSRect) {
        bar.isReleasedWhenClosed = false
        bar.isOpaque = false
        bar.backgroundColor = .clear
        bar.hasShadow = false
        bar.hidesOnDeactivate = false
        bar.becomesKeyOnlyIfNeeded = true
        bar.collectionBehavior = [.transient, .ignoresCycle]
        bar.setAccessibilityElement(false)
        bar.alphaValue = 0

        let view = TitleBarView(frame: frame)
        view.wantsLayer = true
        view.layer?.masksToBounds = true
        bar.contentView = view

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

    /// Each phone's frame on screen with its title
    private var paneRects: [(frame: NSRect, title: String)] {
        guard let panes else { return [(window.frame, titleLabel.stringValue)] }
        return panes().map { ($0.frame.offsetBy(dx: window.frame.minX, dy: window.frame.minY), $0.title) }
    }

    private func radius(of rect: NSRect) -> CGFloat {
        isFullscreen ? 0 : (min(rect.width, rect.height) * cornerRatio).rounded()
    }

    private var cornerRadius: CGFloat { radius(of: paneRects.first?.frame ?? window.frame) }

    // Fit the bar to the hovered phone's width, just above it; against the
    // top of the screen there is no room, so it lies over the phone's top
    private func layoutBar() {
        let rects = paneRects
        let pane = rects[min(barPane, rects.count - 1)]
        let parent = pane.frame
        if panes != nil { titleLabel.stringValue = pane.title }
        var y = parent.maxY + barGap
        if let screen = window.screen ?? NSScreen.main, y + titleBarHeight > screen.visibleFrame.maxY {
            y = parent.maxY - titleBarHeight - barGap
        }
        bar.setFrame(NSRect(x: parent.minX, y: y, width: parent.width, height: titleBarHeight), display: false)
        material.frame = NSRect(x: 0, y: 0, width: parent.width, height: titleBarHeight)
        material.layer?.cornerRadius = barRadius
        var x = lightMargin + 2
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
        // A window of several phones leaves the rounding to each
        content.layer?.cornerRadius = panes == nil ? cornerRadius : 0
        content.layer?.cornerCurve = .continuous
        content.layer?.masksToBounds = panes == nil
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
    private let forceBar = ProcessInfo.processInfo.environment["DMB_SHOW_BAR"] != nil

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
            let over = paneRects.firstIndex { $0.frame.contains(mouse) }
            // The bar sits outside the phone, so the pointer on it counts too
            let overBar = barShown && bar.frame.contains(mouse)
            let command = NSEvent.modifierFlags.contains(.command)
            hovered = (onTop && (over != nil || overBar) && command) || dragging || forceBar || Date() < introUntil
            // The bar moves to the phone under the pointer
            if let over, over != barPane, !dragging {
                barPane = over
                if barShown { layoutBar() }
            }
        }
        if barShown && bar.level != window.level { bar.level = window.level }
        if hovered && !barShown { showBar() } else if !hovered && barShown { hideBar() }
    }

    /// Flashes the bar once the window is on screen
    func introduceBar(for seconds: TimeInterval = 1.5) {
        introUntil = Date().addingTimeInterval(seconds)
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
