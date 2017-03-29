/// Testcase for null function pointer checking.
///
/// Assertion fails on 1.18-nightly.

type ReadFn = extern fn(*mut u8, usize, *mut std::os::raw::c_void) -> isize;

#[repr(C)]
#[derive(Clone)]
pub struct mp4parse_io {
    pub read: ReadFn,
    pub userdata: *mut std::os::raw::c_void,
}

// Even though mp4parse_parser is opaque to C, rusty-cheddar won't let us
// use more than one member, so we introduce *another* wrapper.
#[repr(C)]
struct Wrap {
    io: mp4parse_io,
}

unsafe extern fn mp4parse_new(io: *const mp4parse_io) -> *mut Wrap {
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
    let parser = Box::new(Wrap {
        io: (*io).clone(),
    });

    Box::into_raw(parser)
}

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
    println!("Testing null read callback... ");
    boom();
    println!("Ok!");
}
