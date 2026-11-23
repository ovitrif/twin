import CoreMedia
import Foundation

// A scrcpy client for one Android phone: it runs scrcpy's own server on the
// phone (scrcpy-server 5.0.1, Apache-2.0, the build Twin bundles), reads its
// H.264 video and sends touch and keys over its control socket. This is the
// part of scrcpy's desktop app Twin needs, ported from its C client
// (app/src/server.c, demuxer.c, control_msg.c).

public enum ScrcpyProtocol {
    public static let version = "5.0.1"
    static let devicePath = "/data/local/tmp/scrcpy-server.jar"
    static let flagConfig: UInt64 = 1 << 62
    static let flagKeyFrame: UInt64 = 1 << 61
    static let codecH264: UInt32 = 0x6832_3634  // "h264"

    /// Pointer id of a finger, so drags read as touchscreen swipes
    static let genericFinger: UInt64 = UInt64(bitPattern: -2)

    public enum TouchAction: UInt8 { case down = 0, up = 1, move = 2 }

    /// INJECT_TOUCH_EVENT: 32 bytes
    public static func touch(_ action: TouchAction, x: Int32, y: Int32, width: UInt16, height: UInt16) -> Data {
        var d = Data([2, action.rawValue])
        d.appendBE(genericFinger)
        d.appendBE(UInt32(bitPattern: x))
        d.appendBE(UInt32(bitPattern: y))
        d.appendBE(width)
        d.appendBE(height)
        d.appendBE(action == .up ? UInt16(0) : UInt16(0xFFFF))  // pressure
        d.appendBE(UInt32(0))  // action button
        d.appendBE(UInt32(0))  // buttons
        return d
    }

    /// INJECT_KEYCODE down or up: 14 bytes
    public static func key(_ keycode: UInt32, down: Bool) -> Data {
        var d = Data([0, down ? 0 : 1])
        d.appendBE(keycode)
        d.appendBE(UInt32(0))  // repeat
        d.appendBE(UInt32(0))  // meta state
        return d
    }

    /// At most `limit` bytes of UTF-8, cut between characters so the phone
    /// never gets half of one
    static func utf8Prefix(_ text: String, _ limit: Int) -> Data {
        var data = Data()
        for scalar in text.unicodeScalars {
            let bytes = Array(String(scalar).utf8)
            guard data.count + bytes.count <= limit else { break }
            data.append(contentsOf: bytes)
        }
        return data
    }

    /// INJECT_TEXT: a 4-byte length and UTF-8
    public static func text(_ text: String) -> Data {
        let bytes = utf8Prefix(text, 300)
        var d = Data([1])
        d.appendBE(UInt32(bytes.count))
        d.append(bytes)
        return d
    }

    /// INJECT_SCROLL_EVENT: 21 bytes; scrolls run from -16 to 16 wheel steps
    public static func scroll(x: Int32, y: Int32, width: UInt16, height: UInt16, h: Float, v: Float) -> Data {
        var d = Data([3])
        d.appendBE(UInt32(bitPattern: x))
        d.appendBE(UInt32(bitPattern: y))
        d.appendBE(width)
        d.appendBE(height)
        d.appendBE(UInt16(bitPattern: fixed(h / 16)))
        d.appendBE(UInt16(bitPattern: fixed(v / 16)))
        d.appendBE(UInt32(0))
        return d
    }

    /// SET_CLIPBOARD: the phone's clipboard takes the text and, with paste,
    /// pastes it where the focus is (scrcpy's Ctrl+V)
    public static func setClipboard(_ text: String, paste: Bool) -> Data {
        // scrcpy's SC_CONTROL_MSG_CLIPBOARD_TEXT_MAX_LENGTH
        let bytes = utf8Prefix(text, (1 << 18) - 14)
        var d = Data([9])
        d.appendBE(UInt64(0))  // sequence: no acknowledgement
        d.append(paste ? 1 : 0)
        d.appendBE(UInt32(bytes.count))
        d.append(bytes)
        return d
    }

    /// BACK_OR_SCREEN_ON down or up
    public static func back(down: Bool) -> Data { Data([4, down ? 0 : 1]) }

    /// scrcpy's sc_float_to_i16fp: [-1, 1] as a signed 16-bit fixed point
    static func fixed(_ value: Float) -> Int16 {
        let clamped = min(max(value, -1), 1)
        return clamped >= 1 ? Int16.max : Int16((clamped * 0x8000).rounded(.towardZero))
    }

