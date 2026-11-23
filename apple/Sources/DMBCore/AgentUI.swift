import Foundation

/// The "Agents read the iPhone" setting in Twin's `config.toml`
public enum AgentUI {
    /// On unless a top-level `agent_ui` key says `false`; comments, CRLF
    /// line ends and spacing as TOML allows them
    public static func allowed(config text: String) -> Bool {
        for raw in text.split(whereSeparator: \.isNewline) {
            var line = Substring(raw)
            if let hash = line.firstIndex(of: "#") { line = line[..<hash] }
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            // Keys after a [table] header are not the setting
            if trimmed.hasPrefix("[") { return true }
            let parts = trimmed.split(separator: "=", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
            if parts.count == 2, parts[0] == "agent_ui" { return parts[1] != "false" }
        }
        return true
    }
}
