// Tanggung jawab: program Rust mandiri yang memuat libdpi.so dan memanggil
// fungsi C lewat dlopen/dlsym (DPI-C import, LRM §35). Tidak butuh crate
// eksternal — memverifikasi pemanggilan C nyata dari Rust.
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

const RTLD_NOW: c_int = 2;
const RTLD_LOCAL: c_int = 0;

#[link(name = "dl")]
extern "C" {
    fn dlopen(path: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, nama: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

fn dpi_open(path: &str) -> *mut c_void {
    let c_path = CString::new(path).unwrap();
    unsafe { dlopen(c_path.as_ptr(), RTLD_NOW | RTLD_LOCAL) }
}

fn dpi_sym(handle: *mut c_void, nama: &str) -> *mut c_void {
    let c_nama = CString::new(nama).unwrap();
    unsafe { dlsym(handle, c_nama.as_ptr()) }
}

unsafe fn dpi_call_void_i32(fn_ptr: *mut c_void, nilai: i32) {
    let f: extern "C" fn(i32) = std::mem::transmute(fn_ptr);
    f(nilai);
}

unsafe fn dpi_call_i32_u8(fn_ptr: *mut c_void, nilai: u8) -> i32 {
    let f: extern "C" fn(u8) -> i32 = std::mem::transmute(fn_ptr);
    f(nilai)
}

fn main() {
    let handle = dpi_open("./libdpi.so");
    assert!(!handle.is_null(), "libdpi.so tidak dapat dibuka");

    let set_data = dpi_sym(handle, "set_data");
    assert!(!set_data.is_null(), "simbol set_data tidak ditemukan");
    let hitung_paritas = dpi_sym(handle, "hitung_paritas");
    assert!(!hitung_paritas.is_null(), "simbol hitung_paritas tidak ditemukan");

    unsafe { dpi_call_void_i32(set_data, 42) };

    let hasil = unsafe { dpi_call_i32_u8(hitung_paritas, 0xFF) };
    println!("paritas(0xFF) = {}", hasil);
    assert_eq!(hasil, 0, "paritas(0xFF) harusnya 0");

    let hasil = unsafe { dpi_call_i32_u8(hitung_paritas, 0x01) };
    println!("paritas(0x01) = {}", hasil);
    assert_eq!(hasil, 1, "paritas(0x01) harusnya 1");

    unsafe { dlclose(handle) };

    println!("DPI-C import pemanggilan C nyata: OK");
}
