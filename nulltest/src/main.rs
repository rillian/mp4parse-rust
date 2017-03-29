extern crate mp4parse_capi;
use mp4parse_capi::*;

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
