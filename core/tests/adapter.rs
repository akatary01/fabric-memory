use fabric_core::{adapter::Adapter, ledger::Ledger, sha256, Embedding, Event};
use tempfile::tempdir;

#[test]
fn checksum_dimensions_and_adapter_revision_are_enforced() {
    let root = tempdir().unwrap();
    let folder = root.path().join("adapter");
    std::fs::create_dir(&folder).unwrap();
    let source = "a".repeat(64);
    let target = "b".repeat(64);
    let raw: Vec<u8> = [1f32, 0., 0., 1.]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    std::fs::write(folder.join("weights.f32"), &raw).unwrap();
    let mut metadata = serde_json::json!({"version":1,"source":source,"target":target,"input_dim":2,"output_dim":2,
        "source_mean":[0.,0.],"target_mean":[0.,0.],"weights_sha256":sha256(&raw),"training_sha256":"0".repeat(64),"ridge":1.});
    std::fs::write(
        folder.join("adapter.json"),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    let a = Adapter::load(&folder).unwrap();
    let input = Embedding {
        space: source.clone(),
        values: vec![1., 0.],
    };
    assert_eq!(a.apply(&input).unwrap().values, vec![1., 0.]);
    assert!(a
        .apply(&Embedding {
            space: target.clone(),
            values: vec![1., 0.]
        })
        .is_err());
    let mut db = Ledger::open(&root.path().join("db"), &target, 2).unwrap();
    let e = Event {
        version: 1,
        id: uuid::Uuid::new_v4().to_string(),
        actor: uuid::Uuid::new_v4().to_string(),
        clock: 1,
        memory: "x".into(),
        deleted: false,
        text: "x".into(),
        embedding: Some(input),
    };
    db.ingest(&[e.clone()], Some(&a)).unwrap();
    metadata["target_mean"] = serde_json::json!([0.1, 0.]);
    std::fs::write(
        folder.join("adapter.json"),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    let b = Adapter::load(&folder).unwrap();
    assert!(db.ingest(&[e], Some(&b)).is_err());
    std::fs::write(folder.join("weights.f32"), [0u8; 16]).unwrap();
    assert!(Adapter::load(&folder).is_err());
}

#[test]
fn real_trained_adapters_match_python_golden_vectors_when_assets_present() {
    let Some(path) = std::env::var_os("FABRIC_TEST_ASSETS") else {
        return;
    };
    let root = std::path::PathBuf::from(path);
    let golden: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("golden.json")).unwrap()).unwrap();
    for direction in ["phone-to-mac", "mac-to-phone"] {
        let adapter = Adapter::load(&root.join("adapters").join(direction)).unwrap();
        let input: Embedding = serde_json::from_value(golden[direction]["input"].clone()).unwrap();
        let expected: Embedding =
            serde_json::from_value(golden[direction]["expected"].clone()).unwrap();
        let actual = adapter.apply(&input).unwrap();
        assert_eq!(actual.space, expected.space);
        let max = actual
            .values
            .iter()
            .zip(expected.values)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max < 0.00001, "{direction}: {max}");
    }
}
