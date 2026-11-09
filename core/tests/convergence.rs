use fabric_core::{ledger::Ledger, Embedding, Event};
use tempfile::tempdir;

fn space() -> String {
    "a".repeat(64)
}
fn event(actor: &str, id: &str, clock: u64, deleted: bool) -> Event {
    Event {
        version: 1,
        id: id.into(),
        actor: actor.into(),
        clock,
        memory: "shared".into(),
        deleted,
        text: if deleted { "".into() } else { id.into() },
        embedding: if deleted {
            None
        } else {
            Some(Embedding {
                space: space(),
                values: vec![1., 0., 0.],
            })
        },
    }
}
fn fixtures() -> Vec<Event> {
    vec![
        event(
            "00000000-0000-4000-8000-000000000001",
            "00000000-0000-4000-8000-000000000011",
            1,
            false,
        ),
        event(
            "00000000-0000-4000-8000-000000000002",
            "00000000-0000-4000-8000-000000000012",
            1,
            false,
        ),
        event(
            "00000000-0000-4000-8000-000000000001",
            "00000000-0000-4000-8000-000000000013",
            2,
            true,
        ),
    ]
}

#[test]
fn replay_reordering_duplication_and_restart_converge() {
    let dir = tempdir().unwrap();
    let events = fixtures();
    let mut expected = None;
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let path = dir.path().join(format!("{:?}.sqlite", order));
        {
            let mut db = Ledger::open(&path, &space(), 3).unwrap();
            for i in order {
                assert_eq!(db.ingest(&[events[i].clone()], None).unwrap(), 1);
            }
            assert_eq!(db.ingest(&events, None).unwrap(), 0);
        }
        let db = Ledger::open(&path, &space(), 3).unwrap();
        let status = db.status().unwrap();
        assert_eq!(status.live_memories, 0);
        assert_eq!(status.tombstones, 1);
        assert_eq!(status.events, 3);
        match &expected {
            None => expected = Some(status.state_digest),
            Some(digest) => assert_eq!(digest, &status.state_digest),
        }
    }
}

#[test]
fn collision_rolls_back_entire_batch_and_clock_survives_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let events = fixtures();
    let mut db = Ledger::open(&path, &space(), 3).unwrap();
    db.ingest(&events[..1], None).unwrap();
    let mut collision = events[0].clone();
    collision.text = "tampered".into();
    assert!(db.ingest(&[events[1].clone(), collision], None).is_err());
    assert_eq!(db.status().unwrap().events, 1);
    drop(db);
    let mut db = Ledger::open(&path, &space(), 3).unwrap();
    let local = db
        .put(
            "new",
            "after reopen",
            Some(Embedding {
                space: space(),
                values: vec![0., 1., 0.],
            }),
        )
        .unwrap();
    assert_eq!(local.clock, 2);
}

#[test]
fn foreign_vectors_and_invalid_batches_do_not_enter_native_index() {
    let dir = tempdir().unwrap();
    let mut db = Ledger::open(&dir.path().join("db"), &space(), 3).unwrap();
    let mut bad = fixtures()[0].clone();
    bad.embedding.as_mut().unwrap().space = "b".repeat(64);
    assert!(db.ingest(&[bad], None).is_err());
    let mut bad = fixtures()[0].clone();
    bad.embedding.as_mut().unwrap().values = vec![f32::NAN, 0., 1.];
    assert!(db.ingest(&[bad], None).is_err());
    assert_eq!(db.status().unwrap().events, 0);
    drop(db);
    assert!(Ledger::open(&dir.path().join("db"), &"b".repeat(64), 3).is_err());
}

#[test]
fn shared_database_handles_serialize_clock_allocation() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let mut a = Ledger::open(&path, &space(), 3).unwrap();
    let mut b = Ledger::open(&path, &space(), 3).unwrap();
    let one = a
        .put(
            "a",
            "a",
            Some(Embedding {
                space: space(),
                values: vec![1., 0., 0.],
            }),
        )
        .unwrap();
    let two = b
        .put(
            "b",
            "b",
            Some(Embedding {
                space: space(),
                values: vec![0., 1., 0.],
            }),
        )
        .unwrap();
    assert_eq!(one.clock + 1, two.clock);
    assert_eq!(one.actor, two.actor);
}
