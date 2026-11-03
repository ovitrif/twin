import AppKit
import AVFoundation
import IMBCore

// iOS Mirror: one window showing one iPhone. `imb mirror` starts it with
// the phone to show; opened on its own (Finder, Spotlight) it quits.
//
//   --name <phone name>   the capture device to show (default: the first iPhone)
//   --udid <udid>         the phone's UDID, for the title and `imb status`
//   --title <text>        window title (default: the phone's name)
//   --product <type>      productType such as iPhone18,4, for the corner radius
//   --height <points>     initial window height
//   --replay <files>      play comma-separated videos in turn instead of a phone, for testing
//   --mjpeg <url>         show a Motion JPEG stream instead of a phone
//   --wda <url>           send input to WebDriverAgent at this URL

struct Options {
    var name: String?
    var udid: String?
    var title: String?
    var product = ""
    var height: CGFloat?
    var replay: String?
    var mjpeg: String?
    var wda: String?

    init(_ args: [String]) {
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
            default: break
            }
        }
    }

    var isEmpty: Bool { name == nil && udid == nil && replay == nil && mjpeg == nil }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private let options: Options
    private var window: MirrorWindow!
    private var view: MirrorView!
    private var styler: WindowStyler!
    private var source: FrameSource!
    private var input: Input?
    private var videoSize = CGSize.zero

    init(options: Options) { self.options = options }

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMenu()
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let height = options.height ?? (screen.height * 0.8).rounded()
        // iPhone Air proportions until the first frame says otherwise
        let width = (height * 1260 / 2736).rounded()
        window = MirrorWindow(contentRect: NSRect(x: screen.midX - width / 2, y: screen.midY - height / 2,
                                                  width: width, height: height),
                              styleMask: [.borderless, .resizable, .miniaturizable],
                              backing: .buffered, defer: false)
        window.collectionBehavior = [.fullScreenPrimary]
        window.isMovableByWindowBackground = false
        window.isReleasedWhenClosed = false
        window.contentAspectRatio = NSSize(width: width, height: height)
        view = MirrorView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        window.contentView = view
        let title = options.title ?? options.name ?? "iPhone"
        window.title = title
        styler = WindowStyler(window: window, title: title, cornerRatio: Screen.cornerRatio(productType: options.product))
        window.makeKeyAndOrderFront(nil)
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

        if let wda = options.wda.flatMap(URL.init(string:)) { connectInput(WDAClient(base: wda)) }
    }

    private func connectInput(_ client: WDAClient) {
        let input = Input(client: client)
        input.onError = { message in FileHandle.standardError.write(Data("imb: \(message)\n".utf8)) }
        view.onGesture = { input.gesture($0) }
        view.onScroll = { input.scroll(at: $0, by: $1) }
        view.onKeys = { input.type($0) }
        self.input = input
        Task { @MainActor in
            do {
                try await client.connect()
                if let size = client.screenSize { self.view.deviceSize = size }
            } catch {
                input.onError?("\(error)")
            }
        }
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
        let old = videoSize
        videoSize = size
        view.setStatus(nil)
        if input?.screenSize == nil {
            // No input yet: the video's size stands in, scaled to points later
            view.deviceSize = size
        } else if let screen = input?.screenSize,
                  (size.width > size.height) != (screen.width > screen.height) {
            view.deviceSize = CGSize(width: screen.height, height: screen.width)
        }
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
        device.addItem(withTitle: "Close", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "w")
        deviceItem.submenu = device
        main.addItem(deviceItem)
        NSApp.mainMenu = main
    }

    @objc private func home() { input?.home() }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { source?.stop() }
}

let options = Options(Array(CommandLine.arguments.dropFirst()))
if options.isEmpty {
    FileHandle.standardError.write(Data("iOS Mirror shows the phone `imb mirror` asks for; run `imb mirror`.\n".utf8))
    exit(0)
}
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = AppDelegate(options: options)
app.delegate = delegate
app.run()
