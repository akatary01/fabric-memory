import XCTest
@testable import FabricCore

final class LocalGenerationTests:XCTestCase {
    func testPhoneSessionAnswersUsingBundledModels() async throws {
        guard let path=ProcessInfo.processInfo.environment["FABRIC_PHONE_ASSETS"] else {
            throw XCTSkip("Set FABRIC_PHONE_ASSETS to the real bundled iPhone assets")
        }
        let storage=FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer{try? FileManager.default.removeItem(at:storage)}
        let session=try PhoneSession(assets:URL(fileURLWithPath:path),storage:storage)
        let answer=try await session.send("Reply with just the number: what is 2 + 2?")
        XCTAssertTrue(answer.text.contains("4"),answer.text)
        XCTAssertTrue(answer.model.contains("on-device"))
        let history=await session.conversation();XCTAssertEqual(history.count,2)
        await session.releaseGenerator()
    }
    func testRealNativeLFM3AnswersFromLocalContext() throws {
        guard let path=ProcessInfo.processInfo.environment["FABRIC_GENERATION_ASSETS"] else {
            throw XCTSkip("Set FABRIC_GENERATION_ASSETS to the staged real LFM3 model folder")
        }
        let generator=try LocalGenerator(folder:URL(fileURLWithPath:path))
        let answer=try generator.answer(question:"What is the demonstration launch phrase? Reply with only the phrase.",
            context:"The demonstration launch phrase is silver acorn.",history:[])
        XCTAssertTrue(answer.text.lowercased().contains("silver acorn"),answer.text)
        XCTAssertTrue(answer.model.contains("LFM3"))
    }
}
