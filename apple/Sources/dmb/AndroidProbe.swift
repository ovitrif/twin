import AppKit
import CoreImage
import DMBCore
import VideoToolbox

/// `dmb android-probe --serial S --server PATH [--png OUT] [--swipe X1 Y1 X2 Y2]`:
/// checks the scrcpy client headless. Counts frames for three seconds, saves
/// the first decoded key frame and, with --swipe (0 to 1 across the screen),
/// drags a finger the way the mirror does.
func androidProbe(_ args: [String]) {
    var serial: String?, server: String?, png: String?
    var swipe: [CGFloat] = []
    var it = args.makeIterator()
    while let arg = it.next() {
        switch arg {
        case "--serial": serial = it.next()
        case "--server": server = it.next()
        case "--png": png = it.next()
        case "--swipe": swipe = (0..<4).compactMap { _ in it.next().flatMap { Double($0) }.map { CGFloat($0) } }
        default: break
        }
    }
    guard let serial, let server, let adb = ScrcpySession.findADB() else { fail("android-probe needs --serial, --server and adb") }
    let session = ScrcpySession(serial: serial, adb: adb, server: server)
    var frames = 0
    var keyFrames = 0
    var saved = false
    var decoder: VTDecompressionSession?
    session.onStatus = { print("status: \($0)") }
    session.onFrame = { sample in
        frames += 1
        let sync = !(((CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[CFString: Any]])?
            .first?[kCMSampleAttachmentKey_NotSync] as? Bool) ?? false)
        if sync { keyFrames += 1 }
        guard let png, !saved, sync, let format = CMSampleBufferGetFormatDescription(sample) else { return }
        saved = true
        VTDecompressionSessionCreate(allocator: nil, formatDescription: format, decoderSpecification: nil,
                                     imageBufferAttributes: nil, outputCallback: nil, decompressionSessionOut: &decoder)
        guard let decoder else { print("no decoder"); return }
        VTDecompressionSessionDecodeFrame(decoder, sampleBuffer: sample, flags: [], infoFlagsOut: nil) { status, _, image, _, _ in
            guard status == noErr, let image else { print("decode failed \(status)"); return }
            let ci = CIImage(cvPixelBuffer: image)
            if let cg = CIContext().createCGImage(ci, from: ci.extent),
               let data = NSBitmapImageRep(cgImage: cg).representation(using: .png, properties: [:]) {
                try? data.write(to: URL(fileURLWithPath: png))
                print("saved \(png) \(cg.width)x\(cg.height)")
            }
        }
    }
    do { try session.start() } catch { fail("\(error)") }
    print("connected to \(session.deviceName)")
    Thread.sleep(forTimeInterval: 3)
    let size = session.videoSize
    print("frames \(frames) key frames \(keyFrames) video \(Int(size.width))x\(Int(size.height))")
    if swipe.count == 4, size.width > 0 {
        let w = UInt16(size.width), h = UInt16(size.height)
        func point(_ t: CGFloat) -> (Int32, Int32) {
            (Int32((swipe[0] + (swipe[2] - swipe[0]) * t) * size.width),
             Int32((swipe[1] + (swipe[3] - swipe[1]) * t) * size.height))
        }
        let start = point(0)
        session.send(ScrcpyProtocol.touch(.down, x: start.0, y: start.1, width: w, height: h))
        for step in 1...12 {
            usleep(16_000)
            let p = point(CGFloat(step) / 12)
            session.send(ScrcpyProtocol.touch(.move, x: p.0, y: p.1, width: w, height: h))
        }
        let end = point(1)
        session.send(ScrcpyProtocol.touch(.up, x: end.0, y: end.1, width: w, height: h))
        print("swiped")
        Thread.sleep(forTimeInterval: 1.5)
    }
    session.stop()
}

/// `dmb iphone-probe --udid U --helper PATH --png OUT`: checks the iPhone's
/// Device Hub stream headless. Decodes the first key frame through the same
/// assembler the mirror uses and saves it cropped to the screen's size.
func iphoneProbe(_ args: [String]) {
    var udid: String?, helper: String?, png: String?
    var it = args.makeIterator()
    while let arg = it.next() {
        switch arg {
        case "--udid": udid = it.next()
        case "--helper": helper = it.next()
        case "--png": png = it.next()
        default: break
        }
    }
    guard let udid, let helper, let png else { fail("iphone-probe needs --udid, --helper and --png") }
    let process = Process()
    process.executableURL = URL(fileURLWithPath: helper)
    process.arguments = [udid, "--video"]
    let input = Pipe(), output = Pipe(), errors = Pipe()
    process.standardInput = input
    process.standardOutput = output
    process.standardError = errors
    var screen = CGSize.zero
    errors.fileHandleForReading.readabilityHandler = { handle in
        for line in String(decoding: handle.availableData, as: UTF8.self).split(separator: "\n") {
            print("helper: \(line)")
            let parts = line.split(separator: " ")
            if parts.count == 3, parts[0] == "screen", let w = Double(parts[1]), let h = Double(parts[2]) {
                screen = CGSize(width: w, height: h)
            }
        }
    }
    do { try process.run() } catch { fail("\(error)") }
    let assembler = HEVCAssembler()
    var buffer = Data(), units = 0, done = false
    let deadline = Date().addingTimeInterval(15)
    while !done && Date() < deadline {
        buffer.append(output.fileHandleForReading.availableData)
        while buffer.count >= 4 {
            let length = buffer.prefix(4).reduce(0) { $0 << 8 | Int($1) }
            guard buffer.count >= 4 + length else { break }
            let unit = buffer.subdata(in: buffer.startIndex + 4..<buffer.startIndex + 4 + length)
            buffer.removeSubrange(buffer.startIndex..<buffer.startIndex + 4 + length)
            units += 1
            guard !done, let sample = assembler.sampleBuffer(accessUnit: unit),
                  let format = CMSampleBufferGetFormatDescription(sample) else { continue }
            var decoder: VTDecompressionSession?
            VTDecompressionSessionCreate(allocator: nil, formatDescription: format, decoderSpecification: nil,
                                         imageBufferAttributes: nil, outputCallback: nil, decompressionSessionOut: &decoder)
            guard let decoder else { continue }
            VTDecompressionSessionDecodeFrame(decoder, sampleBuffer: sample, flags: [], infoFlagsOut: nil) { status, _, image, _, _ in
                guard status == noErr, let image else { print("decode failed \(status)"); return }
                let ci = CIImage(cvPixelBuffer: image)
                // Keep the screen, at the top left; the padding is right and bottom
                let crop = screen.width > 0
                    ? CGRect(x: 0, y: ci.extent.height - screen.height, width: screen.width, height: screen.height) : ci.extent
                if let cg = CIContext().createCGImage(ci, from: crop),
                   let data = NSBitmapImageRep(cgImage: cg).representation(using: .png, properties: [:]) {
                    try? data.write(to: URL(fileURLWithPath: png))
                    print("decoded \(Int(ci.extent.width))x\(Int(ci.extent.height)), saved \(cg.width)x\(cg.height)")
                    done = true
                }
            }
        }
        usleep(20_000)
    }
    print("access units \(units)")
    try? input.fileHandleForWriting.close()
    usleep(500_000)
    if process.isRunning { process.terminate() }
}
