import Foundation

extension Process {
    /// Waits for the process to end, up to `timeout`; then terminates it and,
    /// if it ignores that for `grace`, kills it. False when it had to be ended.
    public func waitUntilExit(timeout: TimeInterval, grace: TimeInterval = 2) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while isRunning && Date() < deadline { usleep(20_000) }
        guard isRunning else { return true }
        terminate()
        let killAt = Date().addingTimeInterval(grace)
        while isRunning && Date() < killAt { usleep(20_000) }
        if isRunning { kill(processIdentifier, SIGKILL) }
        waitUntilExit()
        return false
    }
}

/// The pid file `dmb mirror` writes for each phone it mirrors.
public enum MirrorPIDFile {
    public static let bundleIdentifier = "tech.masivo.twin.mirror"

    /// The pid in `file` while Twin Mirror still runs as it; a pid that ended
    /// or now belongs to another app leaves the file stale, and it is removed.
    /// A bare development build has no bundle identifier and reads as stale.
    public static func read(_ file: URL, bundleIdentifier: (pid_t) -> String?) -> pid_t? {
        guard let text = try? String(contentsOf: file, encoding: .utf8) else { return nil }
        guard let pid = pid_t(text.trimmingCharacters(in: .whitespacesAndNewlines)), pid > 0,
              bundleIdentifier(pid) == Self.bundleIdentifier else {
            try? FileManager.default.removeItem(at: file)
            return nil
        }
        return pid
    }
}
