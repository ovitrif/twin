import AppKit
import CoreImage
import IMBCore
import VideoToolbox

/// `imb android-probe --serial S --server PATH [--png OUT] [--swipe X1 Y1 X2 Y2]`:
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
