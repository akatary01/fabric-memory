import XCTest
@testable import FabricCore
final class MemoryRecallTests:XCTestCase {
    private func hit(_ id:String,_ text:String,_ score:Float)->Hit{Hit(memory:id,text:text,score:score,origin:"fixture",access:nil)}
    func testLowRelevanceAndGeneratedRepliesAreExcluded(){
        XCTAssertTrue(MemoryRecall.select([hit("note-low","Unrelated fact",0.49),hit("chat-x-assistant","I cannot browse",0.99),hit("chat-x-user","What is my theme?",0.99)]).isEmpty)
    }
    func testRecallDeduplicatesAndCapsSharedContext(){
        let text=String(repeating:"x",count:1000)
        let selected=MemoryRecall.select([hit("a",text,0.9),hit("b",text,0.89),hit("c",String(repeating:"y",count:1000),0.88)])
        XCTAssertEqual(selected.count,2);XCTAssertEqual(selected.map{$0.text.count}.reduce(0,+),800)
    }
}
