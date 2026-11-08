// Tanggung jawab: runtime pemanggilan fungsi C lewat DPI-C import (LRM §35).
// Memuat library DPI-C (`dlopen`), cari simbol (`dlsym`), dan panggil fungsi C
// dengan tipe dasar (void/int/bit/logic/byte/shortint/longint/time).
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

// Buka library DPI-C. `RTLD_NOW` = resolusi simbol segera; `RTLD_LOCAL` =
// simbol tidak tersedia untuk library yang dimuat kemudian.
const RTLD_NOW: c_int = 2;
const RTLD_LOCAL: c_int = 0;

#[link(name = "dl")]
extern "C" {
    fn dlopen(path: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, nama: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

/// Buka library DPI-C dan kembalikan handle (atau null bila gagal).
pub fn dpi_open(path: &str) -> *mut c_void {
    let c_path = match CString::new(path) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    unsafe { dlopen(c_path.as_ptr(), RTLD_NOW | RTLD_LOCAL) }
}

/// Cari simbol dalam handle library dan kembalikan pointer (atau null).
///
/// # Safety
///
/// `handle` harus valid: hasil `dpi_open` yang belum pernah ditutup lewat
/// `dpi_close`.
pub unsafe fn dpi_sym(handle: *mut c_void, nama: &str) -> *mut c_void {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let c_nama = match CString::new(nama) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    unsafe { dlsym(handle, c_nama.as_ptr()) }
}

/// Tutup library DPI-C.
///
/// # Safety
///
/// `handle` harus valid dan belum pernah ditutup sebelumnya.
pub unsafe fn dpi_close(handle: *mut c_void) {
    if !handle.is_null() {
        unsafe {
            let _ = dlclose(handle);
        }
    }
}

/// Cast pointer simbol ke fungsi `extern "C" fn() -> void` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn() -> void` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_void0(fn_ptr: *mut c_void) {
    if fn_ptr.is_null() {
        return;
    }
    let f: extern "C" fn() = std::mem::transmute(fn_ptr);
    f();
}

/// Cast pointer simbol ke fungsi `extern "C" fn(i32) -> void` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(i32) -> void` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_void_i32(fn_ptr: *mut c_void, nilai: i32) {
    if fn_ptr.is_null() {
        return;
    }
    let f: extern "C" fn(i32) = std::mem::transmute(fn_ptr);
    f(nilai);
}

/// Cast pointer simbol ke fungsi `extern "C" fn(u8) -> void` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(u8) -> void` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_void_u8(fn_ptr: *mut c_void, nilai: u8) {
    if fn_ptr.is_null() {
        return;
    }
    let f: extern "C" fn(u8) = std::mem::transmute(fn_ptr);
    f(nilai);
}

/// Cast pointer simbol ke fungsi `extern "C" fn(i16) -> void` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(i16) -> void` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_void_i16(fn_ptr: *mut c_void, nilai: i16) {
    if fn_ptr.is_null() {
        return;
    }
    let f: extern "C" fn(i16) = std::mem::transmute(fn_ptr);
    f(nilai);
}

/// Cast pointer simbol ke fungsi `extern "C" fn(i64) -> void` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(i64) -> void` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_void_i64(fn_ptr: *mut c_void, nilai: i64) {
    if fn_ptr.is_null() {
        return;
    }
    let f: extern "C" fn(i64) = std::mem::transmute(fn_ptr);
    f(nilai);
}

/// Cast pointer simbol ke fungsi `extern "C" fn() -> i32` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn() -> i32` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_i32_0(fn_ptr: *mut c_void) -> i32 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn() -> i32 = std::mem::transmute(fn_ptr);
    f()
}

/// Cast pointer simbol ke fungsi `extern "C" fn(i32) -> i32` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(i32) -> i32` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_i32_i32(fn_ptr: *mut c_void, nilai: i32) -> i32 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn(i32) -> i32 = std::mem::transmute(fn_ptr);
    f(nilai)
}

/// Cast pointer simbol ke fungsi `extern "C" fn(u8) -> i32` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(u8) -> i32` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_i32_u8(fn_ptr: *mut c_void, nilai: u8) -> i32 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn(u8) -> i32 = std::mem::transmute(fn_ptr);
    f(nilai)
}

/// Cast pointer simbol ke fungsi `extern "C" fn(i16) -> i32` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(i16) -> i32` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_i32_i16(fn_ptr: *mut c_void, nilai: i16) -> i32 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn(i16) -> i32 = std::mem::transmute(fn_ptr);
    f(nilai)
}

/// Cast pointer simbol ke fungsi `extern "C" fn(i64) -> i32` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(i64) -> i32` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_i32_i64(fn_ptr: *mut c_void, nilai: i64) -> i32 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn(i64) -> i32 = std::mem::transmute(fn_ptr);
    f(nilai)
}

/// Cast pointer simbol ke fungsi `extern "C" fn() -> u8` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn() -> u8` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_u8_0(fn_ptr: *mut c_void) -> u8 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn() -> u8 = std::mem::transmute(fn_ptr);
    f()
}

/// Cast pointer simbol ke fungsi `extern "C" fn(u8) -> u8` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(u8) -> u8` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_u8_u8(fn_ptr: *mut c_void, nilai: u8) -> u8 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn(u8) -> u8 = std::mem::transmute(fn_ptr);
    f(nilai)
}

/// Cast pointer simbol ke fungsi `extern "C" fn() -> i64` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn() -> i64` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_i64_0(fn_ptr: *mut c_void) -> i64 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn() -> i64 = std::mem::transmute(fn_ptr);
    f()
}

/// Cast pointer simbol ke fungsi `extern "C" fn(i64) -> i64` dan panggil.
///
/// # Safety
///
/// `fn_ptr` harus mengarah ke fungsi C dengan signature `fn(i64) -> i64` yang
/// valid, didapatkan lewat `dpi_sym`.
pub unsafe fn dpi_call_i64_i64(fn_ptr: *mut c_void, nilai: i64) -> i64 {
    if fn_ptr.is_null() {
        return 0;
    }
    let f: extern "C" fn(i64) -> i64 = std::mem::transmute(fn_ptr);
    f(nilai)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_open_null_bila_file_tidak_ada() {
        assert!(dpi_open("libtidak_ada_dpi.so").is_null());
    }

    #[test]
    fn dpi_sym_null_bila_handle_null() {
        assert!(unsafe { dpi_sym(std::ptr::null_mut(), "fungsi") }.is_null());
    }
}
