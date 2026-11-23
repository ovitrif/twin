import AppKit
import Testing
@testable import MirrorUI

@MainActor @Test func aPairTooNarrowForItsGapFitsNothingRatherThanLessThanNothing() {
    let pair = PairView(frame: .zero)
    pair.gap = 12
    #expect(pair.long(fitting: CGSize(width: 5, height: 600)) == 0)
    #expect(pair.long(fitting: .zero) == 0)
    let long = pair.long(fitting: CGSize(width: 1000, height: 600))
    #expect(long > 0 && long <= 600)
    #expect(pair.size(forLong: long).width <= 1000)
}
