//! 키보드 클리커 — 키를 **몇 번** 눌렀는지만 센다.
//!
//! # 프라이버시 (절대 조건)
//! 키의 정체(`VKey`, 스캔 코드, 순서, 시각)는 "이미 눌려 있는 키인가?" 를 가리는 비트맵을
//! 갱신하는 데에만 쓰이고 즉시 버려진다. 어디에도 저장·로그하지 않으며, `apply` 로 넘어가는
//! 것은 개수(`u64`) 하나뿐이다. 이 파일에서 키 데이터를 `log::` 로 남기면 안 된다.
//!
//! 구조: `KeySource`(OS 입력) → "새 키가 눌렸다" 콜백 → `AtomicU64` 카운터 → 정산 스레드가
//! 5초마다(또는 100번마다) `apply(n)` 으로 넘긴다.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// 정산 주기.
const SETTLE_EVERY: Duration = Duration::from_secs(5);
/// 이 횟수마다 정산 스레드를 깨운다.
const SETTLE_AT: u64 = 100;

/// 256비트 "눌려 있음" 비트맵에서 **새 누름**만 true.
/// 자동 반복과 이미 눌린 키는 false, 키를 뗄 때(`is_break`)는 비트를 지우고 false.
#[cfg_attr(not(test), allow(dead_code))]
pub fn observe(down: &mut [u64; 4], vk: u8, is_break: bool) -> bool {
    let (i, mask) = (usize::from(vk >> 6), 1u64 << (vk & 63));
    if is_break {
        down[i] &= !mask;
        return false;
    }
    let fresh = down[i] & mask == 0;
    down[i] |= mask;
    fresh
}

/// 장치 변경(키보드 뽑힘 등) 시 비트맵 초기화 — 뗌 이벤트를 놓쳐 키가 "눌린 채" 남는 것을 막는다.
#[cfg(test)]
fn reset(down: &mut [u64; 4]) {
    *down = [0; 4];
}

/// OS 키 입력 원천. `on_press` 는 "새 키가 눌렸다" 마다 (키 정체 없이) 호출된다.
pub trait KeySource: Send {
    /// 콜백 전달을 시작한다. 반환된 핸들을 drop 하면 멈춘다.
    fn start(&mut self, on_press: Box<dyn Fn() + Send + Sync + 'static>) -> Box<dyn KeyHandle>;
}

/// drop 하면 전달이 멈추는 핸들.
pub trait KeyHandle: Send {}

/// 아무것도 세지 않는 원천 (비 Windows).
/// Windows 에서는 `RawInputSource` 가 쓰이므로 만들어지지 않는다.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub struct NullSource;

#[cfg_attr(target_os = "windows", allow(dead_code))]
struct NullHandle;
impl KeyHandle for NullHandle {}

impl KeySource for NullSource {
    fn start(&mut self, _on_press: Box<dyn Fn() + Send + Sync + 'static>) -> Box<dyn KeyHandle> {
        Box::new(NullHandle)
    }
}

struct Shared {
    count: AtomicU64,
    stop: Mutex<bool>,
    wake: Condvar,
    apply: Mutex<Box<dyn Fn(u64) + Send + 'static>>,
}

impl Shared {
    /// 쌓인 수를 비우고 `apply` 로 넘긴다. 넘긴 수를 돌려준다.
    fn settle(&self) -> u64 {
        let n = self.count.swap(0, Ordering::AcqRel);
        if n > 0 {
            let apply = self.apply.lock().unwrap_or_else(|e| e.into_inner());
            apply(n);
        }
        n
    }
}

/// 키 누름 횟수를 센다 (어떤 키인지는 모른다). drop 하면 세기를 멈추고 남은 수를 정산한다.
pub struct KeyClicker {
    shared: Arc<Shared>,
    handle: Option<Box<dyn KeyHandle>>,
    settler: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for KeyClicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyClicker").finish_non_exhaustive()
    }
}

impl KeyClicker {
    /// 실제 OS 원천으로 시작 (Windows: Raw Input, 그 외: `NullSource`).
    /// `apply(n)` 은 백그라운드 스레드에서, 지난 호출 이후의 누름 수와 함께 불린다.
    pub fn start(apply: Box<dyn Fn(u64) + Send + 'static>) -> Option<KeyClicker> {
        #[cfg(target_os = "windows")]
        let source: Box<dyn KeySource> = Box::new(RawInputSource);
        #[cfg(not(target_os = "windows"))]
        let source: Box<dyn KeySource> = Box::new(NullSource);
        Self::start_with(source, apply)
    }

