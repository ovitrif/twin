import AppKit
import IMBCore

/// Sends the window's touches, scrolls and typing to WebDriverAgent, one at a
/// time and in order.
final class Input {
    private let client: WDAClient
    private var queue: [() async throws -> Void] = []
    private var busy = false
    private var scrollStart: CGPoint?
    private var scrollTotal = CGVector.zero
    private var scrollTimer: Timer?
    private var typed = ""
    private var typeTimer: Timer?
    var onError: ((String) -> Void)?
    var screenSize: CGSize? { client.screenSize }

    init(client: WDAClient) { self.client = client }

    func gesture(_ gesture: Gesture) { enqueue { try await self.client.perform(gesture) } }
    func home() { enqueue { try await self.client.home() } }

    /// Typing in a burst goes as one request
    func type(_ text: String) {
        typed += text
        typeTimer?.invalidate()
        typeTimer = Timer.scheduledTimer(withTimeInterval: 0.08, repeats: false) { [weak self] _ in
            guard let self, !self.typed.isEmpty else { return }
            let text = self.typed
            self.typed = ""
            self.enqueue { try await self.client.type(text) }
        }
    }

    /// A wheel or trackpad scroll becomes one swipe once it pauses, as long as
    /// the scroll and starting where the pointer was
    func scroll(at point: CGPoint, by delta: CGVector) {
        if scrollStart == nil { scrollStart = point }
        scrollTotal.dx += delta.dx
        scrollTotal.dy += delta.dy
        scrollTimer?.invalidate()
        scrollTimer = Timer.scheduledTimer(withTimeInterval: 0.15, repeats: false) { [weak self] _ in
            self?.flushScroll()
        }
    }

    private func flushScroll() {
        guard let start = scrollStart, let size = client.screenSize else { return }
        scrollStart = nil
        let total = scrollTotal
        scrollTotal = .zero
        guard abs(total.dx) + abs(total.dy) >= 4 else { return }
        let end = CGPoint(x: min(max(start.x + total.dx, 1), size.width - 1),
                          y: min(max(start.y + total.dy, 1), size.height - 1))
        gesture(Gesture(points: [start, end], times: [0, 0.25]))
    }

    private func enqueue(_ work: @escaping () async throws -> Void) {
        queue.append(work)
        drain()
    }

    private func drain() {
        guard !busy, !queue.isEmpty else { return }
        busy = true
        let work = queue.removeFirst()
        Task { @MainActor in
            do { try await work() } catch { self.onError?("\(error)") }
            self.busy = false
            self.drain()
        }
    }
}
