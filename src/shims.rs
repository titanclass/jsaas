/// When linking with OpenSSL, it seems to want secure_getenv to be
/// defined. It doesn't appear to be on ARM.
///
/// Seemingly related:
/// https://github.com/alexcrichton/openssl-src-rs/blob/master/src/lib.rs#L253-L256
#[cfg(target_arch = "arm")]
#[no_mangle]
extern "C" fn secure_getenv(_: *mut std::os::raw::c_char) -> *mut std::os::raw::c_char {
    std::ptr::null_mut()
}
