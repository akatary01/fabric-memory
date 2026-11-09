import XCTest
@testable import FabricCore

final class ConvergenceTests: XCTestCase {
    let space = String(repeating:"a",count:64)
    func folder() throws -> URL {
        let url=FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at:url,withIntermediateDirectories:true)
        addTeardownBlock { try? FileManager.default.removeItem(at:url) }; return url
    }
    func fixtures() -> [Event] {
        let a="00000000-0000-4000-8000-000000000001", b="00000000-0000-4000-8000-000000000002"
        return [
            Event(id:"00000000-0000-4000-8000-000000000011",actor:a,clock:1,memory:"shared",text:"one",embedding:Embedding(space:space,values:[1,0,0])),
            Event(id:"00000000-0000-4000-8000-000000000012",actor:b,clock:1,memory:"shared",text:"two",embedding:Embedding(space:space,values:[0,1,0])),
            Event(id:"00000000-0000-4000-8000-000000000013",actor:a,clock:2,memory:"shared",deleted:true,text:"",embedding:nil)]
    }
    func testReplayOrderAndTombstoneConverge() throws {
        let root=try folder(); let events=fixtures(); var digests=Set<String>()
        for order in [[0,1,2],[0,2,1],[1,0,2],[1,2,0],[2,0,1],[2,1,0]] {
            let db=try Ledger(path:root.appendingPathComponent(UUID().uuidString),space:space,dimension:3)
            for i in order { XCTAssertEqual(try db.ingest([events[i]],adapter:nil),1) }
            XCTAssertEqual(try db.ingest(events,adapter:nil),0)
            let status=try db.status(); XCTAssertEqual(status.live_memories,0); XCTAssertEqual(status.tombstones,1)
            digests.insert(status.state_digest)
        }
        XCTAssertEqual(digests.count,1)
    }
    func testCollisionRollsBackBatch() throws {
        let db=try Ledger(path:folder().appendingPathComponent("db"),space:space,dimension:3)
        let events=fixtures(); _=try db.ingest([events[0]],adapter:nil)
        let collision=Event(id:events[0].id,actor:events[0].actor,clock:1,memory:"shared",text:"tampered",embedding:events[0].embedding)
        XCTAssertThrowsError(try db.ingest([events[1],collision],adapter:nil))
        XCTAssertEqual(try db.status().events,1)
    }
    func testForeignVectorsAreRejected() throws {
        let db=try Ledger(path:folder().appendingPathComponent("db"),space:space,dimension:3)
        let e=Event(actor:UUID().uuidString.lowercased(),clock:1,memory:"x",text:"x",embedding:Embedding(space:String(repeating:"b",count:64),values:[1,0,0]))
        XCTAssertThrowsError(try db.ingest([e],adapter:nil)); XCTAssertEqual(try db.status().events,0)
    }
}
