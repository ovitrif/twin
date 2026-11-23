import Foundation
import Testing
@testable import MirrorUI

@Test func aLineSplitAcrossChunksArrivesWhole() {
    var buffer = LineBuffer()
    #expect(buffer.append(Data("rea".utf8)) == [])
    #expect(buffer.append(Data("dy\nscreen 1206".utf8)) == ["ready"])
    #expect(buffer.append(Data(" 2622\n\nerr\n".utf8)) == ["screen 1206 2622", "err"])
}
