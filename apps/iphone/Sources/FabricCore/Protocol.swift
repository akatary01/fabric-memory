import Foundation
import CryptoKit

public struct FabricError: Error, CustomStringConvertible {
    public let description: String
    public init(_ message: String) { description = message }
}
public func require(_ condition: Bool, _ message: String) throws {
    if !condition { throw FabricError(message) }
}
public func digest(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }
public func normalize(_ values: [Float]) throws -> [Float] {
    try require(!values.isEmpty && values.allSatisfy { $0.isFinite }, "invalid vector")
    let norm = sqrt(values.reduce(0.0) { $0 + Double($1)*Double($1) })
    try require(norm.isFinite && norm > 1e-12, "zero vector")
    return values.map { Float(Double($0)/norm) }
}
public struct Embedding: Codable, Equatable {
    public let space: String
    public let values: [Float]
    public init(space: String, values: [Float]) { self.space = space; self.values = values }
}
public struct MemoryAccess: Codable, Equatable {
    public let scope:String
    public let allowed_devices:[String]
    public let reason:String
    public init(scope:String,allowed_devices:[String]=[],reason:String=""){self.scope=scope;self.allowed_devices=allowed_devices;self.reason=reason}
    public var syncable:Bool{scope=="public"}
    public func validate() throws {try require(["private","public","restricted"].contains(scope) && reason.utf8.count<=1000 && allowed_devices.count<=64,"Invalid memory access");try require(allowed_devices.allSatisfy{$0.count==32 && $0.utf8.allSatisfy{(48...57).contains($0)||(97...102).contains($0)}},"Invalid approved device");try require(scope=="restricted" || allowed_devices.isEmpty,"Only restricted memories have approved devices")}
}
public struct Event: Codable, Equatable {
    public let version: Int
    public let id: String
    public let actor: String
    public let clock: UInt64
    public let memory: String
    public let deleted: Bool
    public let text: String
    public let embedding: Embedding?
    public let access:MemoryAccess?

    public init(version: Int = 1, id: String = Foundation.UUID().uuidString.lowercased(), actor: String, clock: UInt64,
                memory: String, deleted: Bool = false, text: String, embedding: Embedding?,access:MemoryAccess?=nil) {
        self.version = version; self.id = id; self.actor = actor; self.clock = clock
        self.memory = memory; self.deleted = deleted; self.text = text; self.embedding = embedding;self.access=access
    }
    public func validate() throws {
        try access?.validate()
        try require(version == 1, "unsupported event version")
        for value in [id, actor] {
            try require(Foundation.UUID(uuidString: value)?.uuidString.lowercased() == value, "invalid UUID")
        }
        try require(clock > 0 && clock < 9_007_199_254_740_991, "invalid clock")
        try require(!memory.isEmpty && memory.utf8.count <= 128 && memory.utf8.allSatisfy { (33...126).contains($0) }, "invalid memory ID")
        try require(text.utf8.count <= 65_536, "text exceeds limit")
        if deleted {
            try require(text.isEmpty && embedding == nil, "tombstone contains content")
        } else {
            guard let e = embedding else { throw FabricError("missing embedding") }
            try require(e.space.count == 64 && e.space.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }, "invalid space")
            try require((1...4096).contains(e.values.count), "invalid dimension")
            _ = try normalize(e.values)
        }
    }
    public func wins(over old: Event) -> Bool {
        if clock != old.clock { return clock > old.clock }
        if actor != old.actor { return actor > old.actor }
        return id > old.id
    }
}
public func readEvents(_ url: URL) throws -> [Event] {
    let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max
    try require(size <= 32*1024*1024, "transfer exceeds 32 MiB")
    let data = try Data(contentsOf: url)
    try require(data.count <= 32*1024*1024, "transfer exceeds limit")
    let lines = data.split(separator: 10).filter { !$0.isEmpty }
    try require(lines.count <= 10_000, "too many events")
    return try lines.map { line in
        let object = try JSONSerialization.jsonObject(with: Data(line)) as? [String: Any]
        let allowed: Set<String> = ["version","id","actor","clock","memory","deleted","text","embedding","access"]
        try require(object != nil && Set(object!.keys).isSubset(of: allowed), "unknown event field")
        if let access=object?["access"] as? [String:Any]{try require(Set(access.keys).isSubset(of:["scope","allowed_devices","reason"]),"Unknown access field")}
        if let e = object?["embedding"] as? [String: Any] {
            try require(Set(e.keys).isSubset(of: ["space", "values"]), "unknown embedding field")
        }
        let event = try JSONDecoder().decode(Event.self, from: Data(line))
        try event.validate()
        return event
    }
}
public func writeJSON<T: Encodable>(_ value: T, to url: URL) throws {
    let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(value).write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
}
