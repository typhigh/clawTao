use crate::session_actor::{SessionMsg, SessionRegistry};
use crate::store::json_store::JsonSessionStore;
use std::sync::{Arc, Barrier};

fn make_registry() -> SessionRegistry {
    let dir = std::env::temp_dir().join(format!("clawtao_test_actor_{}", uuid::Uuid::new_v4()));
    SessionRegistry::new(Arc::new(JsonSessionStore::new(dir)))
}

#[test]
fn get_or_spawn_reuses_existing_actor() {
    let reg = make_registry();
    let spawned = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s = Arc::clone(&spawned);

    let _tx1 = reg.get_or_spawn("s1", move |rx| {
        s.store(true, std::sync::atomic::Ordering::SeqCst);
        std::thread::spawn(move || {
            for msg in rx {
                if matches!(msg, SessionMsg::Shutdown) { break; }
            }
        })
    });
    assert!(spawned.load(std::sync::atomic::Ordering::SeqCst));

    // Second call: factory should NOT run again.
    let spawned2 = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s2 = Arc::clone(&spawned2);
    let _tx2 = reg.get_or_spawn("s1", move |_rx| {
        s2.store(true, std::sync::atomic::Ordering::SeqCst);
        std::thread::spawn(|| {})
    });
    assert!(!spawned2.load(std::sync::atomic::Ordering::SeqCst));

    reg.remove("s1");
}

#[test]
fn remove_shuts_down_actor() {
    let reg = make_registry();
    let barrier = Arc::new(Barrier::new(2));
    let b = Arc::clone(&barrier);
    let tx = reg.get_or_spawn("s1", move |rx| {
        std::thread::spawn(move || {
            b.wait();
            assert!(matches!(rx.recv().unwrap(), SessionMsg::Shutdown));
        })
    });
    drop(tx);
    barrier.wait();
    reg.remove("s1");
}
