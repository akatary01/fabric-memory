import XCTest
@testable import FabricCore

final class SyncTests:XCTestCase {
    func testPairingRejectsNonLocalAndUnencryptedURLs() throws {
        let invalid="""
        {"version":1,"type":"memory-fabric-pairing","url":"http://example.com:8765","token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","certificate_der":"invalid"}
        """
        let value=try JSONDecoder().decode(Pairing.self,from:Data(invalid.utf8))
        XCTAssertThrowsError(try value.validate())
    }
    func testRealMacServiceTLSAndSwiftEventExchange() async throws {
        guard let configPath=ProcessInfo.processInfo.environment["FABRIC_SYNC_TEST_PAIRING"],let assets=ProcessInfo.processInfo.environment["FABRIC_TEST_ASSETS"] else {
            throw XCTSkip("Set the real local service pairing path and assets path for network integration")
        }
        let object=try JSONSerialization.jsonObject(with:Data(contentsOf:URL(fileURLWithPath:configPath))) as! [String:Any]
        let pair=try JSONDecoder().decode(Pairing.self,from:JSONSerialization.data(withJSONObject:object))
        try pair.validate()
        let client=try SyncClient(pairing:pair)
        let folder=FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer{try? FileManager.default.removeItem(at:folder)}
        let source=URL(fileURLWithPath:assets)
        let manifest=try JSONDecoder().decode(EncoderManifest.self,from:Data(contentsOf:source.appendingPathComponent("phone/encoder.json")))
        let ledger=try Ledger(path:folder.appendingPathComponent("memory.sqlite"),space:manifest.space,dimension:manifest.dimension)
        try ledger.put(memory:"swift-transport-check",text:"",embedding:nil)
        let adapter=try Adapter(folder:source.appendingPathComponent("adapters/mac-to-phone"))
        let first=try await client.sync(ledger:ledger,adapter:adapter)
        XCTAssertEqual(first.uploaded,1);XCTAssertGreaterThan(first.downloaded,0)
        let second=try await client.sync(ledger:ledger,adapter:adapter)
        XCTAssertEqual(second.uploaded,0);XCTAssertEqual(second.downloaded,0)
    }
}
