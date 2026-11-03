import AppKit
import AVFoundation
import IMBCore

// iOS Mirror: one window showing one iPhone. `imb mirror` starts it with
// the phone to show; opened on its own (Finder, Spotlight) it quits.
//
//   --name <phone name>   the capture device to show (default: the first iPhone)
//   --udid <udid>         the phone's UDID, for the title and `imb status`
//   --title <text>        window title (default: iPhone)
//   --product <type>      productType such as iPhone18,4, for the corner radius
//   --height <points>     initial window height
//   --replay <files>      play comma-separated videos in turn instead of a phone, for testing
//   --mjpeg <url>         show a Motion JPEG stream instead of a phone
//   --wda <url>           send input to WebDriverAgent at this URL instead of
//                         imb-hid, for phones before iOS 27
//   --no-input            show the phone only
//   --parent <pid>        quit when this process ends (imb mirror --wait)
//   --log <file>          append diagnostics here instead of stderr

public struct Options {
    var name: String?
    var udid: String?
    var title: String?
    var product = ""
    var height: CGFloat?
    var replay: String?
    var mjpeg: String?
    var wda: String?
    var noInput = false
    var parent: pid_t?
    var log: String?

    public init(_ args: [String]) {
        var it = args.makeIterator()
        while let arg = it.next() {
            switch arg {
            case "--name": name = it.next()
            case "--udid": udid = it.next()
            case "--title": title = it.next()
            case "--product": product = it.next() ?? ""
            case "--height": height = it.next().flatMap { Double($0) }.map { CGFloat($0) }
            case "--replay": replay = it.next()
            case "--mjpeg": mjpeg = it.next()
            case "--wda": wda = it.next()
            case "--no-input": noInput = true
            case "--parent": parent = it.next().flatMap { pid_t($0) }
            case "--log": log = it.next()
            default: break
            }
        }
    }

    public var isEmpty: Bool { name == nil && udid == nil && replay == nil && mjpeg == nil }
}

public final class AppDelegate: NSObject, NSApplicationDelegate {
    private let options: Options
    private var window: MirrorWindow!
    private var view: MirrorView!
    private var styler: WindowStyler!
    private var source: FrameSource!
    private var input: InputSink?
    private var hid: HIDInput?
    private var videoSize = CGSize.zero
    private var parentWatch: DispatchSourceProcess?

    public init(options: Options) { self.options = options }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        if let log = options.log { freopen(log, "a", stderr) }
        if let parent = options.parent {
            let watch = DispatchSource.makeProcessSource(identifier: parent, eventMask: .exit, queue: .main)
            watch.setEventHandler { NSApp.terminate(nil) }
            watch.resume()
            parentWatch = watch
            if kill(parent, 0) != 0 { NSApp.terminate(nil) }
        }
        buildMenu()
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let height = options.height ?? (screen.height * 0.8).rounded()
        // iPhone Air proportions until the first frame says otherwise
        let width = (height * 1260 / 2736).rounded()
        window = MirrorWindow(contentRect: NSRect(x: screen.midX - width / 2, y: screen.midY - height / 2,
                                                  width: width, height: height),
                              styleMask: [.borderless, .miniaturizable],  // the resize ring resizes it
                              backing: .buffered, defer: false)
        window.collectionBehavior = [.fullScreenPrimary]
        window.isMovableByWindowBackground = false
        window.acceptsMouseMovedEvents = true
        window.isReleasedWhenClosed = false
        window.contentAspectRatio = NSSize(width: width, height: height)
        view = MirrorView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        window.contentView = view
        let title = options.title ?? "iPhone"
        window.title = title
        styler = WindowStyler(window: window, title: title, cornerRatio: Screen.cornerRatio(productType: options.product))
        window.makeKeyAndOrderFront(nil)
        styler.applyStyle()  // places the resize ring now the window is on screen
        window.makeFirstResponder(view)
        NSApp.activate(ignoringOtherApps: true)