    /// The largest video packet and clipboard text taken from the phone, far
    /// above real ones; a corrupt length ends the stream instead of
    /// allocating gigabytes
    static let maxPacket = 64 << 20
    static let maxClipboard = 4 << 20

    static func checkedLength(_ length: UInt32, max: Int) throws -> Int {
        guard Int(length) <= max else { throw DMBError("scrcpy sent \(length) bytes, over the \(max) it takes") }
        return Int(length)
    }

    /// The NAL units of an Annex B buffer, without their start codes
    public static func nalUnits(_ data: Data) -> [Data] {
        var units: [Data] = []
        let bytes = [UInt8](data)
        var i = 0
        var start: Int?
        while i + 2 < bytes.count {
            if bytes[i] == 0, bytes[i + 1] == 0, bytes[i + 2] == 1 {
                if let s = start {
                    var end = i
                    if end > s, bytes[end - 1] == 0 { end -= 1 }
                    units.append(Data(bytes[s..<end]))
                }
                i += 3
                start = i
            } else {
                i += 1
            }
        }
        if let s = start, s < bytes.count { units.append(Data(bytes[s...])) }
        return units
    }
}

extension Data {
    mutating func appendBE<T: FixedWidthInteger>(_ value: T) {
        Swift.withUnsafeBytes(of: value.bigEndian) { append(contentsOf: $0) }
    }
}

/// A socket that closes only once no read or write is using it, so no
/// call ever reaches a closed descriptor, or one reused for another file.
final class SocketBox {
    private let lock = NSLock()
    private let fd: Int32
    private var users = 0
    private var closed = false

    init(_ fd: Int32) { self.fd = fd }

    /// Runs `body` with the descriptor; nil once the socket is closed
    func use<T>(_ body: (Int32) throws -> T) rethrows -> T? {
        lock.lock()
        guard !closed else { lock.unlock(); return nil }
        users += 1
        lock.unlock()
        defer {
            lock.lock()
            users -= 1
            let last = closed && users == 0
            lock.unlock()
            if last { Darwin.close(fd) }
        }
        return try body(fd)
    }

    /// Wakes a blocked read or write; the descriptor closes once it returns
    func close() {
        lock.lock()
        guard !closed else { lock.unlock(); return }
        closed = true
        shutdown(fd, SHUT_RDWR)
        let idle = users == 0
        lock.unlock()
        if idle { Darwin.close(fd) }
    }

    /// Exactly `count` bytes, retrying interrupted reads
    func read(_ count: Int) throws -> Data {
        var data = Data(count: count)
        var offset = 0
        try data.withUnsafeMutableBytes { buffer in
            while offset < count {
                guard let (n, error) = use({ fd -> (Int, Int32) in
                    let n = recv(fd, buffer.baseAddress! + offset, count - offset, 0)
                    return (n, errno)
                }) else { throw DMBError("scrcpy stream ended") }
                if n < 0 && error == EINTR { continue }
                if n <= 0 { throw DMBError("scrcpy stream ended") }
                offset += n
            }
        }
        return data
    }

    /// All of `data`, retrying interrupted writes; false once the socket fails
    @discardableResult
    func write(_ data: Data) -> Bool {
        data.withUnsafeBytes { buffer in
            var offset = 0
            while offset < buffer.count {
                guard let (n, error) = use({ fd -> (Int, Int32) in
                    let n = Darwin.send(fd, buffer.baseAddress! + offset, buffer.count - offset, 0)
                    return (n, errno)
                }) else { return false }
                if n < 0 && error == EINTR { continue }
                if n <= 0 { return false }
                offset += n
            }
            return true
        }
    }
}

/// One running scrcpy server and its sockets.
public final class ScrcpySession {
    public let serial: String
    private let adb: String
    private let server: String
    private let maxSize: Int
    private let writeQueue = DispatchQueue(label: "dmb.scrcpy.control")
    private var format: CMVideoFormatDescription?
    private var pendingConfig: [Data] = []
    /// Guards the state below, which `start`, `stop`, the reader threads
    /// and the write queue share
    private let lock = NSLock()
    private var shell: Process?
    private var port: Int = 0
    private var video: SocketBox?
    private var control: SocketBox?
    private var isStopped = false
    private var size = CGSize.zero
    /// The video's current size, which touch positions are relative to
    public var videoSize: CGSize { locked { size } }
    public var onFrame: ((CMSampleBuffer) -> Void)?
    public var onStatus: ((String) -> Void)?
    /// Text the phone copied, off the main thread
    public var onClipboard: ((String) -> Void)?
    public private(set) var deviceName = ""

