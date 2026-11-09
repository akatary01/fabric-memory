import XCTest
@testable import FabricCore

final class EncoderTests: XCTestCase {
    func assets() throws -> URL {
        guard let path=ProcessInfo.processInfo.environment["FABRIC_TEST_ASSETS"] else {
            throw XCTSkip("Set FABRIC_TEST_ASSETS to run real encoder/adapter parity checks")
        }
        return URL(fileURLWithPath:path)
    }
    func testRealMiniLMEncoderAndWordPieceMatchTrainingPipeline() throws {
        let root=try assets()
        let job=try JSONSerialization.jsonObject(with:Data(contentsOf:root.appendingPathComponent("job.json"))) as! [String:Any]
        let encoder=try Encoder(folder:root.appendingPathComponent("phone"))
        let docs=job["documents"] as! [String]; let queries=job["queries"] as! [String]
        let cases=job["tokenizer_cases"] as! [[String:Any]]
        let reference=job["reference_phone_vectors"] as! [[NSNumber]]
        for (i,test) in cases.enumerated() {
            let text=test["text"] as! String
            XCTAssertEqual(encoder.tokenizer.encode(text),(test["ids"] as! [NSNumber]).map { $0.int64Value })
            let actual=try encoder.encode(text)
            let expected=reference[docs.count+queries.count+i].map { $0.floatValue }
            XCTAssertLessThan(zip(actual.values,expected).map { abs($0-$1) }.max()!,0.0001)
        }
        for i in docs.indices {
            let actual=try encoder.encode(docs[i]); let expected=reference[i].map { $0.floatValue }
            XCTAssertLessThan(zip(actual.values,expected).map { abs($0-$1) }.max()!,0.0001)
        }
        for i in queries.indices {
            let actual=try encoder.encode(queries[i]); let expected=reference[docs.count+i].map { $0.floatValue }
            XCTAssertLessThan(zip(actual.values,expected).map { abs($0-$1) }.max()!,0.0001)
        }
    }
    func testRealAdapterMatchesGoldenVector() throws {
        let root=try assets()
        let all=try JSONSerialization.jsonObject(with:Data(contentsOf:root.appendingPathComponent("golden.json"))) as! [String:Any]
        for direction in ["phone-to-mac","mac-to-phone"] {
            let example=all[direction] as! [String:Any]
            let data=try JSONSerialization.data(withJSONObject:example["input"]!)
            let input=try JSONDecoder().decode(Embedding.self,from:data)
            let expected=try JSONDecoder().decode(Embedding.self,from:JSONSerialization.data(withJSONObject:example["expected"]!))
            let adapter=try Adapter(folder:root.appendingPathComponent("adapters/"+direction))
            let actual=try adapter.apply(input)
            XCTAssertEqual(actual.space,expected.space)
            XCTAssertLessThan(zip(actual.values,expected.values).map { abs($0-$1) }.max()!,0.00001)
        }
    }
}
