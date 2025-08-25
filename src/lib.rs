use once_cell::sync::Lazy;
use std::ffi::{CStr, CString};
use std::os::raw::{c_double, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

// Ensure we link against libdl for dlopen/dlsym
#[link(name = "dl")]
extern "C" {}

// Opaque libinput types we interact with
#[repr(C)]
pub struct libinput_event_pointer {
    _priv: [u8; 0],
}

// Axis constants (ABI: enum -> int)
const LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL: c_int = 0;
const LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL: c_int = 1;

// Axis source constants
const LIBINPUT_POINTER_AXIS_SOURCE_WHEEL: c_int = 0;
const LIBINPUT_POINTER_AXIS_SOURCE_FINGER: c_int = 1;
const LIBINPUT_POINTER_AXIS_SOURCE_CONTINUOUS: c_int = 2;
const LIBINPUT_POINTER_AXIS_SOURCE_WHEEL_TILT: c_int = 3;

// Function pointer types for the original libinput symbols
 type FnAxisValue = unsafe extern "C" fn(*mut libinput_event_pointer, c_int) -> c_double;
 type FnAxisSource = unsafe extern "C" fn(*mut libinput_event_pointer) -> c_int;

// Settings parsed once from environment
#[derive(Debug, Clone)]
struct Settings {
    // base
    scale: f64,                // SCROLL_SCALE (default 1.0)
    // per-axis
    scale_x: Option<f64>,      // SCROLL_SCALE_X
    scale_y: Option<f64>,      // SCROLL_SCALE_Y
    // per-source multipliers
    scale_wheel: Option<f64>,      // SCROLL_SCALE_WHEEL
    scale_finger: Option<f64>,     // SCROLL_SCALE_FINGER
    scale_continuous: Option<f64>, // SCROLL_SCALE_CONTINUOUS
}

impl Settings {
    fn from_env() -> Self {
        fn parse(name: &str) -> Option<f64> {
            std::env::var(name).ok().and_then(|v| v.parse::<f64>().ok())
        }
        let scale = parse("SCROLL_SCALE").unwrap_or(1.0);
        let scale_x = parse("SCROLL_SCALE_X");
        let scale_y = parse("SCROLL_SCALE_Y");
        let scale_wheel = parse("SCROLL_SCALE_WHEEL");
        let scale_finger = parse("SCROLL_SCALE_FINGER");
        let scale_continuous = parse("SCROLL_SCALE_CONTINUOUS");
        Settings { scale, scale_x, scale_y, scale_wheel, scale_finger, scale_continuous }
    }

    fn axis_base(&self, axis: c_int) -> f64 {
        match axis {
            LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL => self.scale_x.unwrap_or(self.scale),
            LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL => self.scale_y.unwrap_or(self.scale),
            _ => self.scale,
        }
    }

    fn source_mul(&self, source: c_int) -> f64 {
        match source {
            LIBINPUT_POINTER_AXIS_SOURCE_WHEEL => self.scale_wheel.unwrap_or(1.0),
            LIBINPUT_POINTER_AXIS_SOURCE_FINGER => self.scale_finger.unwrap_or(1.0),
            LIBINPUT_POINTER_AXIS_SOURCE_CONTINUOUS | LIBINPUT_POINTER_AXIS_SOURCE_WHEEL_TILT => {
                self.scale_continuous.unwrap_or(1.0)
            }
            _ => 1.0,
        }
    }
}

static SETTINGS: Lazy<Settings> = Lazy::new(Settings::from_env);
static DISABLE: Lazy<bool> = Lazy::new(|| matches!(std::env::var("SCROLL_DISABLE"), Ok(v) if v == "1" || v.eq_ignore_ascii_case("true")));
static DEBUG: Lazy<bool> = Lazy::new(|| matches!(std::env::var("SCROLL_DEBUG"), Ok(v) if v == "1" || v.eq_ignore_ascii_case("true")));
static LOG_ONCE: AtomicBool = AtomicBool::new(false);

// Helper: resolve a symbol via RTLD_NEXT
unsafe fn dlsym_next(name: &CStr) -> *mut c_void {
    libc::dlsym(libc::RTLD_NEXT, name.as_ptr())
}

// Helper: resolve a symbol by explicitly dlopen()'ing libinput with common SONAMEs
unsafe fn dlsym_via_soname(name: &CStr) -> *mut c_void {
    const CANDIDATES: &[&str] = &[
        "libinput.so.10",
        "libinput.so.9",
        "libinput.so.8",
        "libinput.so",
    ];
    for soname in CANDIDATES {
        if let Ok(csoname) = CString::new(*soname) {
            let handle = libc::dlopen(csoname.as_ptr(), libc::RTLD_LAZY);
            if !handle.is_null() {
                let sym = libc::dlsym(handle, name.as_ptr());
                if !sym.is_null() {
                    return sym;
                }
            }
        }
    }
    ptr::null_mut()
}

unsafe fn resolve_symbol(symbol: &CStr) -> *mut c_void {
    let mut p = dlsym_next(symbol);
    if p.is_null() {
        p = dlsym_via_soname(symbol);
    }
    p
}

static ORIG_AXIS_VALUE: Lazy<FnAxisValue> = Lazy::new(|| unsafe {
    let name = CStr::from_bytes_with_nul_unchecked(b"libinput_event_pointer_get_axis_value\0");
    let p = resolve_symbol(name);
    if p.is_null() {
        // Hard failure: we cannot safely continue without the real function.
        eprintln!("[libinput_scroll_shim] FATAL: could not resolve original libinput_event_pointer_get_axis_value");
        std::process::abort();
    }
    std::mem::transmute::<*mut c_void, FnAxisValue>(p)
});

// Optional new-style symbol present in some libinput versions
static ORIG_AXIS_VALUE_V120: Lazy<Option<FnAxisValue>> = Lazy::new(|| unsafe {
    let name = CStr::from_bytes_with_nul_unchecked(b"libinput_event_pointer_get_axis_value_v120\0");
    let p = resolve_symbol(name);
    if p.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, FnAxisValue>(p)) }
});

