import AppKit
import IMBCore

enum TouchPhase: String { case down, move, up }

/// Where the window's input goes. Points run from 0 to 1 across the phone's
/// screen, top-left origin.
protocol InputSink: AnyObject {
    var onError: ((String) -> Void)? { get set }
    func touch(_ phase: TouchPhase, at point: CGPoint)
    func scroll(at point: CGPoint, by delta: CGVector)
    func type(_ text: String)
    /// A key with no character, as a HID keyboard usage
    func key(_ usage: Int)
    func button(_ name: String)
}

/// Live touch through `imb-hid` and the CoreDevice HID service (iOS 27 and
/// later): every pointer move reaches the phone as it happens. A session that
/// drops, as when the phone replugs, starts again by itself.
final class HIDInput: InputSink {
    var onError: ((String) -> Void)?
    var onReady: (() -> Void)?
    private let helper: URL
    private let udid: String
    private var process: Process?
    private var stdin = Pipe()
    private var stopping = false
    private var scrollPoint: CGPoint?
    private var scrollPending = CGVector.zero
    private var scrollTimer: Timer?

    init(helper: URL, udid: String) {
        self.helper = helper
        self.udid = udid
    }

    func start() throws {
        let process = Process()
        process.executableURL = helper
        process.arguments = [udid]
        let stdin = Pipe()
        process.standardInput = stdin
        let stdout = Pipe()
        process.standardOutput = stdout
        stdout.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let text = String(decoding: handle.availableData, as: UTF8.self)
            for line in text.split(separator: "\n") {
                DispatchQueue.main.async {
                    if line == "ready" { self?.onReady?() } else { self?.onError?(String(line)) }
                }
            }
        }
        process.terminationHandler = { [weak self] ended in
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                guard let self, !self.stopping, self.process === ended else { return }
                self.onError?("input session ended (\(ended.terminationStatus)), reconnecting")
                try? self.start()
            }
        }
        try process.run()
        self.process = process
        self.stdin = stdin
    }

    /// Closing stdin lets imb-hid end the media stream, which the phone needs
    /// to accept the next session; terminate only if it hangs
    func stop() {
        stopping = true
        guard let process, process.isRunning else { return }
        try? stdin.fileHandleForWriting.close()
        let deadline = Date().addingTimeInterval(0.5)
        while process.isRunning && Date() < deadline { usleep(20_000) }
        if process.isRunning { process.terminate() }
    }

    private func send(_ command: [String: Any]) {
        guard process?.isRunning == true, let data = try? JSONSerialization.data(withJSONObject: command) else { return }
        stdin.fileHandleForWriting.write(data + Data("\n".utf8))
    }

    func touch(_ phase: TouchPhase, at point: CGPoint) {
        send(["touch": phase.rawValue, "x": point.x, "y": point.y])
    }

    /// A scroll is a finger that lands where the pointer is, follows the
    /// scroll and lifts once it pauses. It lands only once the scroll has
    /// travelled 1.5% of the screen, so a nudge never reads as a tap.
    func scroll(at point: CGPoint, by delta: CGVector) {
        scrollTimer?.invalidate()
        scrollTimer = Timer.scheduledTimer(withTimeInterval: 0.12, repeats: false) { [weak self] _ in
            guard let self else { return }
            self.scrollPending = .zero
            guard let p = self.scrollPoint else { return }
            self.scrollPoint = nil
            self.touch(.up, at: p)
        }
        if scrollPoint == nil {
            scrollPending.dx += delta.dx
            scrollPending.dy += delta.dy
            guard hypot(scrollPending.dx, scrollPending.dy) >= 0.015 else { return }
            scrollPoint = point
            touch(.down, at: point)
            moveScroll(by: scrollPending)
            scrollPending = .zero
        } else {
            moveScroll(by: delta)
        }
    }

    private func moveScroll(by delta: CGVector) {
        guard var p = scrollPoint else { return }
        p.x = min(max(p.x + delta.dx, 0), 1)
        p.y = min(max(p.y + delta.dy, 0), 1)
        scrollPoint = p
        touch(.move, at: p)
    }

    func type(_ text: String) { send(["text": text]) }
    func key(_ usage: Int) { send(["key": usage]) }
    func button(_ name: String) { send(["button": name]) }
}

/// WebDriverAgent, for phones before iOS 27: each touch goes as one gesture
/// once the pointer is released, one request at a time and in order.
final class WDAInput: InputSink {
    var onError: ((String) -> Void)?
    private let client: WDAClient
    private var queue: [() async throws -> Void] = []
    private var busy = false
    private var start: TimeInterval = 0
    private var points: [CGPoint] = []
    private var times: [TimeInterval] = []
    private var scrollStart: CGPoint?
    private var scrollTotal = CGVector.zero
    private var scrollTimer: Timer?
    private var typed = ""
    private var typeTimer: Timer?

    init(client: WDAClient) {
        self.client = client
        enqueue { try await client.connect() }
    }

    private var size: CGSize { client.screenSize ?? CGSize(width: 1, height: 1) }

    private func phonePoint(_ p: CGPoint) -> CGPoint {
        Screen.devicePoint(CGPoint(x: p.x * size.width, y: p.y * size.height), view: size, device: size)
    }

    func touch(_ phase: TouchPhase, at point: CGPoint) {
        let now = ProcessInfo.processInfo.systemUptime
        switch phase {
        case .down:
            start = now
            points = [phonePoint(point)]
            times = [0]
        case .move:
            points.append(phonePoint(point))
            times.append(now - start)
        case .up:
            points.append(phonePoint(point))
            times.append(now - start)
            let gesture = Gesture(points: points, times: times)
            enqueue { try await self.client.perform(gesture) }
        }
    }

    /// A scroll becomes one swipe once it pauses, as long as the scroll and
    /// starting where the pointer was
    func scroll(at point: CGPoint, by delta: CGVector) {
        if scrollStart == nil { scrollStart = point }
        scrollTotal.dx += delta.dx
        scrollTotal.dy += delta.dy
        scrollTimer?.invalidate()
        scrollTimer = Timer.scheduledTimer(withTimeInterval: 0.15, repeats: false) { [weak self] _ in
            guard let self, let start = self.scrollStart else { return }
            let total = self.scrollTotal
            self.scrollStart = nil
            self.scrollTotal = .zero
            let end = CGPoint(x: min(max(start.x + total.dx, 0), 1), y: min(max(start.y + total.dy, 0), 1))
            let gesture = Gesture(points: [self.phonePoint(start), self.phonePoint(end)], times: [0, 0.25])
            self.enqueue { try await self.client.perform(gesture) }
        }
    }

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

    func key(_ usage: Int) {}

    func button(_ name: String) {
        guard name == "home" else { return }
        enqueue { try await self.client.home() }
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
