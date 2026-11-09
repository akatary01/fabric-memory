import Foundation
import CSQLite

public struct Hit: Codable {
    public let memory: String
    public let text: String
    public let score: Float
    public let origin: String
}
public struct Status: Codable {
    public let actor: String
    public let events: Int
    public let live_memories: Int
    public let tombstones: Int
    public let state_digest: String
    public let event_digest: String
    public let logical_clock: UInt64
    public let projection_bytes: Int
}
public final class Ledger {
    private var db: OpaquePointer?
    private let lock = NSRecursiveLock()
    public private(set) var actor = ""
    public let space: String
    public let dimension: Int
    private let json = JSONEncoder()
    private let decoder = JSONDecoder()

    public init(path: URL, space: String, dimension: Int) throws {
        self.space = space; self.dimension = dimension
        try require(space.count == 64 && (1...4096).contains(dimension), "invalid index configuration")
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard sqlite3_open_v2(path.path, &db, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX, nil) == SQLITE_OK else {
            if let db { sqlite3_close(db) }; db = nil; throw FabricError("could not open database")
        }
        sqlite3_busy_timeout(db, 5000)
        do {
            for sql in ["PRAGMA journal_mode=WAL", "PRAGMA synchronous=FULL",
              "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
              "CREATE TABLE IF NOT EXISTS events(id TEXT PRIMARY KEY,actor TEXT NOT NULL,clock INTEGER NOT NULL,payload TEXT NOT NULL,UNIQUE(actor,clock))",
              "CREATE TABLE IF NOT EXISTS heads(memory TEXT PRIMARY KEY,event TEXT NOT NULL,vector TEXT)",
              "CREATE INDEX IF NOT EXISTS event_clock ON events(clock)"] { _ = try rows(sql) }
            _ = try rows("INSERT OR IGNORE INTO meta VALUES('actor',?)", [Foundation.UUID().uuidString.lowercased()])
            _ = try rows("INSERT OR IGNORE INTO meta VALUES('space',?)", [space])
            _ = try rows("INSERT OR IGNORE INTO meta VALUES('dimension',?)", [String(dimension)])
            actor = try rows("SELECT value FROM meta WHERE key='actor'")[0][0]!
            let stored = try rows("SELECT value FROM meta WHERE key='space'")[0][0]!
            let dim = try rows("SELECT value FROM meta WHERE key='dimension'")[0][0]!
            try require(stored == space && dim == String(dimension), "database embedding space mismatch")
        } catch { sqlite3_close(db); db = nil; throw error }
    }
    deinit { sqlite3_close(db) }
    private func rows(_ sql: String, _ bindings: [String?] = []) throws -> [[String?]] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else { throw FabricError("SQLite prepare failed") }
        defer { sqlite3_finalize(statement) }
        for (i, value) in bindings.enumerated() {
            let code: Int32
            if let value {
                code = value.withCString { sqlite3_bind_text(statement, Int32(i+1), $0, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self)) }
            } else { code = sqlite3_bind_null(statement, Int32(i+1)) }
            try require(code == SQLITE_OK, "SQLite bind failed")
        }
        var result: [[String?]] = []
        while true {
            let code = sqlite3_step(statement)
            if code == SQLITE_DONE { break }
            try require(code == SQLITE_ROW, "SQLite operation failed: \(String(cString: sqlite3_errmsg(db)))")
            result.append((0..<sqlite3_column_count(statement)).map { index in
                guard let text = sqlite3_column_text(statement, index) else { return nil }
                return String(cString: text)
            })
        }
        return result
    }
    private func string<T: Encodable>(_ value: T) throws -> String { String(decoding: try json.encode(value), as: UTF8.self) }
    private func decode<T: Decodable>(_ type: T.Type, _ value: String) throws -> T { try decoder.decode(type, from: Data(value.utf8)) }
    private func transaction<T>(_ body: () throws -> T) throws -> T {
        _ = try rows("BEGIN IMMEDIATE")
        do { let value = try body(); _ = try rows("COMMIT"); return value }
        catch { _ = try? rows("ROLLBACK"); throw error }
    }
    @discardableResult
    private func insert(_ event: Event, vector: Embedding?) throws -> Bool {
        if let existing = try rows("SELECT payload FROM events WHERE id=?", [event.id]).first?[0] {
            try require(try decode(Event.self, existing) == event, "immutable event ID collision")
            return false
        }
        let raw = try string(event)
        _ = try rows("INSERT INTO events VALUES(?,?,?,?)", [event.id, event.actor, String(event.clock), raw])
        let old = try rows("SELECT event FROM heads WHERE memory=?", [event.memory]).first?[0]
        let wins = try old.map { event.wins(over: try decode(Event.self, $0)) } ?? true
        if wins {
            let encoded = try vector.map { try string($0) }
            _ = try rows("INSERT INTO heads VALUES(?,?,?) ON CONFLICT(memory) DO UPDATE SET event=excluded.event,vector=excluded.vector", [event.memory, raw, encoded])
        }
        return true
    }
    @discardableResult
    public func put(memory: String, text: String, embedding: Embedding?) throws -> Event {
        lock.lock(); defer { lock.unlock() }
        return try transaction {
            let clock = UInt64(try rows("SELECT COALESCE(MAX(clock),0)+1 FROM events")[0][0]!)!
            let event = Event(actor: actor, clock: clock, memory: memory, deleted: embedding == nil, text: text, embedding: embedding)
            try event.validate()
            if let e = embedding { try require(e.space == space && e.values.count == dimension, "native embedding mismatch") }
            try insert(event, vector: embedding)
            return event
        }
    }
    public func ingest(_ events: [Event], adapter: Adapter?) throws -> Int {
        lock.lock(); defer { lock.unlock() }
        try require(events.count <= 10_000, "too many events")
        let vectors: [Embedding?] = try events.map { event in
            try event.validate()
            guard let e = event.embedding else { return nil }
            if e.space == space {
                try require(e.values.count == dimension, "native dimension mismatch")
                return Embedding(space: space, values: try normalize(e.values))
            }
            guard let adapter else { throw FabricError("no adapter for foreign vector") }
            try require(adapter.manifest.target == space && adapter.manifest.output_dim == dimension, "adapter target mismatch")
            return try adapter.apply(e)
        }
        return try transaction {
            var count = 0
            if events.contains(where: { $0.embedding != nil && $0.embedding!.space != space }) {
                let adapter = adapter! // Foreign vectors were validated above.
                let key = "adapter:"+adapter.manifest.source
                _ = try rows("INSERT OR IGNORE INTO meta VALUES(?,?)",[key,adapter.id])
                let existing = try rows("SELECT value FROM meta WHERE key=?",[key])[0][0]!
                try require(existing == adapter.id,"adapter changed; rebuild foreign projection explicitly")
            }
            for (event, vector) in zip(events, vectors) { if try insert(event, vector: vector) { count += 1 } }
            return count
        }
    }
    public func events() throws -> [Event] {
        lock.lock(); defer { lock.unlock() }
        return try rows("SELECT payload FROM events ORDER BY id").map { try decode(Event.self, $0[0]!) }
    }
    public func export(to url: URL) throws {
        let lines = try events().map { try json.encode($0) }
        var data = Data()
        for line in lines { data.append(line); data.append(10) }
        try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
    public func search(_ query: Embedding, k: Int = 5) throws -> [Hit] {
        lock.lock(); defer { lock.unlock() }
        try require(query.space == space && query.values.count == dimension && k > 0, "query mismatch")
        let q = try normalize(query.values)
        var hits: [Hit] = []
        for row in try rows("SELECT event,vector FROM heads WHERE vector IS NOT NULL ORDER BY memory") {
            let event = try decode(Event.self, row[0]!)
            let vector = try normalize(decode(Embedding.self, row[1]!).values)
            let score = zip(q, vector).reduce(Float(0)) { $0 + $1.0*$1.1 }
            hits.append(Hit(memory: event.memory, text: event.text, score: score, origin: event.actor))
        }
        hits.sort { $0.score == $1.score ? $0.memory < $1.memory : $0.score > $1.score }
        return Array(hits.prefix(min(k,100)))
    }
    public func status() throws -> Status {
        lock.lock(); defer { lock.unlock() }
        var state = ""; var live = 0; var dead = 0; var bytes = 0
        for row in try rows("SELECT event,vector FROM heads ORDER BY memory") {
            let event = try decode(Event.self, row[0]!)
            state += event.memory + "\0" + event.id + "\n"
            if event.deleted { dead += 1 } else { live += 1 }
            bytes += row[0]!.utf8.count + (row[1]?.utf8.count ?? 0)
        }
        let all = try events()
        return Status(actor: actor, events: all.count, live_memories: live, tombstones: dead,
            state_digest: digest(Data(state.utf8)), event_digest: digest(Data(all.map { $0.id+"\n" }.joined().utf8)),
            logical_clock: all.map { $0.clock }.max() ?? 0, projection_bytes: bytes)
    }
}
