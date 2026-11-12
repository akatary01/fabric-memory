import Foundation
import Security
import Darwin

private func sampleDeviceUsage(at path: String) -> [String: Any] {
    let host = mach_host_self()
    defer { mach_port_deallocate(mach_task_self_, host) }
    func cpuTicks() -> [UInt32]? {
        var load = host_cpu_load_info()
        var count = mach_msg_type_number_t(MemoryLayout<host_cpu_load_info>.size / MemoryLayout<integer_t>.size)
        let status = withUnsafeMutablePointer(to: &load) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                host_statistics(host, HOST_CPU_LOAD_INFO, $0, &count)
            }
        }
        guard status == KERN_SUCCESS else { return nil }
        return [load.cpu_ticks.0, load.cpu_ticks.1, load.cpu_ticks.2, load.cpu_ticks.3]
    }
    var sample: [String: Any] = [:]
    if let start = cpuTicks() {
        Thread.sleep(forTimeInterval: 0.2)
        if let end = cpuTicks() {
            let delta = zip(start, end).map { UInt64($1 &- $0) }
            let total = delta.reduce(0, +)
            if total > 0 {
                sample["cpu_percent"] = 100 * Double(total - delta[Int(CPU_STATE_IDLE)]) / Double(total)
            }
        }
    }
    var vm = vm_statistics64()
    var vmCount = mach_msg_type_number_t(MemoryLayout<vm_statistics64>.size / MemoryLayout<integer_t>.size)
    let vmStatus = withUnsafeMutablePointer(to: &vm) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(vmCount)) {
            host_statistics64(host, HOST_VM_INFO64, $0, &vmCount)
        }
    }
    if vmStatus == KERN_SUCCESS {
        let total = ProcessInfo.processInfo.physicalMemory
        let pages = UInt64(vm.active_count) + UInt64(vm.wire_count) + UInt64(vm.compressor_page_count)
        sample["memory_total"] = total
        sample["memory_used"] = min(total, pages * UInt64(getpagesize()))
    }
    if let disk = try? FileManager.default.attributesOfFileSystem(forPath: path),
       let size = disk[.systemSize] as? NSNumber,
       let free = disk[.systemFreeSize] as? NSNumber {
        sample["disk_total"] = size.uint64Value
        sample["disk_used"] = size.uint64Value - min(size.uint64Value, free.uint64Value)
    }
    return sample
}

