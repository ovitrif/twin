import CoreMedia
import Foundation

/// Turns H.265 access units in Annex B, as `dmb-hid --video` sends the
/// iPhone's Device Hub stream, into sample buffers a display layer decodes.
public final class HEVCAssembler {
    private var vps: Data?
    private var sps: Data?
    private var pps: Data?
    private var format: CMVideoFormatDescription?
    /// The coded size, which pads the screen to a multiple of 16
    public private(set) var codedSize = CGSize.zero

    public init() {}

    static func nalType(_ unit: Data) -> UInt8 { (unit[unit.startIndex] >> 1) & 0x3F }

    public func sampleBuffer(accessUnit: Data) -> CMSampleBuffer? {
        var slices: [Data] = []
        var keyFrame = false
        var changed = false
        for unit in ScrcpyProtocol.nalUnits(accessUnit) where unit.count >= 2 {
            switch Self.nalType(unit) {
            case 32: if vps != unit { vps = unit; changed = true }
            case 33: if sps != unit { sps = unit; changed = true }
            case 34: if pps != unit { pps = unit; changed = true }
            case 0...31:
                slices.append(unit)
                if (16...21).contains(Self.nalType(unit)) { keyFrame = true }  // IRAP pictures
            default: break  // access unit delimiters, SEI
            }
        }
        if changed || format == nil, let vps, let sps, let pps {
            format = Self.formatDescription(vps: vps, sps: sps, pps: pps) ?? format
            if let format {
                let dims = CMVideoFormatDescriptionGetDimensions(format)
                codedSize = CGSize(width: CGFloat(dims.width), height: CGFloat(dims.height))
            }
        }
        guard let format, !slices.isEmpty else { return nil }
        var avcc = Data()
        for slice in slices {
            avcc.appendBE(UInt32(slice.count))
            avcc.append(slice)
        }
        return ScrcpySession.sampleBuffer(avcc, format: format, keyFrame: keyFrame)
    }

    static func formatDescription(vps: Data, sps: Data, pps: Data) -> CMVideoFormatDescription? {
        var format: CMVideoFormatDescription?
        vps.withUnsafeBytes { v in
            sps.withUnsafeBytes { s in
                pps.withUnsafeBytes { p in
                    let pointers = [v, s, p].map { $0.bindMemory(to: UInt8.self).baseAddress! }
                    let sizes = [vps.count, sps.count, pps.count]
                    CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                        allocator: nil, parameterSetCount: 3, parameterSetPointers: pointers,
                        parameterSetSizes: sizes, nalUnitHeaderLength: 4, extensions: nil,
                        formatDescriptionOut: &format)
                }
            }
        }
        return format
    }
}
