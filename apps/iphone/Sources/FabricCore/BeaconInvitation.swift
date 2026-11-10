import Foundation
import Security

public struct BeaconInvitation {
  public let url: String
  public let code: String
  public let certificate: String?
  public init(url:String,code:String,certificate:String?=nil){self.url=url;self.code=code;self.certificate=certificate}
  public static let defaultURL = "https://akatary.com/fabric-git"
  public static func endpoint(_ text: String) throws -> String {
    guard let c = URLComponents(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
      c.scheme == "https", let host = c.host, !host.isEmpty, c.user == nil, c.password == nil,
      c.query == nil, c.fragment == nil, !c.percentEncodedPath.contains("%"),
      !c.path.contains("\\"), !c.path.split(separator: "/").contains(".."),
      let absolute = c.url?.absoluteString
    else {
      throw FabricError("Enter a complete HTTPS beacon URL without credentials or query parameters")
    }
    return absolute.hasSuffix("/") ? String(absolute.dropLast()) : absolute
  }
  public static func parse(_ text: String, code: String = "") throws -> BeaconInvitation {
    let value = text.trimmingCharacters(in: .whitespacesAndNewlines)
    if value.hasPrefix("fabric-memory://") {
      guard let c = URLComponents(string: value), c.host == "pair", c.user == nil, c.password == nil
      else { throw FabricError("Not a Memory Fabric pairing QR") }
      let urls = (c.queryItems ?? []).filter { $0.name == "url" }
      guard urls.count == 1, let raw = urls[0].value else {
        throw FabricError("Pairing QR must contain one beacon URL")
      }
      let invite =
        c.fragment?.hasPrefix("invite=") == true ? String(c.fragment!.dropFirst(7)) : code
      let address=try endpoint(raw)
      let pins=(c.queryItems ?? []).filter{$0.name=="certificate"}
      var certificate:String?=nil
      if !pins.isEmpty {
        try require(pins.count==1,"Invalid beacon certificate")
        var encoded=(pins[0].value ?? "").replacingOccurrences(of:"-",with:"+").replacingOccurrences(of:"_",with:"/")
        encoded += String(repeating:"=",count:(4-encoded.count%4)%4)
        try require(encoded.count<=8192 && Pairing.isLocal(address),"Certificate pairing requires a local beacon")
        guard let bytes=Data(base64Encoded:encoded),SecCertificateCreateWithData(nil,bytes as CFData) != nil else{throw FabricError("Invalid beacon certificate")}
        certificate=bytes.base64EncodedString()
      }
      return BeaconInvitation(url: address, code: invite,certificate:certificate)
    }
    if var compact=URLComponents(string:value),let fragment=compact.fragment,fragment.hasPrefix("join=") {
      compact.fragment=nil
      guard let address=compact.string else{throw FabricError("Invalid beacon QR")}
      return BeaconInvitation(url:try endpoint(address),code:String(fragment.dropFirst(5)))
    }
    return BeaconInvitation(
      url: try endpoint(value), code: code.trimmingCharacters(in: .whitespacesAndNewlines))
  }
  public func join(name: String) async throws -> Pairing {
    try require(
      (20...256).contains(code.count),
      "Enter a current invitation code or scan the beacon’s join QR")
    try require(
      !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && name.count <= 80,
      "Choose a device name of 1–80 characters")
    let configuration = URLSessionConfiguration.ephemeral
    configuration.timeoutIntervalForRequest = 20
    configuration.timeoutIntervalForResource = 30
    let session = URLSession(
      configuration: configuration, delegate: NoRedirect(certificate:certificate), delegateQueue: nil)
    defer { session.finishTasksAndInvalidate() }
    var request = URLRequest(url: URL(string: url + "/v1/join")!)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONSerialization.data(withJSONObject: [
      "invitation": code, "device_name": name,
    ])
    let (data, response) = try await session.data(for: request)
    try require(data.count <= 65536, "Beacon response exceeds limit")
    guard let http = response as? HTTPURLResponse else {
      throw FabricError("Invalid beacon response")
    }
    if !(200..<300).contains(http.statusCode) {
      let error = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
      throw FabricError(error?["error"] as? String ?? "Beacon rejected invitation")
    }
    let pairing = try JSONDecoder().decode(Pairing.self, from: data)
    try pairing.validate()
    try require(pairing.ca_der==certificate,"Beacon certificate did not match its pairing QR")
    try require(
      pairing.version == 2 && pairing.url == url,
      "Beacon identity did not match the requested address")
    return pairing
  }
}
private final class NoRedirect: NSObject, URLSessionTaskDelegate {
  let certificate:String?
  init(certificate:String?){self.certificate=certificate}
  func urlSession(_ session:URLSession,didReceive challenge:URLAuthenticationChallenge,completionHandler:@escaping(URLSession.AuthChallengeDisposition,URLCredential?)->Void){
    guard let encoded=certificate else{completionHandler(.performDefaultHandling,nil);return}
    guard challenge.protectionSpace.authenticationMethod==NSURLAuthenticationMethodServerTrust,
      let trust=challenge.protectionSpace.serverTrust,let bytes=Data(base64Encoded:encoded),let anchor=SecCertificateCreateWithData(nil,bytes as CFData),
      SecTrustSetAnchorCertificates(trust,[anchor] as CFArray)==errSecSuccess,
      SecTrustSetAnchorCertificatesOnly(trust,true)==errSecSuccess,SecTrustEvaluateWithError(trust,nil)
    else{completionHandler(.cancelAuthenticationChallenge,nil);return}
    completionHandler(.useCredential,URLCredential(trust:trust))
  }
  func urlSession(
    _ session: URLSession, task: URLSessionTask,
    willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
    completionHandler: @escaping (URLRequest?) -> Void
  ) { completionHandler(nil) }
}