public struct Pairing: Codable {
    public let version:Int
    public let type:String
    public let url:String
    public let token:String
    public let device_id:String?
    public let certificate_der:String?
    public let ca_der:String?
    public init(version:Int,type:String,url:String,token:String,certificate_der:String?=nil,ca_der:String?=nil,device_id:String?=nil){self.version=version;self.type=type;self.url=url;self.token=token;self.certificate_der=certificate_der;self.ca_der=ca_der;self.device_id=device_id}
    public func validate() throws {
        guard let u=URLComponents(string:url),let host=u.host else { throw FabricError("Invalid Mac address") }
        try require((version==1 || version==2) && type=="memory-fabric-pairing" && u.scheme=="https" && u.user==nil && u.password==nil && u.query==nil && u.fragment==nil,"Invalid pairing file")
        _ = try BeaconInvitation.endpoint(url)
        try require(token.count>=32 && token.count<=256,"Invalid device credential")
        if version==2 && certificate_der==nil && ca_der==nil {return}
        try require(version==1 || Self.isLocal(url),"Certificate pairing requires a local beacon")
        try require(u.path.isEmpty || u.path=="/","Local Mac pairing requires a root address")
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
        guard let encoded=certificate_der,let data=Data(base64Encoded:encoded) else { throw FabricError("Invalid pairing certificate") }
        try require(SecCertificateCreateWithData(nil,data as CFData) != nil,"Invalid pairing certificate")
    }
    public static func isLocal(_ address:String)->Bool {
        guard let host=URLComponents(string:address)?.host else{return false}
        let parts=host.split(separator:".").compactMap{Int($0)}
        let ip=parts.count==4 && parts.allSatisfy{(0...255).contains($0)}
        return host=="localhost" || host.hasSuffix(".local") || (ip && (parts[0]==10 || parts==[127,0,0,1] || (parts[0]==192 && parts[1]==168) || (parts[0]==172 && (16...31).contains(parts[1]))))
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
    public static func forget() throws {
        let query:[String:Any]=[kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:"local.memoryfabric.pairing",kSecAttrAccount as String:"mac"]
        let status=SecItemDelete(query as CFDictionary);try require(status==errSecSuccess || status==errSecItemNotFound,"Could not disconnect beacon")
    }
}

public final class SyncClient:NSObject,URLSessionDelegate,URLSessionTaskDelegate,@unchecked Sendable {
    private let pairing:Pairing
    private lazy var session:URLSession={
        let config=URLSessionConfiguration.ephemeral;config.timeoutIntervalForRequest=110;config.timeoutIntervalForResource=150
        return URLSession(configuration:config,delegate:self,delegateQueue:nil)
    }()
    public init(pairing:Pairing) throws { try pairing.validate();self.pairing=pairing;super.init() }
    public var baseURL:String { pairing.url }
    public func urlSession(_ session:URLSession,didReceive challenge:URLAuthenticationChallenge,completionHandler:@escaping(URLSession.AuthChallengeDisposition,URLCredential?)->Void) {
        if pairing.certificate_der==nil { completionHandler(.performDefaultHandling,nil);return }
        guard challenge.protectionSpace.authenticationMethod==NSURLAuthenticationMethodServerTrust,
              let trust=challenge.protectionSpace.serverTrust,let encoded=pairing.certificate_der,let data=Data(base64Encoded:encoded),
              let anchorData=Data(base64Encoded:pairing.ca_der ?? encoded),
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
        if path == "v1/device-usage" { request.timeoutInterval = 4 }
        request.setValue("Bearer "+pairing.token,forHTTPHeaderField:"Authorization");request.setValue("application/json",forHTTPHeaderField:"Content-Type")
        let data:Data;let response:URLResponse
        do { (data,response)=try await session.data(for:request) }
        catch let error as URLError {
            let address=pairing.url
            switch error.code {
            case .cannotFindHost:
                throw FabricError("Cannot resolve the beacon at \(address). Check its URL and your network.")
            case .cannotConnectToHost,.timedOut:
                throw FabricError("Cannot connect to \(address). Check that the beacon is deployed and running. Local chat remains available.")
            case .serverCertificateUntrusted,.secureConnectionFailed:
                throw FabricError("The beacon’s HTTPS certificate could not be verified. Check the address and server certificate.")
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
    public struct BeaconDevice:Decodable{public let id:String;public let name:String}
    public func devices() async throws ->[BeaconDevice]{struct Directory:Decodable{let devices:[BeaconDevice]};let directory:Directory=try await request("v1/devices");return directory.devices.filter{$0.id != pairing.device_id}}
    private struct Push:Codable { let events:[Event] }
    private struct Ack:Decodable { let commit:String;let inserted:Int }
    private struct Page:Decodable { let commit:String;let events:[Event];let next_after:String? }
    public struct SyncResult:Codable { public let uploaded:Int;public let downloaded:Int;public let commit:String }
    public func sync(ledger:Ledger,adapter:Adapter) async throws -> SyncResult {
        let events=try ledger.events().filter{$0.access?.syncable ?? true};var uploaded=0;var downloaded=0
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
            let allowed=page.events.filter{event in guard let access=event.access else{return true};if event.actor==ledger.actor{return true};if access.scope=="private"{return false};return access.scope=="public"};downloaded+=try ledger.ingest(allowed,adapter:adapter);commit=page.commit;after=page.next_after
        } while after != nil
        if pairing.device_id != nil {
            struct UsageAcknowledgement: Decodable { let received: Bool }
            let sample = sampleDeviceUsage(at: NSHomeDirectory())
            if !sample.isEmpty, let data = try? JSONSerialization.data(withJSONObject: sample) {
                let _: UsageAcknowledgement? = try? await request("v1/device-usage", body: data)
            }
        }
        return SyncResult(uploaded:uploaded,downloaded:downloaded,commit:commit ?? "")
    }
    public struct Turn:Codable { public let role:String;public let content:String;public init(role:String,content:String){self.role=role;self.content=content} }
    public struct Answer:Codable { public let text:String;public let model:String }
    private struct Prompt:Codable { let question:String;let context:String;let history:[Turn] }
    public func prompt(question:String,context:String,history:[Turn]) async throws -> Answer {
        try await request("v1/prompt",body:JSONEncoder().encode(Prompt(question:question,context:context,history:Array(history.suffix(12)))))
    }
}
