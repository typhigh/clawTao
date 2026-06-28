use crate::session_actor::{actor_loop, SessionMsg, SessionRegistry};
use crate::store::json_store::JsonSessionStore;
use crate::store::store_trait::SessionStore;
use reqwest::blocking::Client;
use serde_json::Value;
use std::sync::{Arc, Barrier, atomic, atomic::Ordering, mpsc};


fn make_registry() -> SessionRegistry {
    let dir = std::env::temp_dir().join(format!("clawtao_test_actor_{}", uuid::Uuid::new_v4()));
    SessionRegistry::new(Arc::new(JsonSessionStore::new(dir)))
}

#[test]
fn get_or_spawn_reuses_existing_actor() {
    let reg = make_registry();
    let spawned = Arc::new(atomic::AtomicBool::new(false));
    let s = Arc::clone(&spawned);

    let _tx1 = reg.get_or_spawn("s1", move |rx| {
        s.store(true, Ordering::SeqCst);
        std::thread::spawn(move || {
            for msg in rx {
                if matches!(msg, SessionMsg::Shutdown) { break; }
            }
        })
    });
    assert!(spawned.load(Ordering::SeqCst));

    let spawned2 = Arc::new(atomic::AtomicBool::new(false));
    let s2 = Arc::clone(&spawned2);
    let _tx2 = reg.get_or_spawn("s1", move |_rx| {
        s2.store(true, Ordering::SeqCst);
        std::thread::spawn(|| {})
    });
    assert!(!spawned2.load(Ordering::SeqCst));

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

#[test]
fn actor_loop_processes_one_run_per_message() {
    let count = Arc::new(atomic::AtomicU32::new(0));
    let c = Arc::clone(&count);

    let (tx, rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();

    let c2 = Arc::clone(&c);
    std::thread::spawn(move || {
        actor_loop(rx, "test",
            Arc::new(JsonSessionStore::new(
                std::env::temp_dir().join(format!("clawtao_test_al_{}", uuid::Uuid::new_v4()))
            )),
            move |_client: &Client, _store: &dyn SessionStore, _params: Value, _rid: Option<Value>| {
                c2.fetch_add(1, Ordering::SeqCst);
            },
        );
        let _ = done_tx.send(());
    });

    tx.send(SessionMsg::Run { params: serde_json::json!({}), response_id: None }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    tx.send(SessionMsg::Shutdown).unwrap();
    done_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 1);
}