static ORIG_AXIS_SOURCE: Lazy<Option<FnAxisSource>> = Lazy::new(|| unsafe {
    let name = CStr::from_bytes_with_nul_unchecked(b"libinput_event_pointer_get_axis_source\0");
    let p = resolve_symbol(name);
    if p.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, FnAxisSource>(p)) }
});

// Some libinput versions expose separate scroll_value helpers – hook them too if present.
static ORIG_SCROLL_VALUE: Lazy<Option<FnAxisValue>> = Lazy::new(|| unsafe {
    let name = CStr::from_bytes_with_nul_unchecked(b"libinput_event_pointer_get_scroll_value\0");
    let p = resolve_symbol(name);
    if p.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, FnAxisValue>(p)) }
});

static ORIG_SCROLL_VALUE_V120: Lazy<Option<FnAxisValue>> = Lazy::new(|| unsafe {
    let name = CStr::from_bytes_with_nul_unchecked(b"libinput_event_pointer_get_scroll_value_v120\0");
    let p = resolve_symbol(name);
    if p.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, FnAxisValue>(p)) }
});

fn log_startup_once() {
    if *DEBUG && !LOG_ONCE.swap(true, Ordering::SeqCst) {
        eprintln!(
            "[libinput_scroll_shim] loaded. disable={} settings={:?}",
            *DISABLE, *SETTINGS
        );
    }
}

