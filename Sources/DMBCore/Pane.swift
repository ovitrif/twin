import Foundation

/// A request to hide or show one phone of a Twin pair, sent by `dmb pane`
/// as a distributed notification addressed by the pair's iPhone UDID
public struct PaneRequest {
    public enum Phone: String { case android, iphone }

    public static let name = Notification.Name("tech.masivo.twin.pane")

    public let pane: Phone
    public let visible: Bool

    public init(pane: Phone, visible: Bool) {
        self.pane = pane
        self.visible = visible
    }

    public init?(_ info: [AnyHashable: Any]?) {
        guard let pane = (info?["pane"] as? String).flatMap(Phone.init(rawValue:)),
              let visible = info?["visible"] as? String else { return nil }
        self.init(pane: pane, visible: visible == "1")
    }

    /// Posts this request to the pair showing the iPhone `udid`
    public func post(udid: String) {
        DistributedNotificationCenter.default().postNotificationName(
            Self.name, object: udid, userInfo: ["pane": pane.rawValue, "visible": visible ? "1" : "0"],
            deliverImmediately: true)
    }
}
