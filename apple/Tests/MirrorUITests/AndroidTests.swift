import Foundation
import Testing
@testable import MirrorUI

@Test func touchesWaitForAVideoSizeScrcpyCanCarry() {
    #expect(ScrcpyInput.position(CGPoint(x: 0.5, y: 0.25), in: CGSize(width: 1080, height: 2400))! == (540, 600, 1080, 2400))
    #expect(ScrcpyInput.position(.zero, in: .zero) == nil)
    #expect(ScrcpyInput.position(.zero, in: CGSize(width: 1080, height: 0)) == nil)
    #expect(ScrcpyInput.position(.zero, in: CGSize(width: 70_000, height: 2400)) == nil)
    #expect(ScrcpyInput.position(.zero, in: CGSize(width: 1080, height: 70_000)) == nil)
}
