import AVFoundation
import CoreMediaIO
import AppKit

/// Delivers the phone's screen as sample buffers, on any queue.
protocol FrameSource: AnyObject {
    var onFrame: ((CMSampleBuffer) -> Void)? { get set }
    var onStatus: ((String) -> Void)? { get set }
    func start()
    func stop()
}

/// The iPhone's screen over USB: the muxed capture device QuickTime uses.
final class CaptureSource: NSObject, FrameSource, AVCaptureVideoDataOutputSampleBufferDelegate {
    var onFrame: ((CMSampleBuffer) -> Void)?
    var onStatus: ((String) -> Void)?
    private let name: String?
    private let session = AVCaptureSession()
    private let queue = DispatchQueue(label: "imb.capture", qos: .userInteractive)
    private var observer: NSObjectProtocol?

    /// `name` is the phone's name as `devicectl` reports it; nil takes the first iPhone.
    init(name: String?) { self.name = name }

    static func allowScreenCaptureDevices() {
        var address = CMIOObjectPropertyAddress(
            mSelector: CMIOObjectPropertySelector(kCMIOHardwarePropertyAllowScreenCaptureDevices),
            mScope: CMIOObjectPropertyScope(kCMIOObjectPropertyScopeGlobal),
            mElement: CMIOObjectPropertyElement(kCMIOObjectPropertyElementMain))
        var allow: UInt32 = 1
        CMIOObjectSetPropertyData(CMIOObjectID(kCMIOObjectSystemObject), &address, 0, nil,
                                  UInt32(MemoryLayout<UInt32>.size), &allow)
    }

    func start() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: break
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { _ in DispatchQueue.main.async { self.start() } }
            return
        default:
            onStatus?("Camera access is off for iOS Mirror.\nTurn it on in System Settings › Privacy & Security › Camera.")
            return
        }
        Self.allowScreenCaptureDevices()
        onStatus?("Waiting for \(name ?? "the iPhone")…")
        // The phone shows up a moment after the property is set, and again
        // after each replug
        observer = NotificationCenter.default.addObserver(
            forName: AVCaptureDevice.wasConnectedNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.attach() }
        attach()
    }

    private func findDevice() -> AVCaptureDevice? {
        let devices = AVCaptureDevice.DiscoverySession(
            deviceTypes: [.external], mediaType: .muxed, position: .unspecified).devices
            .filter { $0.modelID == "iOS Device" }
        guard let name else { return devices.first }
        // devicectl and the capture device may spell the name apart; with one
        // iPhone plugged in it is that one
        return devices.first { $0.localizedName == name } ?? (devices.count == 1 ? devices.first : nil)
    }

    private func attach() {
        guard session.inputs.isEmpty, let device = findDevice() else { return }
        do {
            let input = try AVCaptureDeviceInput(device: device)
            let output = AVCaptureVideoDataOutput()
            output.alwaysDiscardsLateVideoFrames = true
            output.setSampleBufferDelegate(self, queue: queue)
            session.beginConfiguration()
            guard session.canAddInput(input), session.canAddOutput(output) else {
                session.commitConfiguration()
                onStatus?("\(device.localizedName) is busy in another app")
                return
            }
            session.addInput(input)
            session.addOutput(output)
            session.commitConfiguration()
            onStatus?("Starting \(device.localizedName)…")
            queue.async { self.session.startRunning() }
        } catch {
            onStatus?("Cannot open \(device.localizedName): \(error.localizedDescription)")
        }
    }

    func stop() {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        queue.async { self.session.stopRunning() }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer,
                       from connection: AVCaptureConnection) {
        onFrame?(sampleBuffer)
    }
}

/// Plays video files in turn, in a loop, as if they came from a phone, for
/// testing without one. A portrait file then a landscape one plays a rotation.
final class ReplaySource: FrameSource {
    var onFrame: ((CMSampleBuffer) -> Void)?
    var onStatus: ((String) -> Void)?
    private let urls: [URL]
    private let queue = DispatchQueue(label: "imb.replay", qos: .userInteractive)
    private var running = false

    init(urls: [URL]) { self.urls = urls }

    func start() {
        running = true
        queue.async { self.loop() }
    }

