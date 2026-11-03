import AppKit

// Mirrors opened side by side (awb's Side by side setting) move as one: when
// one moves or changes height, the other follows, a gap apart and the same
// height. The mirrors are separate apps (awb's Android Mirror and iOS
// Mirror), so they tell each other through distributed notifications.

private let moved = Notification.Name("tech.masivo.mirror.group.moved")

final class MirrorGroup {
    enum Side: String { case left, right }

    private let window: NSWindow
    private let group: String
    private let side: Side
    private let gap: CGFloat
    private let id = UUID().uuidString
    private var following = false
    private var partner: NSRect?
    private var observers: [NSObjectProtocol] = []

    init(window: NSWindow, group: String, side: Side, gap: CGFloat) {
        self.window = window
        self.group = group
        self.side = side
        self.gap = gap
        let center = DistributedNotificationCenter.default()
        observers.append(center.addObserver(forName: moved, object: group, queue: .main) { [weak self] note in
            self?.partnerMoved(note.userInfo)
        })
        for name in [NSWindow.didMoveNotification, NSWindow.didResizeNotification] {
            observers.append(NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) {
                [weak self] _ in self?.announce()
            })
        }
    }

    /// Before the partner speaks, the left mirror makes room for it so the
    /// pair starts centered; then each announces where it is
    func placeInitially() {
        guard let screen = window.screen?.visibleFrame else { return }
        var frame = window.frame
        // The partner is about as wide as a phone this tall
        let pair = frame.width + gap + frame.height * 0.46
        frame.origin.x = side == .left ? screen.midX - pair / 2 : screen.midX + pair / 2 - frame.width
        following = true
        window.setFrameOrigin(frame.origin)
        following = false
        announce()
    }

    /// A window manager's Center (Raycast's) moves only one mirror; when one
    /// lands centered on its screen, the pair is centered instead
    private func centerPairIfCentered() -> Bool {
        guard let partner, let screen = window.screen?.visibleFrame else { return false }
        let frame = window.frame
        guard abs(frame.midX - screen.midX) < 1.5 else { return false }
        let pair = frame.width + gap + partner.width
        let x = side == .left ? screen.midX - pair / 2 : screen.midX - pair / 2 + partner.width + gap
        guard abs(x - frame.minX) >= 1 else { return false }
        window.setFrameOrigin(NSPoint(x: x.rounded(), y: frame.minY))  // announces again
        return true
    }

    private func announce() {
        guard !following, window.isVisible else { return }
        if centerPairIfCentered() { return }
        let f = window.frame
        DistributedNotificationCenter.default().postNotificationName(
            moved, object: group,
            userInfo: ["id": id, "side": side.rawValue, "x": f.minX, "y": f.minY, "w": f.width, "h": f.height],
            deliverImmediately: true)
    }

    private func partnerMoved(_ info: [AnyHashable: Any]?) {
        guard let info, info["id"] as? String != id, info["side"] as? String != side.rawValue,
              let x = info["x"] as? CGFloat, let y = info["y"] as? CGFloat,
              let w = info["w"] as? CGFloat, let h = info["h"] as? CGFloat,
              !window.styleMask.contains(.fullScreen), h > 0
        else { return }
        let partner = NSRect(x: x, y: y, width: w, height: h)
        self.partner = partner
        var frame = window.frame
        // Same height, keeping this phone's shape, tops aligned
        let width = (frame.width * h / max(frame.height, 1)).rounded()
        frame.size = NSSize(width: width, height: h)
        frame.origin.y = partner.minY
        frame.origin.x = side == .right ? partner.maxX + gap : partner.minX - gap - width
        guard frame != window.frame else { return }
        following = true
        window.setFrame(frame, display: true)
        following = false
    }
}
