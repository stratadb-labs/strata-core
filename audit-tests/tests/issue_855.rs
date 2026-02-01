//! Audit test for issue #855: Global recovery registry uses poisonable Mutex
//! Verdict: CONFIRMED BUG
//!
//! OPEN_DATABASES uses std::sync::Mutex which becomes permanently poisoned
//! if a thread panics while holding the lock. This can cascade failures
//! to all subsequent Database::open() calls.

use std::sync::Mutex;

#[test]
fn issue_855_std_mutex_poisoning_demonstration() {
    // Demonstrate that std::sync::Mutex becomes poisoned after a thread panic.
    // This is the same Mutex type used by OPEN_DATABASES in registry.rs.

    let local_mutex = std::sync::Arc::new(Mutex::new(vec!["entry".to_string()]));
    let mutex_clone = local_mutex.clone();

    // Spawn a thread that panics while holding the lock
    let handle = std::thread::spawn(move || {
        let _guard = mutex_clone.lock().unwrap();
        panic!("intentional panic while holding lock");
    });

    // Wait for the panic
    let _ = handle.join();

    // The mutex is now poisoned -- all subsequent lock() calls fail
    let result = local_mutex.lock();
    assert!(
        result.is_err(),
        "BUG CONFIRMED: std::sync::Mutex becomes poisoned after thread panic. \
         OPEN_DATABASES in registry.rs uses the same Mutex type, making it \
         vulnerable to permanent poisoning if any thread panics during \
         Database::open() or Database::drop()."
    );
}

#[test]
fn issue_855_parking_lot_mutex_does_not_poison() {
    // Demonstrate that parking_lot::Mutex (a workspace dependency) does not poison.
    // This is the recommended replacement for std::sync::Mutex.

    let local_mutex = std::sync::Arc::new(parking_lot::Mutex::new(vec!["entry".to_string()]));
    let mutex_clone = local_mutex.clone();

    // Spawn a thread that panics while holding the lock
    let handle = std::thread::spawn(move || {
        let _guard = mutex_clone.lock();
        panic!("intentional panic while holding lock");
    });

    // Wait for the panic
    let _ = handle.join();

    // parking_lot::Mutex is NOT poisoned -- subsequent lock() calls succeed
    let guard = local_mutex.lock();
    assert_eq!(
        guard.len(),
        1,
        "parking_lot::Mutex should remain usable after thread panic"
    );
}