    func stop() { running = false }

    private func loop() {
        var index = 0
        while running {
            let url = urls[index % urls.count]
            index += 1
            let asset = AVURLAsset(url: url)
            guard let track = asset.tracks(withMediaType: .video).first,
                  let reader = try? AVAssetReader(asset: asset) else {
                DispatchQueue.main.async { self.onStatus?("Cannot read \(url.lastPathComponent)") }
                return
            }
            let output = AVAssetReaderTrackOutput(track: track, outputSettings: [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
            reader.add(output)
            reader.startReading()
            let started = CACurrentMediaTime()
            var first: CMTime?
            while running, let buffer = output.copyNextSampleBuffer() {
                let pts = CMSampleBufferGetPresentationTimeStamp(buffer)
                if first == nil { first = pts }
                let due = started + (pts - first!).seconds
                let wait = due - CACurrentMediaTime()
                if wait > 0 { Thread.sleep(forTimeInterval: wait) }
                onFrame?(buffer)
            }
        }
    }
}

/// A Motion JPEG stream such as WebDriverAgent's on port 9100, for a
/// simulator or a phone off the cable.
final class MJPEGSource: NSObject, FrameSource, URLSessionDataDelegate {
    var onFrame: ((CMSampleBuffer) -> Void)?
    var onStatus: ((String) -> Void)?
    private let url: URL
    private var session: URLSession?
    private var buffer = Data()

    init(url: URL) { self.url = url }

    func start() {
        onStatus?("Connecting to \(url.host ?? "stream")…")
        let session = URLSession(configuration: .ephemeral, delegate: self, delegateQueue: nil)
        session.dataTask(with: url).resume()
        self.session = session
    }

    func stop() { session?.invalidateAndCancel() }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        buffer.append(data)
        // Each part is one JPEG; keep only the newest complete one
        var latest: Data?
        while let soi = buffer.range(of: Data([0xFF, 0xD8])),
              let eoi = buffer.range(of: Data([0xFF, 0xD9]), in: soi.upperBound..<buffer.endIndex) {
            latest = buffer.subdata(in: soi.lowerBound..<eoi.upperBound)
            buffer.removeSubrange(buffer.startIndex..<eoi.upperBound)
        }
        if let latest, let frame = Self.sampleBuffer(jpeg: latest) { onFrame?(frame) }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard session === self.session else { return }
        DispatchQueue.main.async { self.onStatus?("Stream ended, retrying…") }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in self?.start() }
    }

    static func sampleBuffer(jpeg: Data) -> CMSampleBuffer? {
        guard let image = NSBitmapImageRep(data: jpeg)?.cgImage else { return nil }
        return sampleBuffer(image: image)
    }

    static func sampleBuffer(image: CGImage) -> CMSampleBuffer? {
        var pixels: CVPixelBuffer?
        let attrs = [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary
        CVPixelBufferCreate(nil, image.width, image.height, kCVPixelFormatType_32BGRA, attrs, &pixels)
        guard let pixels else { return nil }
        CVPixelBufferLockBaseAddress(pixels, [])
        let context = CGContext(data: CVPixelBufferGetBaseAddress(pixels), width: image.width,
                                height: image.height, bitsPerComponent: 8,
                                bytesPerRow: CVPixelBufferGetBytesPerRow(pixels),
                                space: CGColorSpaceCreateDeviceRGB(),
                                bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
                                    | CGBitmapInfo.byteOrder32Little.rawValue)
        context?.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        CVPixelBufferUnlockBaseAddress(pixels, [])
        var format: CMVideoFormatDescription?
        CMVideoFormatDescriptionCreateForImageBuffer(allocator: nil, imageBuffer: pixels, formatDescriptionOut: &format)
        guard let format else { return nil }
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: CMClockGetTime(CMClockGetHostTimeClock()),
                                        decodeTimeStamp: .invalid)
        var sample: CMSampleBuffer?
        CMSampleBufferCreateReadyWithImageBuffer(allocator: nil, imageBuffer: pixels, formatDescription: format,
                                                 sampleTiming: &timing, sampleBufferOut: &sample)
        return sample
    }
}
