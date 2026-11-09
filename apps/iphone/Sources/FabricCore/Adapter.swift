import Foundation

public struct AdapterManifest: Codable {
    public let version: Int
    public let source: String
    public let target: String
    public let input_dim: Int
    public let output_dim: Int
    public let source_mean: [Float]
    public let target_mean: [Float]
    public let weights_sha256: String
    public let training_sha256: String
    public let ridge: Double
}
public final class Adapter {
    public let id: String
    public let manifest: AdapterManifest
    private let weights: [Float]
    public init(folder: URL) throws {
        let manifestData = try Data(contentsOf:folder.appendingPathComponent("adapter.json"))
        id = digest(manifestData)
        manifest = try JSONDecoder().decode(AdapterManifest.self, from:manifestData)
        let m = manifest
        try require(m.version == 1 && m.source != m.target, "invalid adapter")
        try require((1...4096).contains(m.input_dim) && (1...4096).contains(m.output_dim), "invalid dimensions")
        try require(m.source_mean.count == m.input_dim && m.target_mean.count == m.output_dim, "invalid centering dimensions")
        try require((m.source_mean + m.target_mean).allSatisfy { $0.isFinite }, "invalid centering values")
        let data = try Data(contentsOf: folder.appendingPathComponent("weights.f32"))
        try require(data.count == m.input_dim*m.output_dim*4 && digest(data) == m.weights_sha256, "adapter checksum/size mismatch")
        weights = data.withUnsafeBytes { bytes in
            stride(from: 0, to: data.count, by: 4).map { Float(bitPattern: UInt32(littleEndian: bytes.loadUnaligned(fromByteOffset: $0, as: UInt32.self))) }
        }
        try require(weights.allSatisfy { $0.isFinite }, "invalid adapter weights")
    }
    public func apply(_ embedding: Embedding) throws -> Embedding {
        let m = manifest
        try require(embedding.space == m.source && embedding.values.count == m.input_dim, "adapter source mismatch")
        let input = try normalize(embedding.values)
        var output = m.target_mean
        for i in 0..<m.input_dim {
            let x = input[i] - m.source_mean[i]
            for j in 0..<m.output_dim { output[j] += x * weights[i*m.output_dim+j] }
        }
        return Embedding(space: m.target, values: try normalize(output))
    }
}
