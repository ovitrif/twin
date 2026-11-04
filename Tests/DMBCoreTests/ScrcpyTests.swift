import Foundation
import Testing
@testable import DMBCore

// Byte layouts from scrcpy 5.0.1's app/tests/test_control_msg_serialize.c,
// with this client's pointer id, buttons and meta state

@Test func encodesATouchAsScrcpyDoes() {
    let bytes = [UInt8](ScrcpyProtocol.touch(.down, x: 100, y: 200, width: 1080, height: 1920))
    #expect(bytes == [
        2, 0,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE,  // generic finger (-2)
        0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0xC8,  // 100 200
        0x04, 0x38, 0x07, 0x80,                          // 1080 1920
        0xFF, 0xFF,                                      // pressure 1
        0, 0, 0, 0, 0, 0, 0, 0,
    ])
}

@Test func encodesAScrollAsScrcpyDoes() {
    let bytes = [UInt8](ScrcpyProtocol.scroll(x: 260, y: 1026, width: 1080, height: 1920, h: 16, v: -16))
    #expect(bytes == [
        3,
        0x00, 0x00, 0x01, 0x04, 0x00, 0x00, 0x04, 0x02,
        0x04, 0x38, 0x07, 0x80,
        0x7F, 0xFF, 0x80, 0x00,
        0, 0, 0, 0,
    ])
}

@Test func encodesKeysAndText() {
    #expect([UInt8](ScrcpyProtocol.key(66, down: false)) == [0, 1, 0, 0, 0, 0x42, 0, 0, 0, 0, 0, 0, 0, 0])
    #expect([UInt8](ScrcpyProtocol.text("hi")) == [1, 0, 0, 0, 2, 0x68, 0x69])
}

@Test func splitsAnnexBIntoNALUnits() {
    let stream = Data([0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 4, 5])
    #expect(ScrcpyProtocol.nalUnits(stream) == [Data([0x67, 1, 2]), Data([0x68, 3]), Data([0x65, 4, 5])])
}
