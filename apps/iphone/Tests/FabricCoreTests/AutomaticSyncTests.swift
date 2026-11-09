import XCTest
@testable import FabricCore

private actor SyncProbe {
    var calls=0
    var active=0
    var maximum=0
    var release:CheckedContinuation<Void,Never>?
    func run(first:XCTestExpectation,second:XCTestExpectation) async throws {
        calls += 1;active += 1;maximum=max(maximum,active)
        defer { active -= 1 }
        if calls == 1 {
            await withCheckedContinuation { release=$0;first.fulfill() }
            throw FabricError("offline")
        }
        if calls == 2 { second.fulfill() }
    }
    func resume(){release?.resume();release=nil}
    func counts()->(Int,Int){(calls,maximum)}
}

final class AutomaticSyncTests:XCTestCase {
    func testWritesDuringTransferCoalesceAndRetryWithoutOverlap() async {
        let first=expectation(description:"first transfer")
        let second=expectation(description:"follow-up transfer")
        let probe=SyncProbe()
        let sync=AutomaticSync { try await probe.run(first:first,second:second) }
        await sync.start()
        await fulfillment(of:[first],timeout:2)
        for _ in 0..<20 { await sync.request() }
        let during=await probe.counts();XCTAssertEqual(during.0,1)
        await probe.resume()
        await fulfillment(of:[second],timeout:2)
        await sync.waitUntilIdle()
        let after=await probe.counts();XCTAssertEqual(after.0,2);XCTAssertEqual(after.1,1)
        let status=await sync.status();XCTAssertEqual(status,"Auto sync up to date.")
        await sync.stop();await sync.request();await sync.waitUntilIdle()
        let stopped=await probe.counts();XCTAssertEqual(stopped.0,2)
    }
    func testPollingRetriesFailureAndResumeTriggersImmediateTransfer() async {
        let first=expectation(description:"first transfer")
        let second=expectation(description:"automatic retry")
        let probe=SyncProbe()
        let sync=AutomaticSync(interval:20_000_000) { try await probe.run(first:first,second:second) }
        await sync.start();await fulfillment(of:[first],timeout:2)
        await probe.resume()
        await fulfillment(of:[second],timeout:2)
        await sync.stop();await sync.waitUntilIdle()
        let before=await probe.counts()
        await sync.start();await sync.waitUntilIdle();await sync.stop()
        let after=await probe.counts();XCTAssertGreaterThan(after.0,before.0);XCTAssertEqual(after.1,1)
    }
}
