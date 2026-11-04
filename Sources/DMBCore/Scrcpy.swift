import CoreMedia
import Foundation

// A scrcpy client for one Android phone: it runs scrcpy's own server on the
// phone (scrcpy-server 5.0.1, Apache-2.0, the build awb bundles), reads its
// H.264 video and sends touch and keys over its control socket. This is the
// part of scrcpy's desktop app Android v iOS needs, ported from its C client
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

    /// INJECT_TEXT: a 4-byte length and UTF-8
    public static func text(_ text: String) -> Data {
        let bytes = Data(text.utf8.prefix(300))
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

    /// BACK_OR_SCREEN_ON down or up
    public static func back(down: Bool) -> Data { Data([4, down ? 0 : 1]) }

    /// scrcpy's sc_float_to_i16fp: [-1, 1] as a signed 16-bit fixed point
    static func fixed(_ value: Float) -> Int16 {
        let clamped = min(max(value, -1), 1)
        return clamped >= 1 ? Int16.max : Int16((clamped * 0x8000).rounded(.towardZero))
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

/// One running scrcpy server and its sockets.
public final class ScrcpySession {
    public let serial: String
    private let adb: String
    private let server: String
    private let maxSize: Int
    private var shell: Process?
    private var port: Int = 0
    private var videoFD: Int32 = -1
    private var controlFD: Int32 = -1
    private let writeQueue = DispatchQueue(label: "dmb.scrcpy.control")
    private var format: CMVideoFormatDescription?
    private var pendingConfig: [Data] = []
    private var stopped = false
    /// The video's current size, which touch positions are relative to
    public private(set) var videoSize = CGSize.zero
    public var onFrame: ((CMSampleBuffer) -> Void)?
    public var onStatus: ((String) -> Void)?
    public private(set) var deviceName = ""

    /// `server` is scrcpy-server 5.0.1 on this Mac; `maxSize` caps the video's long side
    public init(serial: String, adb: String, server: String, maxSize: Int = 0) {
        self.serial = serial
        self.adb = adb
        self.server = server
        self.maxSize = maxSize
    }

    /// adb next to an Android SDK, on PATH, or where Homebrew puts it
    public static func findADB() -> String? {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        var candidates = [ProcessInfo.processInfo.environment["ADB"]].compactMap { $0 }
        candidates += (ProcessInfo.processInfo.environment["PATH"] ?? "").split(separator: ":").map { "\($0)/adb" }
        candidates += ["\(home)/Library/Android/sdk/platform-tools/adb", "/opt/homebrew/bin/adb", "/usr/local/bin/adb"]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
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
    /// a thread of its own until `stop()`
    public func start() throws {
        try run(["push", server, ScrcpyProtocol.devicePath])
        let scid = String(format: "%08x", UInt32.random(in: 1..<0x7FFF_FFFF))
        // DMB_SCRCPY_PORT fixes the port, for tests that tunnel it
        let fixed = ProcessInfo.processInfo.environment["DMB_SCRCPY_PORT"]
        let portText = try run(["forward", "tcp:\(fixed ?? "0")", "localabstract:scrcpy_\(scid)"])
        guard let port = fixed.flatMap(Int.init) ?? Int(portText.split(separator: "\n").last ?? "") else {
            throw DMBError("adb forward: \(portText)")
        }
        self.port = port
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
        self.shell = shell

        // The server listens a moment after it starts; until then adb accepts
        // and closes, so a connection counts once its dummy byte arrives
        var video: Int32 = -1
        for _ in 0..<100 {
            let fd = Self.connect(port: port)
            if fd >= 0 {
                var byte: UInt8 = 0
                if recv(fd, &byte, 1, 0) == 1 { video = fd; break }
                close(fd)
            }
            usleep(100_000)
        }
        guard video >= 0 else { stop(); throw DMBError("scrcpy server did not answer on \(serial)") }
        videoFD = video
        controlFD = Self.connect(port: port)
        guard controlFD >= 0 else { stop(); throw DMBError("no scrcpy control socket") }
        let name = try readExactly(videoFD, 64)
        deviceName = String(decoding: name.prefix { $0 != 0 }, as: UTF8.self)
        Thread.detachNewThread { [weak self] in self?.readVideo() }
        Thread.detachNewThread { [weak self] in self?.drainControl() }
    }

    public func stop() {
        guard !stopped else { return }
        stopped = true
        if videoFD >= 0 { close(videoFD) }
        if controlFD >= 0 { close(controlFD) }
        shell?.terminate()
        if port > 0 { _ = try? run(["forward", "--remove", "tcp:\(port)"]) }
    }

    public func send(_ message: Data) {
        writeQueue.async { [weak self] in
            guard let self, self.controlFD >= 0, !self.stopped else { return }
            message.withUnsafeBytes { buffer in
                var offset = 0
                while offset < buffer.count {
                    let n = Darwin.send(self.controlFD, buffer.baseAddress! + offset, buffer.count - offset, 0)
                    if n <= 0 { return }
                    offset += n
                }
            }
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
        return fd
    }

    private func readExactly(_ fd: Int32, _ count: Int) throws -> Data {
        var data = Data(count: count)
        var offset = 0
        try data.withUnsafeMutableBytes { buffer in
            while offset < count {
                let n = recv(fd, buffer.baseAddress! + offset, count - offset, 0)
                if n <= 0 { throw DMBError("scrcpy stream ended") }
                offset += n
            }
        }
        return data
    }

    private func readVideo() {
        do {
            let codec = try readExactly(videoFD, 4).be32(0)
            guard codec == ScrcpyProtocol.codecH264 else { throw DMBError("unexpected codec \(codec)") }
            while !stopped {
                let header = try readExactly(videoFD, 12)
                if header[0] & 0x80 != 0 {
                    // A session packet: the video's new size, as after a rotation
                    let size = CGSize(width: CGFloat(header.be32(4)), height: CGFloat(header.be32(8)))
                    videoSize = size
                    continue
                }
                let pts = header.be64(0)
                let length = Int(header.be32(8))
                let packet = try readExactly(videoFD, length)
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
        videoSize = CGSize(width: CGFloat(dims.width), height: CGFloat(dims.height))
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

    /// Device messages (clipboard, acknowledgements) are read and dropped so
    /// the server never blocks on them
    private func drainControl() {
        var buffer = [UInt8](repeating: 0, count: 4096)
        while !stopped, controlFD >= 0, recv(controlFD, &buffer, buffer.count, 0) > 0 {}
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
