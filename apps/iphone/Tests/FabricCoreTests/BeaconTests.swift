import XCTest

@testable import FabricCore

final class BeaconTests: XCTestCase {
  func testCloudPrefixAndInvitationQR() throws {
    let compact=try BeaconInvitation.parse("https://akatary.com/fabric-git#join=abcdefghijklmnopqrstuvwx")
    XCTAssertEqual(compact.url,"https://akatary.com/fabric-git");XCTAssertEqual(compact.code,"abcdefghijklmnopqrstuvwx")
    let invitation = try BeaconInvitation.parse(
      "fabric-memory://pair?url=https%3A%2F%2Fakatary.com%2Ffabric-git#invite=abcdefghijklmnopqrstuvwx"
    )
    XCTAssertEqual(invitation.url, "https://akatary.com/fabric-git")
    XCTAssertEqual(invitation.code, "abcdefghijklmnopqrstuvwx")
    let data = Data(
      "{\"version\":2,\"type\":\"memory-fabric-pairing\",\"url\":\"https://akatary.com/fabric-git\",\"token\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}"
        .utf8)
    try JSONDecoder().decode(Pairing.self, from: data).validate()
  }
  func testRejectsUnsafeAddressesAndAmbiguousQR() {
    for value in [
      "http://host", "https://user:password@host", "https://host/?token=x", "https://host/#x",
      "https://host/%2fprivate", "fabric-memory://pair?url=https%3A%2F%2Fa&url=https%3A%2F%2Fb",
    ] { XCTAssertThrowsError(try BeaconInvitation.parse(value), value) }
  }

  func testLocalCertificatePairingAndPublicHostRejection() throws {
    let pin="MIIBgzCCASqgAwIBAgIUJSNOeHKEgGaeMlAJB3uPXNQ_--gwCgYIKoZIzj0EAwIwHzEdMBsGA1UEAwwUTWVtb3J5IEZhYnJpYyBiZWFjb24wIBcNNzUwMTAxMDAwMDAwWhgPNDA5NjAxMDEwMDAwMDBaMB8xHTAbBgNVBAMMFE1lbW9yeSBGYWJyaWMgYmVhY29uMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEmRMS3mYnskFLgPyAl8bF02_59De6orZjwru4CKEG-xFAZ4zw7pfIFKHw1Y4YSuZHIwp_ZTuF98v_dUYmwUDk66NCMEAwDgYDVR0PAQH_BAQDAgEGMB0GA1UdDgQWBBRYbs1rDY0Cu5g-FBM2fVjEPcw44DAPBgNVHRMBAf8EBTADAQH_MAoGCCqGSM49BAMCA0cAMEQCIDZ7KM1XXqHlhdhlhK8LLv0f3mvmHvvGGFD6UqL6HKQVAiBfR6eOwQUAZ1pfjyR4MrlzhD3j33OS16W1FPmxBAPxGQ"
    let link="fabric-memory://pair?url=https%3A%2F%2F192.168.1.20%3A8766&certificate="+pin+"#invite=abcdefghijklmnopqrstuvwx"
    let invitation=try BeaconInvitation.parse(link)
    XCTAssertEqual(invitation.url,"https://192.168.1.20:8766")
    XCTAssertNotNil(invitation.certificate)
    try Pairing(version:2,type:"memory-fabric-pairing",url:invitation.url,token:String(repeating:"a",count:32),certificate_der:invitation.certificate,ca_der:invitation.certificate).validate()
    XCTAssertThrowsError(try BeaconInvitation.parse(link.replacingOccurrences(of:"192.168.1.20",with:"example.org")))
    XCTAssertThrowsError(try Pairing(version:2,type:"memory-fabric-pairing",url:"https://example.org",token:String(repeating:"a",count:32),certificate_der:invitation.certificate,ca_der:invitation.certificate).validate())
  }
}