    /// 주입한 원천으로 시작 (테스트).
    pub fn start_with(
        mut source: Box<dyn KeySource>,
        apply: Box<dyn Fn(u64) + Send + 'static>,
    ) -> Option<KeyClicker> {
        let shared = Arc::new(Shared {
            count: AtomicU64::new(0),
            stop: Mutex::new(false),
            wake: Condvar::new(),
            apply: Mutex::new(apply),
        });

        let settler = {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name("merge-keycount-settle".into())
                .spawn(move || {
                    let mut stop = shared.stop.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if *stop {
                            break;
                        }
                        stop = shared
                            .wake
                            .wait_timeout(stop, SETTLE_EVERY)
                            .unwrap_or_else(|e| e.into_inner())
                            .0;
                        if *stop {
                            break;
                        }
                        drop(stop);
                        shared.settle();
                        stop = shared.stop.lock().unwrap_or_else(|e| e.into_inner());
                    }
                })
                .ok()?
        };

        let on_press = {
            let shared = Arc::clone(&shared);
            Box::new(move || {
                let n = shared.count.fetch_add(1, Ordering::AcqRel) + 1;
                if n.is_multiple_of(SETTLE_AT) {
                    shared.wake.notify_one();
                }
            })
        };
        let handle = source.start(on_press);

        Some(KeyClicker { shared, handle: Some(handle), settler: Some(settler) })
    }

    /// 지금 바로 정산한다 (테스트·종료용). 넘긴 수를 돌려준다.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn flush(&self) -> u64 {
        self.shared.settle()
    }
}

impl Drop for KeyClicker {
    fn drop(&mut self) {
        // 원천을 먼저 끊어 더 이상 세지 않게 한 뒤, 정산 스레드를 멈추고 남은 수를 넘긴다.
        self.handle = None;
        *self.shared.stop.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.shared.wake.notify_all();
        if let Some(t) = self.settler.take() {
            let _ = t.join();
        }
        self.shared.settle();
    }
}

#[cfg(target_os = "windows")]
/// Windows Raw Input 원천.
///
/// **동시에 하나만** 살아 있을 수 있다 (콜백·비트맵이 프로세스 전역 static). 이미 살아 있는
/// 상태에서 또 시작하면 아무것도 세지 않는 핸들을 돌려준다. 어느 스레드에서 시작해도 되며
/// (자체 스레드 "merge-keycount" 가 창과 메시지 루프를 가진다), 핸들 drop 은 그 스레드를 join 한다.
pub struct RawInputSource;

