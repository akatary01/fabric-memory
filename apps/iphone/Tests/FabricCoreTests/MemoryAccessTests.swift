import XCTest
@testable import FabricCore
final class MemoryAccessTests:XCTestCase{
 func testAccessRoundTripAndPrivateTombstone()throws{
  let folder=FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString);defer{try? FileManager.default.removeItem(at:folder)}
  let space=String(repeating:"a",count:64);let ledger=try Ledger(path:folder.appendingPathComponent("memory.sqlite"),space:space,dimension:2)
  let policy=MemoryAccess(scope:"private",reason:"Sensitive note");let event=try ledger.put(memory:"private-note",text:"Test only",embedding:Embedding(space:space,values:[1,0]),access:policy)
  let decoded=try JSONDecoder().decode(Event.self,from:JSONEncoder().encode(event));XCTAssertEqual(decoded.access,policy);XCTAssertFalse(policy.syncable)
  _=try ledger.put(memory:"private-note",text:"",embedding:nil);XCTAssertTrue(try ledger.events().allSatisfy{$0.access?.scope=="private"})
 }
 func testLegacyRestrictedDoesNotSync()throws{XCTAssertFalse(MemoryAccess(scope:"restricted").syncable);let access=MemoryAccess(scope:"restricted",allowed_devices:[String(repeating:"a",count:32)]);try access.validate();XCTAssertFalse(access.syncable);XCTAssertThrowsError(try MemoryAccess(scope:"public",allowed_devices:[String(repeating:"a",count:32)]).validate())}
}
