import CoreMedia
import Foundation
import Testing
@testable import DMBCore

/// The clip's access units: a new one starts at each picture's first slice,
/// with the parameter sets before it
private func accessUnits(_ stream: Data) -> [Data] {
    var units: [Data] = []
    var current = Data()
    var hasSlice = false
    for nal in ScrcpyProtocol.nalUnits(stream) {
        let type = HEVCAssembler.nalType(nal)
        let firstSlice = type <= 31 && nal.count > 2 && nal[nal.startIndex + 2] & 0x80 != 0
        if hasSlice && (firstSlice || (32...34).contains(type)) {
            units.append(current)
            current = Data()
            hasSlice = false
        }
        current.append(contentsOf: [0, 0, 0, 1])
        current.append(nal)
        if type <= 31 { hasSlice = true }
    }
    if hasSlice { units.append(current) }
    return units
}

@Test func assemblesTheStreamIntoSampleBuffers() throws {
    let url = try #require(Bundle.module.url(forResource: "testsrc", withExtension: "hevc", subdirectory: "Fixtures"))
    let units = accessUnits(try Data(contentsOf: url))
    #expect(units.count >= 2)
    let assembler = HEVCAssembler()
    let samples = units.compactMap { assembler.sampleBuffer(accessUnit: $0) }
    #expect(samples.count == units.count)
    #expect(assembler.codedSize == CGSize(width: 128, height: 256))
    let first = try #require(samples.first)
    let attachments = CMSampleBufferGetSampleAttachmentsArray(first, createIfNecessary: false) as? [[CFString: Any]]
    #expect(attachments?.first?[kCMSampleAttachmentKey_NotSync] as? Bool != true)  // the key frame
    #expect(CMFormatDescriptionGetMediaSubType(CMSampleBufferGetFormatDescription(first)!) == kCMVideoCodecType_HEVC)
}

@Test func waitsForParameterSetsBeforeTheFirstFrame() {
    // A slice alone, before any VPS, SPS and PPS, cannot be described yet
    let slice = Data([0, 0, 0, 1, 0x02, 0x01, 0x80, 0x00])
    #expect(HEVCAssembler().sampleBuffer(accessUnit: slice) == nil)
}