#[cfg(target_os = "windows")]
mod raw_input {
    use super::{KeyHandle, KeySource, RawInputSource};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{mpsc, Mutex};
    use std::thread::JoinHandle;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
        RAWINPUTHEADER, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE, RID_INPUT,
        RIM_TYPEKEYBOARD,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        PostThreadMessageW, RegisterClassW, TranslateMessage, UnregisterClassW, HWND_MESSAGE, MSG,
        WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_QUIT, WNDCLASSW,
    };

    /// `RI_KEY_BREAK` (RAWKEYBOARD.Flags 의 키 뗌 비트).
    const KEY_BREAK: u16 = 0x01;
    /// 키보드 `RAWINPUT` 은 이보다 작다. 넘치면 이벤트를 버린다.
    const BUF_LEN: usize = size_of::<RAWINPUT>() + 64;

    /// 한 번에 하나만 (전역 static 을 쓰므로).
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static DOWN: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
    static CALLBACK: Mutex<Option<Box<dyn Fn() + Send + Sync>>> = Mutex::new(None);

    #[repr(align(8))]
    struct Aligned([u8; BUF_LEN]);

    fn clear_down() {
        for w in &DOWN {
            w.store(0, Ordering::Release);
        }
    }

    /// `observe` 와 같은 의미를 원자적으로 (wndproc 은 `&mut` 를 가질 수 없다).
    fn observe_atomic(vk: u8, is_break: bool) -> bool {
        let (w, mask) = (&DOWN[usize::from(vk >> 6)], 1u64 << (vk & 63));
        if is_break {
            w.fetch_and(!mask, Ordering::AcqRel);
            false
        } else {
            w.fetch_or(mask, Ordering::AcqRel) & mask == 0
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 패닉·할당·로그 금지 (release 는 panic=abort).
    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_INPUT => {
                let mut buf = Aligned([0; BUF_LEN]);
                let mut size = BUF_LEN as u32;
                let got = GetRawInputData(
                    lp as HRAWINPUT,
                    RID_INPUT,
                    buf.0.as_mut_ptr().cast::<c_void>(),
                    &mut size,
                    size_of::<RAWINPUTHEADER>() as u32,
                );
                if got != u32::MAX && got as usize >= size_of::<RAWINPUTHEADER>() {
                    let raw = &*buf.0.as_ptr().cast::<RAWINPUT>();
                    if raw.header.dwType == RIM_TYPEKEYBOARD {
                        let kb = raw.data.keyboard;
                        if kb.VKey < 255
                            && observe_atomic(kb.VKey as u8, kb.Flags & KEY_BREAK != 0)
                        {
                            if let Ok(cb) = CALLBACK.try_lock() {
                                if let Some(cb) = cb.as_ref() {
                                    cb();
                                }
                            }
                        }
                    }
                }
                // Raw Input 문서: 정리를 위해 DefWindowProc 을 반드시 호출한다.
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_INPUT_DEVICE_CHANGE => {
                clear_down();
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    struct Handle {
        thread_id: u32,
        thread: Option<JoinHandle<()>>,
        owns: bool,
    }
    impl KeyHandle for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.owns {
                return;
            }
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
            *CALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = None;
            ACTIVE.store(false, Ordering::Release);
        }
    }

    fn idle() -> Box<dyn KeyHandle> {
        Box::new(Handle { thread_id: 0, thread: None, owns: false })
    }

    /// 창을 만들고 메시지 루프를 돌린다. 준비되면 스레드 id 를 보낸다.
    fn run(ready: mpsc::Sender<Option<u32>>) {
        unsafe {
            let hinst = GetModuleHandleW(null());
            let class = wide("deskboard.keycount");
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class.as_ptr(),
            };
            if RegisterClassW(&wc) == 0 {
                let _ = ready.send(None);
                return;
            }
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                wide("").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                hinst,
                null(),
            );
            let dev = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                hwndTarget: hwnd,
            };
            if hwnd.is_null()
                || RegisterRawInputDevices(&dev, 1, size_of::<RAWINPUTDEVICE>() as u32) == 0
            {
                if !hwnd.is_null() {
                    DestroyWindow(hwnd);
                }
                UnregisterClassW(class.as_ptr(), hinst);
                let _ = ready.send(None);
                return;
            }
            let _ = ready.send(Some(GetCurrentThreadId()));

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let remove = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_REMOVE,
                hwndTarget: null_mut(),
            };
            RegisterRawInputDevices(&remove, 1, size_of::<RAWINPUTDEVICE>() as u32);
            DestroyWindow(hwnd);
            UnregisterClassW(class.as_ptr(), hinst);
        }
    }

    impl KeySource for RawInputSource {
        fn start(&mut self, on_press: Box<dyn Fn() + Send + Sync + 'static>) -> Box<dyn KeyHandle> {
            if ACTIVE
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                log::warn!("keycount: 이미 실행 중이라 두 번째 Raw Input 원천은 세지 않습니다");
                return idle();
            }
            clear_down();
            *CALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = Some(on_press);

            let (tx, rx) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("merge-keycount".into())
                .spawn(move || run(tx));
            let thread = match spawned {
                Ok(t) => t,
                Err(_) => {
                    *CALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    ACTIVE.store(false, Ordering::Release);
                    return idle();
                }
            };
            match rx.recv() {
                Ok(Some(thread_id)) => {
                    Box::new(Handle { thread_id, thread: Some(thread), owns: true })
                }
                _ => {
                    let _ = thread.join();
                    log::warn!("keycount: Raw Input 등록에 실패해 키를 세지 않습니다");
                    *CALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    ACTIVE.store(false, Ordering::Release);
                    idle()
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn observe_atomic_for_test(vk: u8, is_break: bool) -> bool {
        observe_atomic(vk, is_break)
    }
    #[cfg(test)]
    pub(super) fn clear_for_test() {
        clear_down();
    }
}

/// 테스트용 원천 — `(vk, down)` 이벤트를 `observe` 로 걸러 같은 필터를 거치게 한다.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct FakeSource {
    inner: Arc<Mutex<FakeInner>>,
}

#[cfg(test)]
#[derive(Default)]
struct FakeInner {
    down: [u64; 4],
    callback: Option<Box<dyn Fn() + Send + Sync + 'static>>,
}

#[cfg(test)]
struct FakeHandle(Arc<Mutex<FakeInner>>);

#[cfg(test)]
impl KeyHandle for FakeHandle {}

#[cfg(test)]
impl Drop for FakeHandle {
    fn drop(&mut self) {
        self.0.lock().unwrap().callback = None;
    }
}

#[cfg(test)]
impl FakeSource {
    pub fn emit(&self, vk: u8, down: bool) {
        let mut g = self.inner.lock().unwrap();
        let inner = &mut *g;
        if observe(&mut inner.down, vk, !down) {
            if let Some(cb) = inner.callback.as_ref() {
                cb();
            }
        }
    }

    pub fn device_change(&self) {
        reset(&mut self.inner.lock().unwrap().down);
    }
}

#[cfg(test)]
impl KeySource for FakeSource {
    fn start(&mut self, on_press: Box<dyn Fn() + Send + Sync + 'static>) -> Box<dyn KeyHandle> {
        let mut g = self.inner.lock().unwrap();
        g.down = [0; 4];
        g.callback = Some(on_press);
        Box::new(FakeHandle(Arc::clone(&self.inner)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Calls = Arc<Mutex<Vec<u64>>>;

    fn setup() -> (FakeSource, KeyClicker, Calls) {
        let fake = FakeSource::default();
        let calls: Calls = Arc::default();
        let sink = Arc::clone(&calls);
        let kc = KeyClicker::start_with(
            Box::new(fake.clone()),
            Box::new(move |n| sink.lock().unwrap().push(n)),
        )
        .expect("start");
        (fake, kc, calls)
    }

    fn assert_count(_: u64) {}

    #[test]
    fn press_release_counts_one() {
        let (fake, kc, _) = setup();
        fake.emit(65, true);
        fake.emit(65, false);
        assert_eq!(kc.flush(), 1);
    }

    #[test]
    fn autorepeat_counts_once() {
        let (fake, kc, _) = setup();
        for _ in 0..3 {
            fake.emit(65, true);
        }
        fake.emit(65, false);
        assert_eq!(kc.flush(), 1);
    }

    #[test]
    fn two_keys_count_two() {
        let (fake, kc, _) = setup();
        fake.emit(0x11, true);
        fake.emit(67, true);
        assert_eq!(kc.flush(), 2);
        fake.emit(67, false);
        fake.emit(0x11, false);
        assert_eq!(kc.flush(), 0);
    }

    #[test]
    fn fractional_carry_is_callers_job() {
        let (fake, kc, calls) = setup();
        for vk in [65, 66, 67] {
            fake.emit(vk, true);
        }
        kc.flush();
        for vk in [65, 66, 67] {
            fake.emit(vk, false);
        }
        fake.emit(65, true);
        kc.flush();
        assert_eq!(*calls.lock().unwrap(), vec![3, 1]);
    }

    #[test]
    fn drop_handle_stops_counting() {
        let (fake, kc, calls) = setup();
        drop(kc);
        fake.emit(65, true);
        fake.emit(66, true);
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn final_flush_on_drop() {
        let (fake, kc, calls) = setup();
        for vk in [65, 66, 67] {
            fake.emit(vk, true);
        }
        drop(kc);
        assert_eq!(*calls.lock().unwrap(), vec![3]);
    }

    #[test]
    fn observe_bitmap_semantics() {
        let mut d = [0u64; 4];
        assert!(observe(&mut d, 255, false));
        assert!(!observe(&mut d, 255, false));
        assert!(!observe(&mut d, 255, true));
        assert!(observe(&mut d, 255, false));
        assert!(observe(&mut d, 0, false));
        assert!(observe(&mut d, 64, false));
        assert!(!observe(&mut d, 64, true));
        assert!(!observe(&mut d, 64, true));
        reset(&mut d);
        assert_eq!(d, [0; 4]);
        assert!(observe(&mut d, 0, false));

        #[cfg(target_os = "windows")]
        {
            raw_input::clear_for_test();
            assert!(raw_input::observe_atomic_for_test(70, false));
            assert!(!raw_input::observe_atomic_for_test(70, false));
            assert!(!raw_input::observe_atomic_for_test(70, true));
            assert!(raw_input::observe_atomic_for_test(70, false));
            raw_input::clear_for_test();
        }
    }

    #[test]
    fn no_key_identity_leaves_keycount() {
        let seen: Calls = Arc::default();
        let sink = Arc::clone(&seen);
        let fake = FakeSource::default();
        let kc = KeyClicker::start_with(
            Box::new(fake.clone()),
            Box::new(move |n| {
                assert_count(n);
                sink.lock().unwrap().push(n);
            }),
        )
        .unwrap();
        fake.emit(0x41, true);
        fake.emit(0x42, true);
        fake.device_change();
        kc.flush();
        assert_eq!(*seen.lock().unwrap(), vec![2]);
        let dump = format!("{kc:?}").to_lowercase();
        for banned in ["vk", "scan", "code", "key:", "key_", "down"] {
            assert!(!dump.contains(banned), "{dump}");
        }
    }
}
