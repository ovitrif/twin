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

@Test func disconnectingAndroidInPairCollapsesToIPhoneAndRestoresOnReconnect() {
    var state = PairPaneState(hasIPhone: true)
    #expect(state.layout() == .init(showsAndroid: true, showsIPhone: true, shouldTerminate: false))

    state.setConnected(.android, false)
    #expect(state.layout() == .init(showsAndroid: false, showsIPhone: true, shouldTerminate: false))

    state.setConnected(.android, true)
    #expect(state.layout() == .init(showsAndroid: true, showsIPhone: true, shouldTerminate: false))
}

@Test func disconnectingIPhoneInPairCollapsesToAndroidAndRestoresOnReconnect() {
    var state = PairPaneState(hasIPhone: true)
    state.setConnected(.iphone, false)
    #expect(state.layout() == .init(showsAndroid: true, showsIPhone: false, shouldTerminate: false))

    state.setConnected(.iphone, true)
    #expect(state.layout() == .init(showsAndroid: true, showsIPhone: true, shouldTerminate: false))
}

@Test func hidingTheOnlyConnectedPhoneClosesThePair() {
    var state = PairPaneState(hasIPhone: true)
    state.setConnected(.android, false)
    let layout = state.setUserVisible(.iphone, false)
    #expect(layout.shouldTerminate)
}

@Test func reconnectingAUserHiddenPhoneStaysHidden() {
    var state = PairPaneState(hasIPhone: true)
    _ = state.setUserVisible(.android, false)
    state.setConnected(.android, false)
    state.setConnected(.android, true)
    #expect(state.layout() == .init(showsAndroid: false, showsIPhone: true, shouldTerminate: false))
}

@Test func disconnectingAllPhonesKeepsOneCleanWaitingPane() {
    var pair = PairPaneState(hasIPhone: true)
    pair.setConnected(.android, false)
    pair.setConnected(.iphone, false)
    let bothDown = pair.layout()
    #expect(!bothDown.shouldTerminate)
    #expect(!bothDown.showsAndroid && bothDown.showsIPhone)
    #expect(bothDown.iphoneStatus == "Phones disconnected\nWaiting to reconnect…")

    var single = PairPaneState(hasIPhone: false, androidTitle: "Pixel")
    single.setConnected(.android, false)
    let singleDown = single.layout()
    #expect(!singleDown.shouldTerminate)
    #expect(singleDown.showsAndroid && !singleDown.showsIPhone)
    #expect(singleDown.androidStatus == "Pixel disconnected\nWaiting to reconnect…")
}
