extern crate mp4parse_capi;
use mp4parse_capi::mp4parse_io;
use mp4parse_capi::mp4parse_indice;
use std::collections::HashMap;

// Even though mp4parse_parser is opaque to C, rusty-cheddar won't let us
// use more than one member, so we introduce *another* wrapper.
struct Wrap {
    io: mp4parse_io,
    poisoned: bool,
    opus_header: HashMap<u32, Vec<u8>>,
    pssh_data: Vec<u8>,
    sample_table: HashMap<u32, Vec<mp4parse_indice>>,
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct mp4parse_parser(Wrap);

unsafe extern fn mp4parse_new(io: *const mp4parse_io) -> *mut mp4parse_parser {
    if io.is_null() || (*io).userdata.is_null() {
        return std::ptr::null_mut();
    }
    // is_null() isn't available on a fn type because it can't be null (in
    // Rust) by definition.  But since this value is coming from the C API,
    // it *could* be null.  Ideally, we'd wrap it in an Option to represent
    // reality, but this causes rusty-cheddar to emit the wrong type
    // (https://github.com/Sean1708/rusty-cheddar/issues/30).
    if ((*io).read as *mut std::os::raw::c_void).is_null() {
        return std::ptr::null_mut();
    }
    let parser = Box::new(mp4parse_parser(Wrap {
        io: (*io).clone(),
        poisoned: false,
        opus_header: HashMap::new(),
        pssh_data: Vec::new(),
        sample_table: HashMap::new(),
    }));

    Box::into_raw(parser)
}

type ReadFn = extern fn(*mut u8, usize, *mut std::os::raw::c_void) -> isize;

fn boom() {
    let mut dummy = 42;
    unsafe {
        let io = mp4parse_io {
            read: std::mem::transmute::<*const (), ReadFn>(std::ptr::null()),
            userdata: &mut dummy as *mut _ as *mut std::os::raw::c_void,
        };
        let parser = mp4parse_new(&io);
        assert!(parser.is_null());
    }
}

fn main() {
    println!("Testing new read callback... ");
    boom();
    println!("Ok!");
}
