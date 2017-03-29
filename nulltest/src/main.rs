/// Testcase for null function pointer checking.
///
/// Assertion fails on 1.18-nightly.

type ReadFn = extern fn(*mut u8, usize) -> isize;

struct Io {
    pub read: ReadFn,
}

unsafe extern fn validate(io: *const Io) {
    if ((*io).read as *mut std::os::raw::c_void).is_null() {
        return;
    }
    panic!("Null check failed!");
}

fn main() {
    println!("Testing null read callback... ");

    unsafe {
        let io = Io {
            read: std::mem::transmute::<*const (), ReadFn>(std::ptr::null()),
        };
        validate(&io);
    }

    println!("Ok!");
}
