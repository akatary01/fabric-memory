import Foundation
import UIKit
import FabricCore
import Darwin

private struct TokenCase: Codable { let text: String; let ids: [Int64] }
private struct Job: Codable {
    let version: Int
    let documents: [String]
    let queries: [String]
    let tokenizer_cases: [TokenCase]
    let reference_phone_vectors: [[Float]]
    let stress_count: Int
}
private struct Encoded: Codable {
    let document_vectors: [Embedding]
    let query_vectors: [Embedding]
    let document_ms: [Double]
    let query_ms: [Double]
    let native_status: Status
    let reference_max_absolute_error: Float
    let tokenizer_cases_passed: Int
    let peak_rss_bytes: Int
    let thermal_state: Int
    let device: String
}

/// TestFlight diagnostics only; no product UI or memory/business logic lives here.
final class DiagnosticsController: UIViewController {
    private let label = UILabel()
    private let share = UIButton(type:.system)
    private var result: URL?
    override func viewDidLoad() {
        super.viewDidLoad(); view.backgroundColor = .systemBackground
        label.text = "Running embedding translation checks…"
        label.numberOfLines = 0; label.textAlignment = .center
        share.setTitle("Share results to Mac",for:.normal); share.isHidden = true
        share.addTarget(self,action:#selector(shareResult),for:.touchUpInside)
        let stack = UIStackView(arrangedSubviews:[label,share]); stack.axis = .vertical; stack.spacing = 24
        stack.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(stack)
        NSLayoutConstraint.activate([stack.centerYAnchor.constraint(equalTo:view.centerYAnchor),
            stack.leadingAnchor.constraint(equalTo:view.safeAreaLayoutGuide.leadingAnchor,constant:24),
            stack.trailingAnchor.constraint(equalTo:view.safeAreaLayoutGuide.trailingAnchor,constant:-24)])
    }
    func update(_ message: String, result: URL? = nil) {
        DispatchQueue.main.async { self.label.text = message; self.result = result; self.share.isHidden = result == nil }
    }
    @objc private func shareResult() {
        guard let result else { return }
        let controller = UIActivityViewController(activityItems:[result],applicationActivities:nil)
        controller.popoverPresentationController?.sourceView = share
        present(controller,animated:true)
    }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene:windowScene)
        window.rootViewController = UINavigationController(rootViewController:ChatController()); window.backgroundColor = .systemBackground
        window.makeKeyAndVisible(); self.window = window
    }
}

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        true
    }
    func start(_ diagnostics: DiagnosticsController) {
        let args = ProcessInfo.processInfo.arguments
        if args.contains("--run") {
            DispatchQueue.global(qos: .userInitiated).async {
                self.run(args)
            }
        } else {
            DispatchQueue.global(qos:.userInitiated).async { self.runBundledDemo(diagnostics) }
        }
    }
    private func runBundledDemo(_ diagnostics: DiagnosticsController) {
        do {
            guard let assets = Bundle.main.url(forResource:"DemoAssets",withExtension:nil) else {
                throw FabricError("No bundled demo assets. Run tools/stage_testflight.py before archiving.")
            }
            let demo = try JSONSerialization.jsonObject(with:Data(contentsOf:assets.appendingPathComponent("demo.json"))) as! [String:Any]
            let demoID = demo["demo_id"] as! String
            try require(demoID.count == 64 && demoID.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) },"invalid demo ID")
            let documents = FileManager.default.urls(for:.documentDirectory,in:.userDomainMask)[0]
            let root = documents.appendingPathComponent("TestFlightDemos/\(demoID)")
            try FileManager.default.createDirectory(at:root,withIntermediateDirectories:true)
            for name in ["model","adapter","job.json","incoming.jsonl"] {
                let target = root.appendingPathComponent(name)
                if !FileManager.default.fileExists(atPath:target.path) { try FileManager.default.copyItem(at:assets.appendingPathComponent(name),to:target) }
            }
            let id = "testflight-"+String(Foundation.UUID().uuidString.lowercased().prefix(12))
            let output = root.appendingPathComponent("runs/\(id)")
            for mode in ["encode","import","stress"] {
                diagnostics.update(mode == "encode" ? "Generating MiniLM embeddings on this iPhone…" : mode == "import" ? "Searching translated Mac memories…" : "Testing 1,000 durable memory writes…")
                self.run(["FabricRunner","--run","--mode",mode,"--run-id",id,"--count","1000"],rootOverride:root)
                let marker = mode == "stress" ? "stress-1000" : mode
                if FileManager.default.fileExists(atPath:output.appendingPathComponent("error-\(marker).json").path) {
                    throw FabricError(String(decoding:try Data(contentsOf:output.appendingPathComponent("error-\(marker).json")),as:UTF8.self))
                }
                try require(FileManager.default.fileExists(atPath:output.appendingPathComponent("done-\(marker).json").path),"demo did not complete")
            }
            func object(_ name:String) throws -> Any { try JSONSerialization.jsonObject(with:Data(contentsOf:output.appendingPathComponent(name))) }
            let events = try readEvents(output.appendingPathComponent("events.jsonl"))
            let export: [String:Any] = ["schema_version":1,"type":"memory-fabric-device-export","demo_id":demoID,
                "run_id":id,"build_number":Bundle.main.infoDictionary?["CFBundleVersion"] ?? "unknown",
                "encode":try object("encode-results.json"),"mac_to_phone":try object("import-results.json"),
                "stress":try object("stress-1000.json"),"events":try JSONSerialization.jsonObject(with:JSONEncoder().encode(events))]
            let result = output.appendingPathComponent("MemoryFabric-\(id).json")
            try JSONSerialization.data(withJSONObject:export,options:[.prettyPrinted,.sortedKeys]).write(to:result,options:.atomic)
            diagnostics.update("Demo complete.\n\nMiniLM ran on this iPhone, translated Mac memories were searched, and 1,000 durable writes were tested.\n\nShare the result file to finish the reverse translation check on your Mac.",result:result)
        } catch { diagnostics.update("Demo failed:\n\(error)") }
    }
    private func run(_ args: [String], rootOverride: URL? = nil) {
        func arg(_ name: String, _ fallback: String) -> String {
            guard let i = args.firstIndex(of: name), i+1 < args.count else { return fallback }; return args[i+1]
        }
        let root = rootOverride ?? FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("fabric")
        let id = arg("--run-id", "manual")
        guard id.count <= 64 && !id.isEmpty && id.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 45 }) else { return }
        let output = root.appendingPathComponent("runs/\(id)")
        let mode = arg("--mode", "encode")
        let marker = mode == "stress" ? "stress-\(arg("--count", "1000"))" : mode
        do {
            try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
            let manifest = try JSONDecoder().decode(EncoderManifest.self, from: Data(contentsOf: root.appendingPathComponent("model/encoder.json")))
            let job = try JSONDecoder().decode(Job.self, from: Data(contentsOf: root.appendingPathComponent("job.json")))
            try require(job.version == 1 && job.documents.count <= 1000 && job.queries.count <= 1000, "invalid job")
            let native = try Ledger(path: output.appendingPathComponent("native.sqlite"), space: manifest.space, dimension: manifest.dimension)
            switch mode {
            case "encode":
                let encoder = try Encoder(folder: root.appendingPathComponent("model"))
                for test in job.tokenizer_cases { try require(encoder.tokenizer.encode(test.text) == test.ids, "tokenizer parity failure") }
                var docs: [Embedding] = []; var queries: [Embedding] = []; var documentMS: [Double] = []; var queryMS: [Double] = []
                var maxError: Float = 0
                for (i,text) in (job.documents+job.queries+job.tokenizer_cases.map { $0.text }).enumerated() {
                    let start = ProcessInfo.processInfo.systemUptime
                    let vector = try encoder.encode(text)
                    let ms = (ProcessInfo.processInfo.systemUptime-start)*1000
                    let reference = job.reference_phone_vectors[i]
                    try require(reference.count == vector.values.count, "reference shape mismatch")
                    maxError = max(maxError, zip(reference,vector.values).map { abs($0-$1) }.max() ?? 0)
                    if i < job.documents.count {
                        try native.put(memory: "eval-\(String(format: "%04d",i))", text: text, embedding: vector)
                        docs.append(vector); documentMS.append(ms)
                    } else if i < job.documents.count+job.queries.count { queries.append(vector); queryMS.append(ms) }
                }
                try require(maxError < 0.0001, "phone encoder differs from training encoder")
                var usage = rusage(); getrusage(RUSAGE_SELF, &usage)
                let result = Encoded(document_vectors: docs, query_vectors: queries, document_ms: documentMS, query_ms: queryMS,
                    native_status: try native.status(), reference_max_absolute_error: maxError,
                    tokenizer_cases_passed: job.tokenizer_cases.count, peak_rss_bytes: Int(usage.ru_maxrss),
                    thermal_state: ProcessInfo.processInfo.thermalState.rawValue, device: "physical-iphone")
                try writeJSON(result, to: output.appendingPathComponent("encode-results.json"))
                try native.export(to: output.appendingPathComponent("events.jsonl"))
            case "import":
                let encoded = try JSONDecoder().decode(Encoded.self, from: Data(contentsOf: output.appendingPathComponent("encode-results.json")))
                let foreign = try Ledger(path: output.appendingPathComponent("foreign.sqlite"), space: manifest.space, dimension: manifest.dimension)
                let adapter = try Adapter(folder: root.appendingPathComponent("adapter"))
                let start = ProcessInfo.processInfo.systemUptime
                let inserted = try foreign.ingest(readEvents(root.appendingPathComponent("incoming.jsonl")), adapter: adapter)
                let elapsed = ProcessInfo.processInfo.systemUptime-start
                var nativeRecall = 0; var translatedRecall = 0; var overlap = 0.0; var reciprocal = 0.0
                var rankings: [[String]] = []
                for (i,query) in encoded.query_vectors.enumerated() {
                    let a = try native.search(query,k:5).map { $0.memory }
                    let b = try foreign.search(query,k:5).map { $0.memory }
                    let relevant = "eval-\(String(format: "%04d",i))"
                    if a.contains(relevant) { nativeRecall += 1 }
                    if let index = b.firstIndex(of: relevant) { translatedRecall += 1; reciprocal += 1.0/Double(index+1) }
                    overlap += Double(Set(a).intersection(b).count)/5.0
                    rankings.append(b)
                }
                let count = Double(encoded.query_vectors.count)
                let report: [String: Any] = ["device":"physical-iphone","direction":"mac-to-phone","inserted":inserted,
                    "import_seconds":elapsed,"documents_reembedded_during_import":0,"document_encoder_loaded_during_import":false,
                    "native_recall_at_5":Double(nativeRecall)/count,"translated_recall_at_5":Double(translatedRecall)/count,
                    "translated_mrr_at_5":reciprocal/count,"native_top5_overlap":overlap/count,"rankings":rankings]
                try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted,.sortedKeys]).write(to: output.appendingPathComponent("import-results.json"),options:.atomic)
                try writeJSON(foreign.status(),to:output.appendingPathComponent("foreign-status.json"))
            case "converge":
                let combined = try Ledger(path: output.appendingPathComponent("combined.sqlite"), space: manifest.space, dimension: manifest.dimension)
                _ = try combined.ingest(readEvents(root.appendingPathComponent("incoming.jsonl")),adapter:Adapter(folder:root.appendingPathComponent("adapter")))
                try writeJSON(combined.status(),to:output.appendingPathComponent("convergence-status.json"))
                try combined.export(to:output.appendingPathComponent("converged-events.jsonl"))
            case "stress":
                let count = Int(arg("--count",String(job.stress_count))) ?? 0
                try require((1...100_000).contains(count), "invalid stress count")
                let available = try FileManager.default.attributesOfFileSystem(forPath:output.path)[.systemFreeSize] as? NSNumber
                try require((available?.uint64Value ?? 0) > UInt64(count)*40_000+512*1024*1024, "insufficient storage headroom for stress run")
                let pool = try readEvents(output.appendingPathComponent("events.jsonl"))
                try require(!pool.isEmpty, "empty stress pool")
                let stress = try Ledger(path: output.appendingPathComponent("stress-\(count).sqlite"),space:manifest.space,dimension:manifest.dimension)
                var times: [Double] = []
                let start = ProcessInfo.processInfo.systemUptime
                for i in 0..<count {
                    if ProcessInfo.processInfo.thermalState == .critical { throw FabricError("stopped at critical thermal state") }
                    let e = pool[i%pool.count]; let t = ProcessInfo.processInfo.systemUptime
                    try stress.put(memory:"stress-\(i)",text:e.text,embedding:e.embedding)
                    times.append((ProcessInfo.processInfo.systemUptime-t)*1000)
                }
                let elapsed = ProcessInfo.processInfo.systemUptime-start; times.sort()
                let encoded = try JSONDecoder().decode(Encoded.self,from:Data(contentsOf:output.appendingPathComponent("encode-results.json")))
                var queryTimes: [Double] = []
                for query in encoded.query_vectors.prefix(8) {
                    let t = ProcessInfo.processInfo.systemUptime
                    _ = try stress.search(query,k:5)
                    queryTimes.append((ProcessInfo.processInfo.systemUptime-t)*1000)
                }
                queryTimes.sort()
                var usage = rusage(); getrusage(RUSAGE_SELF,&usage)
                let report: [String: Any] = ["device":"physical-iphone","workload":"durable-single-event-writes-reusing-real-embeddings",
                    "count":count,"seconds":elapsed,"events_per_second":Double(count)/elapsed,"p50_ms":times[count/2],"p95_ms":times[min(count*95/100,count-1)],
                    "query_p50_ms":queryTimes[queryTimes.count/2],"query_max_ms":queryTimes.last!,
                    "peak_rss_bytes":Int(usage.ru_maxrss),"thermal_state":ProcessInfo.processInfo.thermalState.rawValue]
                try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:output.appendingPathComponent("stress-\(count).json"),options:.atomic)
            default: throw FabricError("unknown runner mode")
            }
            try writeJSON(["mode":mode,"success":"true"],to:output.appendingPathComponent("done-\(marker).json"))
            print("FABRIC_SUCCESS \(mode)")
        } catch {
            try? writeJSON(["mode":mode,"error":String(describing:error)],to:output.appendingPathComponent("error-\(marker).json"))
            print("FABRIC_ERROR \(error)")
        }
    }
}
