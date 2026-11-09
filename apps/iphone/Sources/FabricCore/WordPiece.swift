import Foundation

/// BERT uncased WordPiece for the pinned MiniLM tokenizer. MPNet is never loaded on phone.
public final class WordPiece {
    private let vocab: [String: Int]
    private let special: [String]
    private let limit: Int
    public init(url: URL, limit: Int = 256) throws {
        let root = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        guard let model = root?["model"] as? [String: Any], let vocab = model["vocab"] as? [String: Int],
              model["type"] as? String == "WordPiece" else { throw FabricError("unsupported tokenizer") }
        self.vocab = vocab; self.limit = limit
        special = ((root?["added_tokens"] as? [[String: Any]]) ?? []).compactMap { $0["content"] as? String }.sorted { $0.count > $1.count }
        for token in ["[CLS]", "[SEP]", "[UNK]"] { try require(vocab[token] != nil, "missing tokenizer special token") }
    }
    private func chinese(_ v: UInt32) -> Bool {
        (0x4E00...0x9FFF).contains(v) || (0x3400...0x4DBF).contains(v) || (0x20000...0x2A6DF).contains(v) ||
        (0x2A700...0x2B73F).contains(v) || (0x2B740...0x2B81F).contains(v) || (0x2B820...0x2CEAF).contains(v) ||
        (0xF900...0xFAFF).contains(v) || (0x2F800...0x2FA1F).contains(v)
    }
    private func basic(_ text: String) -> [String] {
        var cleaned = ""
        for c in text.unicodeScalars {
            if c.value == 0 || c.value == 0xFFFD { continue }
            if CharacterSet.whitespacesAndNewlines.contains(c) { cleaned.append(" "); continue }
            let category = c.properties.generalCategory
            if category == .control || category == .format { continue }
            if chinese(c.value) { cleaned += " \(c) " } else { cleaned.unicodeScalars.append(c) }
        }
        let lowered = cleaned.lowercased().decomposedStringWithCanonicalMapping
        var tokens: [String] = []; var current = ""
        func flush() { if !current.isEmpty { tokens.append(current); current = "" } }
        for c in lowered.unicodeScalars {
            if c.properties.generalCategory == .nonspacingMark { continue }
            if CharacterSet.whitespacesAndNewlines.contains(c) { flush(); continue }
            let v = c.value
            if (33...47).contains(v) || (58...64).contains(v) || (91...96).contains(v) || (123...126).contains(v) || CharacterSet.punctuationCharacters.contains(c) {
                flush(); tokens.append(String(c))
            } else { current.unicodeScalars.append(c) }
        }
        flush(); return tokens
    }
    private func pieces(_ token: String) -> [Int64] {
        let chars = Array(token.unicodeScalars)
        if chars.count > 100 { return [Int64(vocab["[UNK]"]!)] }
        var result: [Int64] = []; var start = 0
        while start < chars.count {
            var end = chars.count; var found: Int?
            while end > start {
                let piece = (start == 0 ? "" : "##") + String(String.UnicodeScalarView(chars[start..<end]))
                if let id = vocab[piece] { found = id; break }; end -= 1
            }
            guard let id = found else { return [Int64(vocab["[UNK]"]!)] }
            result.append(Int64(id)); start = end
        }
        return result
    }
    public func encode(_ text: String) -> [Int64] {
        // Added special tokens are recognized before lowercasing and punctuation splitting.
        var remaining = text[...]; var tokens: [String] = []
        while !remaining.isEmpty {
            let next = special.compactMap { t -> (Range<String.Index>, String)? in
                remaining.range(of: t).map { ($0, t) }
            }.min { $0.0.lowerBound < $1.0.lowerBound }
            guard let (range, token) = next else { tokens += basic(String(remaining)); break }
            tokens += basic(String(remaining[..<range.lowerBound])); tokens.append(token)
            remaining = remaining[range.upperBound...]
        }
        var ids: [Int64] = [Int64(vocab["[CLS]"]!)]
        for token in tokens {
            let values = special.contains(token) ? [Int64(vocab[token] ?? vocab["[UNK]"]!)] : pieces(token)
            ids += values
            if ids.count >= limit-1 { ids = Array(ids.prefix(limit-1)); break }
        }
        ids.append(Int64(vocab["[SEP]"]!)); return ids
    }
}
