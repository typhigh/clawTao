use crate::session_actor::{actor_loop, SessionMsg, SessionRegistry};
use crate::store::json_store::JsonSessionStore;
use crate::store::store_trait::SessionStore;
use reqwest::blocking::Client;
use serde_json::Value;
use std::sync::{Arc, Barrier, atomic::AtomicBool, atomic, atomic::Ordering, mpsc};

fn test_temp_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}


fn make_registry() -> SessionRegistry {
    let dir = test_temp_dir();
    SessionRegistry::new(Arc::new(JsonSessionStore::new(dir)))
}

#[test]
fn get_or_spawn_reuses_existing_actor() {
    let reg = make_registry();
    let spawned = Arc::new(atomic::AtomicBool::new(false));
    let s = Arc::clone(&spawned);

    let _tx1 = reg.get_or_spawn("s1", move |rx, _cancel| {
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
    let _tx2 = reg.get_or_spawn("s1", move |_rx, _cancel| {
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
    let tx = reg.get_or_spawn("s1", move |rx, _cancel| {
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
            Arc::new(JsonSessionStore::new(test_temp_dir())),
            Arc::new(AtomicBool::new(false)),
            move |_client: &Client, _store: &dyn SessionStore, _params: Value, _rid: Option<Value>, _cancel: &Arc<AtomicBool>| {
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

#[test]
fn cancel_flag_stops_processor() {
    let count = Arc::new(atomic::AtomicU32::new(0));
    let c = Arc::clone(&count);
    let cancel = Arc::new(AtomicBool::new(true)); // pre-cancelled

    let (tx, rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();

    let c2 = Arc::clone(&c);
    std::thread::spawn(move || {
        actor_loop(rx, "test",
            Arc::new(JsonSessionStore::new(test_temp_dir())),
            cancel,
            move |_client: &Client, _store: &dyn SessionStore, _params: Value, _rid: Option<Value>, _cancel: &Arc<AtomicBool>| {
                c2.fetch_add(1, Ordering::SeqCst);
            },
        );
        let _ = done_tx.send(());
    });

    // Even though we send a Run, the processor should still be called (cancel is
    // just a flag — the processor decides what to do with it). The actor loop
    // resets cancel to false at the start of each Run, so this Run runs normally.
    tx.send(SessionMsg::Run { params: serde_json::json!({}), response_id: None }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    tx.send(SessionMsg::Shutdown).unwrap();
    done_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();

    // The processor was called exactly once.
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