    /// `server` is scrcpy-server 5.0.1 on this Mac; `maxSize` caps the video's long side
    public init(serial: String, adb: String, server: String, maxSize: Int = 0) {
        self.serial = serial
        self.adb = adb
        self.server = server
        self.maxSize = maxSize
    }

    private func locked<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }

    private var stopped: Bool { locked { isStopped } }

    /// Keeps a resource `start` made, or hands it back for release when
    /// `stop` already ran
    private func keep(_ body: () -> Void) -> Bool {
        locked {
            guard !isStopped else { return false }
            body()
            return true
        }
    }

    /// adb from ADB, on PATH, bundled with Twin, next to an Android SDK, or
    /// where Homebrew puts it
    public static func findADB() -> String? {
        let executable = Bundle.main.executableURL ?? URL(fileURLWithPath: CommandLine.arguments[0])
        return adbCandidates(environment: ProcessInfo.processInfo.environment,
                             executable: executable.resolvingSymlinksInPath(),
                             home: FileManager.default.homeDirectoryForCurrentUser.path)
            .first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    /// Where `findADB` looks, in order. Twin bundles adb in
    /// Twin.app/Contents/Helpers/platform-tools: the app this binary runs
    /// inside (Twin Mirror.app and dmb sit within it), then the installed one.
    static func adbCandidates(environment: [String: String], executable: URL, home: String) -> [String] {
        var candidates = [environment["ADB"]].compactMap { $0 }
        candidates += (environment["PATH"] ?? "").split(separator: ":").map { "\($0)/adb" }
        var dir = executable.deletingLastPathComponent()
        while dir.pathComponents.count > 1 {
            if dir.pathExtension == "app" {
                candidates.append(dir.appendingPathComponent("Contents/Helpers/platform-tools/adb").path)
            }
            dir = dir.deletingLastPathComponent()
        }
        candidates += ["/Applications/Twin.app/Contents/Helpers/platform-tools/adb",
                       "\(home)/Applications/Twin.app/Contents/Helpers/platform-tools/adb",
                       "\(home)/Library/Android/sdk/platform-tools/adb", "/opt/homebrew/bin/adb", "/usr/local/bin/adb"]
        return candidates
    }

    @discardableResult
    private func run(_ args: [String]) throws -> String {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: adb)
        p.arguments = ["-s", serial] + args
        let out = Pipe()
        p.standardOutput = out
        p.standardError = out
        try p.run()
        let data = out.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        let text = String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        guard p.terminationStatus == 0 else { throw DMBError("adb \(args.first ?? ""): \(text)") }
        return text
    }

    /// Pushes and starts the server, connects both sockets and reads video on
    /// a thread of its own until `stop()`; a failure releases what it started
    public func start() throws {
        let (video, control): (SocketBox, SocketBox)
        do { (video, control) = try connect() } catch { stop(); throw error }
        Thread.detachNewThread { [weak self] in self?.readVideo(video) }
        Thread.detachNewThread { [weak self] in self?.drainControl(control) }
    }

    private func connect() throws -> (SocketBox, SocketBox) {
        let gone = DMBError("scrcpy stopped")
        try run(["push", server, ScrcpyProtocol.devicePath])
        let scid = String(format: "%08x", UInt32.random(in: 1..<0x7FFF_FFFF))
        // DMB_SCRCPY_PORT fixes the port, for tests that tunnel it
        let fixed = ProcessInfo.processInfo.environment["DMB_SCRCPY_PORT"]
        let portText = try run(["forward", "tcp:\(fixed ?? "0")", "localabstract:scrcpy_\(scid)"])
        guard let port = fixed.flatMap(Int.init) ?? Int(portText.split(separator: "\n").last ?? "") else {
            throw DMBError("adb forward: \(portText)")
        }
        guard keep({ self.port = port }) else { removeForward(port); throw gone }
        let shell = Process()
        shell.executableURL = URL(fileURLWithPath: adb)
        var args = ["-s", serial, "shell", "CLASSPATH=\(ScrcpyProtocol.devicePath)", "app_process", "/",
                    "com.genymobile.scrcpy.Server", ScrcpyProtocol.version, "scid=\(scid)", "log_level=warn",
                    "audio=false", "tunnel_forward=true", "video_codec=h264", "stay_awake=true", "cleanup=true"]
        if maxSize > 0 { args.append("max_size=\(maxSize)") }
        shell.arguments = args
        shell.standardOutput = FileHandle.nullDevice
        shell.standardError = FileHandle.nullDevice
        try shell.run()
        guard keep({ self.shell = shell }) else { shell.terminate(); throw gone }

        // The server listens a moment after it starts; until then adb accepts
        // and closes, so a connection counts once its dummy byte arrives
        var videoFD: Int32 = -1
        for _ in 0..<100 where !stopped {
            let fd = Self.connect(port: port)
            if fd >= 0 {
                var byte: UInt8 = 0
                if recv(fd, &byte, 1, 0) == 1 { videoFD = fd; break }
                close(fd)
            }
            usleep(100_000)
        }
        guard videoFD >= 0 else { throw DMBError("scrcpy server did not answer on \(serial)") }
        let video = SocketBox(videoFD)
        guard keep({ self.video = video }) else { video.close(); throw gone }
        let controlFD = Self.connect(port: port)
        guard controlFD >= 0 else { throw DMBError("no scrcpy control socket") }
        let control = SocketBox(controlFD)
        guard keep({ self.control = control }) else { control.close(); throw gone }
        let name = try video.read(64)
        deviceName = String(decoding: name.prefix { $0 != 0 }, as: UTF8.self)
        return (video, control)
    }

    public func stop() {
        let (video, control, shell, port) = locked { () -> (SocketBox?, SocketBox?, Process?, Int) in
            guard !isStopped else { return (nil, nil, nil, 0) }
            isStopped = true
            defer { (self.video, self.control, self.shell, self.port) = (nil, nil, nil, 0) }
            return (self.video, self.control, self.shell, self.port)
        }
        // Closing wakes the reader threads blocked in recv
        video?.close()
        control?.close()
        shell?.terminate()
        if port > 0 { removeForward(port) }
    }

    /// Removes the port forward off the calling thread, waiting a moment at
    /// most, so quitting the app never hangs on adb
    private func removeForward(_ port: Int) {
        let done = DispatchSemaphore(value: 0)
        DispatchQueue.global(qos: .utility).async {
            _ = try? self.run(["forward", "--remove", "tcp:\(port)"])
            done.signal()
        }
        _ = done.wait(timeout: .now() + 0.5)
    }

    public func send(_ message: Data) {
        writeQueue.async { [weak self] in
            guard let self, let control = self.locked({ self.isStopped ? nil : self.control }) else { return }
            control.write(message)
        }
    }

    private static func connect(port: Int) -> Int32 {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return -1 }
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = in_port_t(UInt16(port).bigEndian)
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        let ok = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard ok == 0 else { close(fd); return -1 }
        var one: Int32 = 1
        setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, socklen_t(MemoryLayout<Int32>.size))
        // A send on a dropped phone returns EPIPE instead of raising SIGPIPE
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
        return fd
    }

    private func readVideo(_ video: SocketBox) {
        do {
            let codec = try video.read(4).be32(0)
            guard codec == ScrcpyProtocol.codecH264 else { throw DMBError("unexpected codec \(codec)") }
            while !stopped {
                let header = try video.read(12)
                if header[0] & 0x80 != 0 {
                    // A session packet: the video's new size, as after a rotation
                    let size = CGSize(width: CGFloat(header.be32(4)), height: CGFloat(header.be32(8)))
                    locked { self.size = size }
                    continue
                }
                let pts = header.be64(0)
                let length = try ScrcpyProtocol.checkedLength(header.be32(8), max: ScrcpyProtocol.maxPacket)
                let packet = try video.read(length)
                if pts & ScrcpyProtocol.flagConfig != 0 {
                    pendingConfig = ScrcpyProtocol.nalUnits(packet)
                    continue
                }
                handle(packet, keyFrame: pts & ScrcpyProtocol.flagKeyFrame != 0)
            }
        } catch {
            if !stopped { onStatus?("\(error)") }
        }
    }

    /// Turns an Annex B frame into a sample buffer the display layer decodes
    private func handle(_ packet: Data, keyFrame: Bool) {
        var units = ScrcpyProtocol.nalUnits(packet)
        // A config packet comes alone before the frame it describes; some
        // encoders also put SPS and PPS inside the key frame
        let parameterSets = pendingConfig + units.filter { [7, 8].contains(($0.first ?? 0) & 0x1F) }
        if !parameterSets.isEmpty {
            let sps = parameterSets.last { ($0.first ?? 0) & 0x1F == 7 }
            let pps = parameterSets.last { ($0.first ?? 0) & 0x1F == 8 }
            if let sps, let pps { format = Self.formatDescription(sps: sps, pps: pps) ?? format }
            pendingConfig = []
        }
        units.removeAll { [7, 8, 9].contains(($0.first ?? 0) & 0x1F) }
        guard let format, !units.isEmpty else { return }
        var avcc = Data()
        for unit in units {
            avcc.appendBE(UInt32(unit.count))
            avcc.append(unit)
        }
        guard let sample = Self.sampleBuffer(avcc, format: format, keyFrame: keyFrame) else { return }
        let dims = CMVideoFormatDescriptionGetDimensions(format)
        locked { size = CGSize(width: CGFloat(dims.width), height: CGFloat(dims.height)) }
        onFrame?(sample)
    }

    static func formatDescription(sps: Data, pps: Data) -> CMVideoFormatDescription? {
        var format: CMVideoFormatDescription?
        sps.withUnsafeBytes { s in
            pps.withUnsafeBytes { p in
                let pointers = [s.bindMemory(to: UInt8.self).baseAddress!, p.bindMemory(to: UInt8.self).baseAddress!]
                let sizes = [sps.count, pps.count]
                CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    allocator: nil, parameterSetCount: 2, parameterSetPointers: pointers,
                    parameterSetSizes: sizes, nalUnitHeaderLength: 4, formatDescriptionOut: &format)
            }
        }
        return format
    }

    static func sampleBuffer(_ avcc: Data, format: CMVideoFormatDescription, keyFrame: Bool) -> CMSampleBuffer? {
        var block: CMBlockBuffer?
        guard CMBlockBufferCreateWithMemoryBlock(allocator: nil, memoryBlock: nil, blockLength: avcc.count,
                                                 blockAllocator: nil, customBlockSource: nil, offsetToData: 0,
                                                 dataLength: avcc.count, flags: 0, blockBufferOut: &block) == noErr,
              let block else { return nil }
        avcc.withUnsafeBytes { _ = CMBlockBufferReplaceDataBytes(with: $0.baseAddress!, blockBuffer: block,
                                                                  offsetIntoDestination: 0, dataLength: avcc.count) }
        var sample: CMSampleBuffer?
        var size = avcc.count
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: CMClockGetTime(CMClockGetHostTimeClock()),
                                        decodeTimeStamp: .invalid)
        CMSampleBufferCreateReady(allocator: nil, dataBuffer: block, formatDescription: format, sampleCount: 1,
                                  sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 1,
                                  sampleSizeArray: &size, sampleBufferOut: &sample)
        if let sample, !keyFrame,
           let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true),
           CFArrayGetCount(attachments) > 0 {
            let dict = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(dict, Unmanaged.passUnretained(kCMSampleAttachmentKey_NotSync).toOpaque(),
                                 Unmanaged.passUnretained(kCFBooleanTrue).toOpaque())
        }
        return sample
    }

    /// Device messages: the phone's clipboard goes to `onClipboard`;
    /// acknowledgements and UHID output are read and dropped, so the server
    /// never blocks on them
    private func drainControl(_ control: SocketBox) {
        do {
            while !stopped {
                switch try control.read(1)[0] {
                case 0:  // CLIPBOARD
                    let length = try ScrcpyProtocol.checkedLength(control.read(4).be32(0), max: ScrcpyProtocol.maxClipboard)
                    let text = String(decoding: try control.read(length), as: UTF8.self)
                    onClipboard?(text)
                case 1:  // ACK_CLIPBOARD
                    _ = try control.read(8)
                case 2:  // UHID_OUTPUT
                    let header = try control.read(4)
                    _ = try control.read(Int(header[header.startIndex + 2]) << 8 | Int(header[header.startIndex + 3]))
                default:
                    return
                }
            }
        } catch {}
    }
}

extension Data {
    func be32(_ offset: Int) -> UInt32 {
        self[startIndex + offset..<startIndex + offset + 4].reduce(0) { $0 << 8 | UInt32($1) }
    }

    func be64(_ offset: Int) -> UInt64 {
        self[startIndex + offset..<startIndex + offset + 8].reduce(0) { $0 << 8 | UInt64($1) }
    }
}
