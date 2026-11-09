import Foundation

public actor PhoneSession {
    private let encoder:Encoder
    private let adapter:Adapter
    private let ledger:Ledger
    private let generator:LocalGenerator
    private let historyURL:URL
    private var turns:[SyncClient.Turn]
    private var client:SyncClient?
    private var syncTask:Task<SyncClient.SyncResult,Error>?
    private var automaticSync:AutomaticSync?
    public func startAutomaticSync() async {
        if automaticSync == nil {
            automaticSync=AutomaticSync { [weak self] in
                guard let self else { return }
                _ = try await self.sync()
            }
        }
        await automaticSync?.start()
    }
    public func stopAutomaticSync() async { await automaticSync?.stop() }
    public func automaticSyncStatus() async -> String {
        guard client != nil else { return "Pair your Mac to enable automatic memory sync." }
        return await automaticSync?.status() ?? "Automatic memory sync starting…"
    }
    private func changed() { Task { await automaticSync?.request() } }
    private func exchange(using client:SyncClient) async throws -> SyncClient.SyncResult {
        // Swift actors are reentrant across network awaits. Join one exchange so
        // manual sync, polling, and writes cannot start overlapping transfers.
        if let syncTask { return try await syncTask.value }
        let ledger=self.ledger, adapter=self.adapter
        let task=Task { try await client.sync(ledger:ledger,adapter:adapter) }
        syncTask=task
        defer { syncTask=nil }
        return try await task.value
    }
    public init(assets:URL,storage:URL) throws {
        try FileManager.default.createDirectory(at:storage,withIntermediateDirectories:true)
        var local=storage;var flags=URLResourceValues();flags.isExcludedFromBackup=true;try local.setResourceValues(flags)
        encoder=try Encoder(folder:assets.appendingPathComponent("model"));adapter=try Adapter(folder:assets.appendingPathComponent("adapter"))
        ledger=try Ledger(path:storage.appendingPathComponent("memory.sqlite"),space:encoder.manifest.space,dimension:encoder.manifest.dimension)
        generator=try LocalGenerator(folder:assets.appendingPathComponent("generation"))
        historyURL=storage.appendingPathComponent("conversation.json")
        turns=(try? JSONDecoder().decode([SyncClient.Turn].self,from:Data(contentsOf:historyURL))) ?? []
        if let pair=try Pairing.load(){client=try SyncClient(pairing:pair)}
    }
    public func conversation()->[SyncClient.Turn]{turns}
    public func isPaired()->Bool{client != nil}
    public func releaseGenerator(){generator.unload()}
    public func pair(data:Data) async throws -> SyncClient.SyncResult {
        let pairing=try JSONDecoder().decode(Pairing.self,from:data);try pairing.validate()
        let next=try SyncClient(pairing:pairing)
        if let syncTask { _ = await syncTask.result }
        let result=try await exchange(using:next)
        try pairing.save();client=next;changed();return result
    }
    public func sync() async throws -> SyncClient.SyncResult {
        guard let client else {throw FabricError("Connect Mac using the pairing file from the Mac app first")}
        return try await exchange(using:client)
    }
    public func send(_ question:String) async throws -> SyncClient.Answer {
        try require(!question.trimmingCharacters(in:.whitespacesAndNewlines).isEmpty && question.utf8.count<=4000,"Prompt is empty or exceeds 4,000 bytes")
        let vector=try encoder.encode(question)
        let id=Foundation.UUID().uuidString.lowercased()
        let memoryID="chat-"+id+"-user"
        // Persist locally before generation; chat has no network dependency.
        try ledger.put(memory:memoryID,text:question,embedding:vector)
        // Even failed generation leaves a durable question that should sync.
        defer { changed() }
        let hits=try ledger.search(vector,k:20).filter{!$0.memory.hasSuffix("-user")}.prefix(3)
        let context=hits.map { "[\($0.memory)]\n\($0.text.prefix(600))" }.joined(separator:"\n\n")
        let history=turns
        let answer=try generator.answer(question:question,context:context,history:history)
        try ledger.put(memory:"chat-"+id+"-assistant",text:answer.text,embedding:encoder.encode(answer.text))
        turns.append(SyncClient.Turn(role:"user",content:question));turns.append(SyncClient.Turn(role:"assistant",content:answer.text))
        turns=Array(turns.suffix(100));try writeJSON(turns,to:historyURL)
        return answer
    }
}


// One coalescing worker plus a bounded poll/retry. No network work blocks a
// local write, and requests arriving during a transfer cause one further pass.
actor AutomaticSync {
    private let operation:() async throws -> Void
    private let interval:UInt64
    private var active=false
    private var pending=false
    private var polling:Task<Void,Never>?
    private var worker:Task<Void,Never>?
    private var message="Automatic memory sync starting…"
    init(interval:UInt64=10_000_000_000, operation:@escaping () async throws -> Void) {
        self.interval=interval; self.operation=operation
    }
    deinit { polling?.cancel(); worker?.cancel() }
    func start() {
        guard !active else { return }
        active=true
        request()
        let interval=self.interval
        polling=Task { [weak self] in
            while !Task.isCancelled {
                do { try await Task.sleep(nanoseconds:interval) } catch { return }
                guard !Task.isCancelled else { return }
                await self?.request()
            }
        }
    }
    func stop() {
        active=false;pending=false;polling?.cancel();polling=nil
        // Let an existing transfer finish; resume joins it instead of duplicating it.
    }
    func request() {
        guard active else { return }
        pending=true
        guard worker == nil else { return }
        worker=Task { [weak self] in await self?.drain() }
    }
    private func drain() async {
        while active && pending && !Task.isCancelled {
            pending=false
            message="Automatically syncing memories…"
            do { try await operation(); message="Auto sync up to date." }
            catch { message="Auto sync waiting for the Mac; retrying automatically." }
        }
        worker=nil
    }
    func status()->String { message }
    func waitUntilIdle() async { await worker?.value }
}
