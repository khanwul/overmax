use std::sync::{Mutex, MutexGuard};

/// 포이즌된 Mutex에서도 보유 데이터를 버리지 않고 guard를 반환한다.
///
/// 릴리스 빌드는 `panic = "abort"`라 포이즌이 발생하지 않지만, debug/test(unwind)
/// 빌드에서 스레드 패닉으로 인한 포이즌 시 데이터 유실 없이 복구하기 위해 사용한다.
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// 정상 시 클론된 값, 포이즌 시 기본값(`T: Default`)을 반환한다.
///
/// `.lock().map(|g| g.clone()).unwrap_or_default()` 패턴을 대체한다.
pub fn lock_clone_or_default<T: Clone + Default>(mutex: &Mutex<T>) -> T {
    mutex.lock().map(|g| g.clone()).unwrap_or_default()
}
