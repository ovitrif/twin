import CoreGraphics
import Foundation
import Testing
@testable import DMBCore

@Test func parsesWiredPhonesAndSkipsSimulators() throws {
    let json = """
    {"result": {"devices": [
      {"properties": {
        "connection": {"transportType": "wired"},
        "hardware": {"reality": "physical", "platform": "iOS", "udid": "00008150-AAAA",
                     "marketingName": "iPhone Air", "productType": "iPhone18,4"},
        "software": {"osVersionNumber": "27.0"},
        "state": {"name": "Test iPhone"}}},
      {"hardwareProperties": {"reality": "physical", "platform": "iOS", "udid": "00008110-BBBB",
                              "marketingName": "iPhone 15 Pro", "productType": "iPhone16,1"},
       "connectionProperties": {"transportType": "localNetwork"},
       "deviceProperties": {"name": "Old Format", "osVersionNumber": "18.6"}},
      {"properties": {"hardware": {"reality": "simulated", "platform": "iOS", "udid": "SIM"}}}
    ]}}
    """
    let devices = try Devices.parse(Data(json.utf8))
    #expect(devices == [
        Device(udid: "00008150-AAAA", name: "Test iPhone", model: "iPhone Air", productType: "iPhone18,4", wired: true, osVersion: "27.0"),
        Device(udid: "00008110-BBBB", name: "Old Format", model: "iPhone 15 Pro", productType: "iPhone16,1", wired: false, osVersion: "18.6"),
    ])
}

@Test func mapsViewPointsToPhonePoints() {
    let view = CGSize(width: 322, height: 700)
    let phone = CGSize(width: 420, height: 912)
    #expect(Screen.devicePoint(CGPoint(x: 161, y: 350), view: view, device: phone) == CGPoint(x: 210, y: 456))
    #expect(Screen.devicePoint(CGPoint(x: -5, y: 800), view: view, device: phone) == CGPoint(x: 0, y: 911))
}

@Test func roundsFaceIDPhonesOnly() {
    #expect(Screen.cornerRatio(productType: "iPhone18,4") == 0.14)
    #expect(Screen.cornerRatio(productType: "iPhone14,6") == 0)
    #expect(Screen.cornerRatio(productType: "iPhone9,1") == 0)
}

@Test func replaysAGestureWithItsTiming() throws {
    let gesture = Gesture(points: [CGPoint(x: 10, y: 20), CGPoint(x: 10, y: 120), CGPoint(x: 10, y: 120)],
                          times: [0, 0.2, 0.7])
    let pointer = try #require((gesture.actions()["actions"] as? [[String: Any]])?.first)
    let steps = try #require(pointer["actions"] as? [[String: Any]])
    #expect(steps.map { $0["type"] as? String } == ["pointerMove", "pointerDown", "pointerMove", "pointerMove", "pointerUp"])
    #expect(steps.map { $0["duration"] as? Int } == [0, nil, 200, 500, nil])
}
