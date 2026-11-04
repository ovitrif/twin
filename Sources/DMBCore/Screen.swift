import CoreGraphics

public enum Screen {
    /// Display corner radius as a share of the screen width. Face ID iPhones
    /// round their displays at about 14% of the width (55 pt on a 393 pt
    /// iPhone 15 Pro); Touch ID ones are square.
    public static func cornerRatio(productType: String) -> CGFloat {
        let square: Set<String> = ["iPhone12,8", "iPhone14,6"]  // iPhone SE 2 and 3
        if square.contains(productType) { return 0 }
        guard productType.hasPrefix("iPhone"),
              let major = Int(productType.dropFirst(6).split(separator: ",").first ?? "")
        else { return 0.14 }
        return major >= 10 ? 0.14 : 0  // iPhone X (10,x) and later
    }

    /// A point in the mirror view (top-left origin) as a point on the phone,
    /// clamped to its screen.
    public static func devicePoint(_ p: CGPoint, view: CGSize, device: CGSize) -> CGPoint {
        guard view.width > 0, view.height > 0 else { return .zero }
        let x = (p.x / view.width * device.width).rounded()
        let y = (p.y / view.height * device.height).rounded()
        return CGPoint(x: min(max(x, 0), device.width - 1), y: min(max(y, 0), device.height - 1))
    }
}
