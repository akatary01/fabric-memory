import Foundation
import OnnxRuntimeBindings

public struct EncoderManifest: Codable {
    public let version: Int
    public let model: String
    public let revision: String
    public let dimension: Int
    public let max_tokens: Int
    public let space: String
    public let files: [String: String]
    public let pipeline: String
}
public final class Encoder {
    public let manifest: EncoderManifest
    public let tokenizer: WordPiece
    private let env: ORTEnv
    private let session: ORTSession
    private let inputs: [String]
    private let output: String
    public init(folder: URL) throws {
        manifest = try JSONDecoder().decode(EncoderManifest.self, from: Data(contentsOf: folder.appendingPathComponent("encoder.json")))
        try require(manifest.version == 1 && manifest.dimension == 384 && manifest.max_tokens == 256, "unsupported phone encoder")
        for (name, hash) in manifest.files {
            try require(!name.contains("/") && !name.contains("\\") && !name.hasPrefix("."), "invalid asset path")
            let data = try Data(contentsOf: folder.appendingPathComponent(name), options: .mappedIfSafe)
            try require(digest(data) == hash, "encoder asset checksum mismatch: \(name)")
        }
        tokenizer = try WordPiece(url: folder.appendingPathComponent("tokenizer.json"), limit: manifest.max_tokens)
        env = try ORTEnv(loggingLevel: .warning)
        let options = try ORTSessionOptions()
        try options.setIntraOpNumThreads(2)
        try options.setGraphOptimizationLevel(.all)
        session = try ORTSession(env: env, modelPath: folder.appendingPathComponent("model.onnx").path, sessionOptions: options)
        inputs = try session.inputNames(); output = try session.outputNames()[0]
    }
    public func encode(_ text: String) throws -> Embedding {
        let ids = tokenizer.encode(text)
        var values: [String: ORTValue] = [:]
        for name in inputs {
            let integers: [Int64]
            switch name {
            case "input_ids": integers = ids
            case "attention_mask": integers = [Int64](repeating: 1, count: ids.count)
            case "token_type_ids": integers = [Int64](repeating: 0, count: ids.count)
            default: throw FabricError("unsupported model input: \(name)")
            }
            let data = integers.withUnsafeBytes { NSMutableData(bytes: $0.baseAddress!, length: $0.count) }
            values[name] = try ORTValue(tensorData: data, elementType: .int64, shape: [1, NSNumber(value: ids.count)])
        }
        let outputs = try session.run(withInputs: values, outputNames: Set([output]), runOptions: nil)
        guard let tensor = outputs[output] else { throw FabricError("missing model output") }
        let shape = try tensor.tensorTypeAndShapeInfo().shape.map { $0.intValue }
        try require(shape == [1, ids.count, manifest.dimension], "unexpected encoder output")
        let data = try tensor.tensorData() as Data
        var pooled = [Float](repeating: 0, count: manifest.dimension)
        data.withUnsafeBytes { bytes in
            for token in 0..<ids.count {
                for j in 0..<manifest.dimension {
                    pooled[j] += bytes.loadUnaligned(fromByteOffset: (token*manifest.dimension+j)*4, as: Float.self)
                }
            }
        }
        for j in pooled.indices { pooled[j] /= Float(ids.count) }
        return Embedding(space: manifest.space, values: try normalize(pooled))
    }
}
