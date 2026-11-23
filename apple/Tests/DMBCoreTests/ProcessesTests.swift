import Foundation
import Testing
@testable import DMBCore

@Test func aProcessThatIgnoresTerminateIsKilledAfterItsTimeout() throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/sh")
    process.arguments = ["-c", "trap '' TERM; exec sleep 30"]
    try process.run()
    let started = Date()
    #expect(process.waitUntilExit(timeout: 0.2, grace: 0.3) == false)
    #expect(!process.isRunning)
    #expect(Date().timeIntervalSince(started) < 5)
}

@Test func aQuickProcessEndsWithinItsTimeout() throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/true")
    try process.run()
    #expect(process.waitUntilExit(timeout: 5))
}

@Test func aMirrorPIDCountsOnlyWhileTwinMirrorHoldsIt() throws {
    let file = FileManager.default.temporaryDirectory.appendingPathComponent("mirror-\(UUID()).pid")
    try "4242\n".write(to: file, atomically: true, encoding: .utf8)
    #expect(MirrorPIDFile.read(file) { $0 == 4242 ? MirrorPIDFile.bundleIdentifier : nil } == 4242)
    #expect(FileManager.default.fileExists(atPath: file.path))
    // The pid now belongs to another app: the file is stale
    #expect(MirrorPIDFile.read(file) { _ in "com.apple.Safari" } == nil)
    #expect(!FileManager.default.fileExists(atPath: file.path))
}
