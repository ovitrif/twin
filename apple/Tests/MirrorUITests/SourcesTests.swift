import Foundation
import Testing
@testable import MirrorUI

@Test func aMotionJPEGBufferKeepsTheNewestFrameAndHasACap() {
    var buffer = Data([1, 0xFF, 0xD8, 2, 0xFF, 0xD9, 0xFF, 0xD8, 3, 0xFF, 0xD9, 0xFF, 0xD8, 4])
    #expect(MJPEGSource.takeLatestJPEG(&buffer) == Data([0xFF, 0xD8, 3, 0xFF, 0xD9]))
    #expect(buffer == Data([0xFF, 0xD8, 4]))  // the next frame's start
    // A stream that never ends a frame stops growing at the cap
    var junk = Data(repeating: 7, count: 101)
    #expect(MJPEGSource.takeLatestJPEG(&junk, limit: 100) == nil)
    #expect(junk.isEmpty)
}

@Test func aStoppedMotionJPEGStreamNeverStartsAgain() {
    let source = MJPEGSource(url: URL(string: "http://127.0.0.1:9/")!)
    var statuses: [String] = []
    source.onStatus = { statuses.append($0) }
    source.stop()
    source.start()  // as the retry after a dropped stream does
    #expect(statuses.isEmpty)
}