        if let replay = options.replay {
            source = ReplaySource(urls: replay.split(separator: ",").map { URL(fileURLWithPath: String($0)) })
        } else if let mjpeg = options.mjpeg.flatMap(URL.init(string:)) {
            source = MJPEGSource(url: mjpeg)
        } else {
            source = CaptureSource(name: options.name)
        }
        source.onStatus = { [weak self] text in DispatchQueue.main.async { self?.view.setStatus(text) } }
        source.onFrame = { [weak self] buffer in self?.show(buffer) }
        source.start()

        connectInput()
    }

    /// imb-hid sits next to this binary in the app bundle
    private var hidHelper: URL? {
        let candidates = [ProcessInfo.processInfo.environment["IMB_HID"],
                          Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("imb-hid").path]
        return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0) }
            .map { URL(fileURLWithPath: $0) }
    }

    private func connectInput() {
        guard !options.noInput else { return }
        let log: (String) -> Void = { FileHandle.standardError.write(Data("imb: \($0)\n".utf8)) }
        if let wda = options.wda.flatMap(URL.init(string:)) {
            let input = WDAInput(client: WDAClient(base: wda))
            input.onError = log
            self.input = input
        } else if let udid = options.udid, let helper = hidHelper {
            // Started on the first frame: the phone reconnects its USB when
            // capture begins, which drops any session opened before
            let hid = HIDInput(helper: helper, udid: udid)
            hid.onError = log
            hid.onReady = { log("input ready") }
            self.hid = hid
            self.input = hid
        }
        view.input = input
    }

    private func show(_ buffer: CMSampleBuffer) {
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: true),
           CFArrayGetCount(attachments) > 0 {
            let dict = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(dict, Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                                 Unmanaged.passUnretained(kCFBooleanTrue).toOpaque())
        }
        guard let format = CMSampleBufferGetFormatDescription(buffer) else { return }
        let dims = CMVideoFormatDescriptionGetDimensions(format)
        let size = CGSize(width: CGFloat(dims.width), height: CGFloat(dims.height))
        DispatchQueue.main.async {
            if size != self.videoSize { self.resize(to: size) }
        }
        let renderer = view.display.sampleBufferRenderer
        if renderer.status == .failed { renderer.flush() }
        renderer.enqueue(buffer)
    }

    /// Fit the window to the video. On a rotation the window turns about its
    /// center, so the phone stays the same size on screen.
    private func resize(to size: CGSize) {
        if videoSize == .zero, let hid {
            do { try hid.start() } catch { FileHandle.standardError.write(Data("imb: cannot start imb-hid: \(error)\n".utf8)) }
        }
        let old = videoSize
        videoSize = size
        view.setStatus(nil)
        guard size.width > 0, size.height > 0, !window.styleMask.contains(.fullScreen) else { return }
        var frame = window.frame
        let turned = old != .zero && (old.width > old.height) != (size.width > size.height)
        let long = max(frame.width, frame.height)
        let newSize = turned
            ? NSSize(width: frame.height, height: frame.width)
            : (size.width > size.height
                ? NSSize(width: long, height: (long * size.height / size.width).rounded())
                : NSSize(width: (frame.height * size.width / size.height).rounded(), height: frame.height))
        frame.origin.x += (frame.width - newSize.width) / 2
        frame.origin.y += (frame.height - newSize.height) / 2
        frame.size = newSize
        window.contentAspectRatio = newSize
        window.setFrame(frame, display: true, animate: false)
        styler.applyStyle()
    }

    private func buildMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem()
        let app = NSMenu()
        app.addItem(withTitle: "Quit iOS Mirror", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = app
        main.addItem(appItem)
        let deviceItem = NSMenuItem()
        let device = NSMenu(title: "Device")
        device.addItem(withTitle: "Home", action: #selector(home), keyEquivalent: "h").target = self
        device.addItem(withTitle: "Lock", action: #selector(lock), keyEquivalent: "l").target = self
        device.addItem(withTitle: "Close", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "w")
        deviceItem.submenu = device
        main.addItem(deviceItem)
        NSApp.mainMenu = main
    }

    @objc private func home() { input?.button("home") }
    @objc private func lock() { input?.button("lock") }

    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    public func applicationWillTerminate(_ notification: Notification) {
        source?.stop()
        hid?.stop()
    }
}