#[inline]
fn compute_scale(axis: c_int, source: Option<c_int>) -> f64 {
    let base = SETTINGS.axis_base(axis);
    let mul = source.map(|s| SETTINGS.source_mul(s)).unwrap_or(1.0);
    base * mul
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_axis_value(
    pe: *mut libinput_event_pointer,
    axis: c_int,
) -> c_double {
    log_startup_once();

    // Always call the real function first to obtain the original value
    let orig_v = (ORIG_AXIS_VALUE)(pe, axis);

    if *DISABLE {
        return orig_v;
    }

    // Only scale scroll axes
    if axis != LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL && axis != LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL {
        return orig_v;
    }

    let source = (*ORIG_AXIS_SOURCE).map(|f| f(pe));
    let scale = compute_scale(axis, source);

    if (scale - 1.0).abs() < 1e-9 {
        return orig_v;
    }

    let scaled = orig_v * scale as c_double;

    if *DEBUG {
        if let Some(src) = source {
            eprintln!("[libinput_scroll_shim] axis={} src={} val={:.6} -> {:.6} (scale={:.3})", axis, src, orig_v, scaled, scale);
        } else {
            eprintln!("[libinput_scroll_shim] axis={} val={:.6} -> {:.6} (scale={:.3})", axis, orig_v, scaled, scale);
        }
    }

    scaled
}

// Hook variant with version suffix, if the caller uses it. Keep behavior identical.
#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_axis_value_v120(
    pe: *mut libinput_event_pointer,
    axis: c_int,
) -> c_double {
    log_startup_once();

    // Prefer calling the matching original symbol if available, else fall back
    let orig_v = if let Some(f) = *ORIG_AXIS_VALUE_V120 { f(pe, axis) } else { (ORIG_AXIS_VALUE)(pe, axis) };

    if *DISABLE {
        return orig_v;
    }

    if axis != LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL && axis != LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL {
        return orig_v;
    }

    let source = (*ORIG_AXIS_SOURCE).map(|f| f(pe));
    let scale = compute_scale(axis, source);

    if (scale - 1.0).abs() < 1e-9 { return orig_v; }

    let scaled = orig_v * scale as c_double;

    if *DEBUG {
        if let Some(src) = source {
            eprintln!("[libinput_scroll_shim] v120 axis={} src={} val={:.6} -> {:.6} (scale={:.3})", axis, src, orig_v, scaled, scale);
        } else {
            eprintln!("[libinput_scroll_shim] v120 axis={} val={:.6} -> {:.6} (scale={:.3})", axis, orig_v, scaled, scale);
        }
    }

    scaled
}

// Also hook scroll_value helpers if consumers use these instead of axis_value
#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_scroll_value(
    pe: *mut libinput_event_pointer,
    axis: c_int,
) -> c_double {
    log_startup_once();

    // Prefer the matching original symbol if present; fall back to axis_value
    let orig_v = if let Some(f) = *ORIG_SCROLL_VALUE { f(pe, axis) } else { (ORIG_AXIS_VALUE)(pe, axis) };

    if *DISABLE { return orig_v; }
    if axis != LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL && axis != LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL { return orig_v; }

    let source = (*ORIG_AXIS_SOURCE).map(|f| f(pe));
    let scale = compute_scale(axis, source);
    if (scale - 1.0).abs() < 1e-9 { return orig_v; }

    let scaled = orig_v * scale as c_double;

    if *DEBUG {
        if let Some(src) = source {
            eprintln!("[libinput_scroll_shim] scroll axis={} src={} val={:.6} -> {:.6} (scale={:.3})", axis, src, orig_v, scaled, scale);
        } else {
            eprintln!("[libinput_scroll_shim] scroll axis={} val={:.6} -> {:.6} (scale={:.3})", axis, orig_v, scaled, scale);
        }
    }

    scaled
}

#[no_mangle]
pub unsafe extern "C" fn libinput_event_pointer_get_scroll_value_v120(
    pe: *mut libinput_event_pointer,
    axis: c_int,
) -> c_double {
    log_startup_once();

    // Try v120, then plain scroll_value, else axis_value
    let orig_v = if let Some(f) = *ORIG_SCROLL_VALUE_V120 {
        f(pe, axis)
    } else if let Some(f) = *ORIG_SCROLL_VALUE {
        f(pe, axis)
    } else {
        (ORIG_AXIS_VALUE)(pe, axis)
    };

    if *DISABLE { return orig_v; }
    if axis != LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL && axis != LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL { return orig_v; }

    let source = (*ORIG_AXIS_SOURCE).map(|f| f(pe));
    let scale = compute_scale(axis, source);
    if (scale - 1.0).abs() < 1e-9 { return orig_v; }

    let scaled = orig_v * scale as c_double;

    if *DEBUG {
        if let Some(src) = source {
            eprintln!("[libinput_scroll_shim] scroll_v120 axis={} src={} val={:.6} -> {:.6} (scale={:.3})", axis, src, orig_v, scaled, scale);
        } else {
            eprintln!("[libinput_scroll_shim] scroll_v120 axis={} val={:.6} -> {:.6} (scale={:.3})", axis, orig_v, scaled, scale);
        }
    }

    scaled
}
