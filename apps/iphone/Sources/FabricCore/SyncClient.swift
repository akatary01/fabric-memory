import Foundation
import Security
import Darwin

public struct Pairing: Codable {
    public let version:Int
    public let type:String
    public let url:String
    public let token:String
    public let certificate_der:String
    public let ca_der:String?
    public func validate() throws {
        guard let u=URLComponents(string:url),let host=u.host else { throw FabricError("Invalid Mac address") }
        try require(version==1 && type=="memory-fabric-pairing" && u.scheme=="https" && u.user==nil && u.password==nil && u.query==nil && (u.path.isEmpty || u.path=="/"),"Invalid pairing file")
        let numericHost=host.trimmingCharacters(in:CharacterSet(charactersIn:"[]"))
        var ipv6Address=in6_addr()
        let ipv6=numericHost.withCString{inet_pton(AF_INET6,$0,&ipv6Address)}==1
        let ipv6Bytes=withUnsafeBytes(of:ipv6Address){Array($0)}
        let unicastIPv6=ipv6 && ipv6Bytes[0] != 0xff && ipv6Bytes.contains(where:{$0 != 0})
        let segments=host.split(separator:".",omittingEmptySubsequences:false)
        let parts=segments.compactMap{Int($0)}
        let ipv4=segments.count==4 && parts.count==4 && parts.allSatisfy{(0...255).contains($0)}
        let privateIP=ipv4 && (parts[0]==10 || parts==[127,0,0,1] || (parts[0]==192 && parts[1]==168) || (parts[0]==172 && (16...31).contains(parts[1])))
        // IPv6-only Personal Hotspots use globally scoped LAN addresses. The
        // paired leaf certificate and root still authenticate this exact Mac.
        let privateHost=host.hasSuffix(".local") || host=="localhost" || privateIP || unicastIPv6
        try require(privateHost && token.count>=32 && token.count<=256,"Pairing requires a local Mac address")
        guard let data=Data(base64Encoded:certificate_der) else { throw FabricError("Invalid pairing certificate") }
        try require(SecCertificateCreateWithData(nil,data as CFData) != nil,"Invalid pairing certificate")
    }
    public static func load() throws -> Pairing? {
        let query:[String:Any]=[kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:"local.memoryfabric.pairing",
            kSecAttrAccount as String:"mac",kSecReturnData as String:true,kSecMatchLimit as String:kSecMatchLimitOne]
        var result:CFTypeRef?;let code=SecItemCopyMatching(query as CFDictionary,&result)
        if code==errSecItemNotFound { return nil }
        try require(code==errSecSuccess,"Could not read Mac pairing from Keychain")
        let p=try JSONDecoder().decode(Pairing.self,from:result as! Data);try p.validate();return p
    }
    public func save() throws {
        try validate()
        let query:[String:Any]=[kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:"local.memoryfabric.pairing",kSecAttrAccount as String:"mac"]
        let value:[String:Any]=[kSecValueData as String:try JSONEncoder().encode(self),kSecAttrAccessible as String:kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
        let updated=SecItemUpdate(query as CFDictionary,value as CFDictionary)
        if updated==errSecItemNotFound { try require(SecItemAdd(query.merging(value){_,new in new} as CFDictionary,nil)==errSecSuccess,"Could not save Mac pairing") }
        else { try require(updated==errSecSuccess,"Could not update Mac pairing") }
    }
}

public final class SyncClient:NSObject,URLSessionDelegate,URLSessionTaskDelegate,@unchecked Sendable {
    private let pairing:Pairing
    private lazy var session:URLSession={
        let config=URLSessionConfiguration.ephemeral;config.timeoutIntervalForRequest=110;config.timeoutIntervalForResource=150
        return URLSession(configuration:config,delegate:self,delegateQueue:nil)
    }()
    public init(pairing:Pairing) throws { try pairing.validate();self.pairing=pairing;super.init() }
    public func urlSession(_ session:URLSession,didReceive challenge:URLAuthenticationChallenge,completionHandler:@escaping(URLSession.AuthChallengeDisposition,URLCredential?)->Void) {
        guard challenge.protectionSpace.authenticationMethod==NSURLAuthenticationMethodServerTrust,
              let trust=challenge.protectionSpace.serverTrust,let data=Data(base64Encoded:pairing.certificate_der),
              let anchorData=Data(base64Encoded:pairing.ca_der ?? pairing.certificate_der),
              let cert=SecCertificateCreateWithData(nil,anchorData as CFData) else { completionHandler(.cancelAuthenticationChallenge,nil);return }
        // Trust only the explicitly paired certificate; normal TLS hostname/date checks remain.
        guard let chain=SecTrustCopyCertificateChain(trust) as? [SecCertificate],let leaf=chain.first,
              SecCertificateCopyData(leaf) as Data == data,
              SecTrustSetAnchorCertificates(trust,[cert] as CFArray)==errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust,true)==errSecSuccess,
              SecTrustEvaluateWithError(trust,nil) else { completionHandler(.cancelAuthenticationChallenge,nil);return }
        completionHandler(.useCredential,URLCredential(trust:trust))
    }
    public func urlSession(_ session:URLSession,task:URLSessionTask,willPerformHTTPRedirection response:HTTPURLResponse,newRequest request:URLRequest,completionHandler:@escaping(URLRequest?)->Void) { completionHandler(nil) }
    private func request<T:Decodable>(_ path:String,body:Data?=nil,query:[URLQueryItem]=[]) async throws -> T {
        var components=URLComponents(url:URL(string:pairing.url)!.appendingPathComponent(path),resolvingAgainstBaseURL:false)!
        if !query.isEmpty { components.queryItems=query }
        var request=URLRequest(url:components.url!);request.httpMethod=body == nil ? "GET":"POST";request.httpBody=body
        request.setValue("Bearer "+pairing.token,forHTTPHeaderField:"Authorization");request.setValue("application/json",forHTTPHeaderField:"Content-Type")
        let data:Data;let response:URLResponse
        do { (data,response)=try await session.data(for:request) }
        catch let error as URLError {
            let address=pairing.url
            switch error.code {
            case .cannotFindHost:
                throw FabricError("Cannot resolve the Mac address \(address). Generate a new Pair iPhone file on the Mac and import it again; IPv6 hotspots need build 3 or later.")
            case .cannotConnectToHost,.timedOut:
                throw FabricError("Cannot connect to \(address). Keep the Mac awake, allow Local Network access, and use a private Wi-Fi network or Personal Hotspot. Generate a new pairing file after changing networks.")
            case .serverCertificateUntrusted,.secureConnectionFailed:
                throw FabricError("The Mac certificate could not be verified. Import the latest Pair iPhone file after changing networks.")
            default:throw error
            }
        }
        try require(data.count<=32*1024*1024,"Mac response exceeds transfer limit")
        guard let http=response as? HTTPURLResponse else { throw FabricError("Invalid response from Mac") }
        if !(200..<300).contains(http.statusCode) {
            let value=try? JSONSerialization.jsonObject(with:data) as? [String:Any]
            throw FabricError(value?["error"] as? String ?? "Mac rejected the request")
        }
        return try JSONDecoder().decode(T.self,from:data)
    }
    private struct Push:Codable { let events:[Event] }
    private struct Ack:Decodable { let commit:String;let inserted:Int }
    private struct Page:Decodable { let commit:String;let events:[Event];let next_after:String? }
    public struct SyncResult:Codable { public let uploaded:Int;public let downloaded:Int;public let commit:String }
    public func sync(ledger:Ledger,adapter:Adapter) async throws -> SyncResult {
        let events=try ledger.events();var uploaded=0;var downloaded=0
        for start in stride(from:0,to:events.count,by:100) {
            let ack:Ack=try await request("v1/events",body:JSONEncoder().encode(Push(events:Array(events[start..<min(start+100,events.count)]))))
            uploaded+=ack.inserted
        }
        var commit:String?;var after:String?
        repeat {
            var query:[URLQueryItem]=[]
            if let commit { query.append(URLQueryItem(name:"commit",value:commit)) }
            if let after { query.append(URLQueryItem(name:"after",value:after)) }
            let page:Page=try await request("v1/events",query:query)
            downloaded+=try ledger.ingest(page.events,adapter:adapter);commit=page.commit;after=page.next_after
        } while after != nil
        return SyncResult(uploaded:uploaded,downloaded:downloaded,commit:commit ?? "")
    }
    public struct Turn:Codable { public let role:String;public let content:String;public init(role:String,content:String){self.role=role;self.content=content} }
    public struct Answer:Codable { public let text:String;public let model:String }
    private struct Prompt:Codable { let question:String;let context:String;let history:[Turn] }
    public func prompt(question:String,context:String,history:[Turn]) async throws -> Answer {
        try await request("v1/prompt",body:JSONEncoder().encode(Prompt(question:question,context:context,history:Array(history.suffix(12)))))
    }
}
