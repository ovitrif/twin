import Foundation

/// Splits a pipe's chunks into whole lines; a chunk can end inside a line
public struct LineBuffer {
    private var pending = Data()

    public init() {}

    public mutating func append(_ chunk: Data) -> [String] {
        pending.append(chunk)
        var lines: [String] = []
        while let end = pending.firstIndex(of: UInt8(ascii: "\n")) {
            let line = String(decoding: pending[pending.startIndex..<end], as: UTF8.self)
            pending.removeSubrange(pending.startIndex...end)
            if !line.isEmpty { lines.append(line) }
        }
        return lines
    }
}
