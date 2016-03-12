//! C API for mp4parse module.
//!
//! Parses ISO Base Media Format aka video/mp4 streams.
//!
//! # Examples
//!
//! ```rust
//! extern crate mp4parse;
//! use std::io::Read;
//!
//! extern fn buf_read(buf: *mut u8, size: usize, userdata: *mut std::os::raw::c_void) -> isize {
//!    let mut input: &mut std::fs::File = unsafe { &mut *(userdata as *mut _) };
//!    let mut buf = unsafe { std::slice::from_raw_parts_mut(buf, size) };
//!    match input.read(&mut buf) {
//!        Ok(n) => n as isize,
//!        Err(_) => -1,
//!    }
//! }
//!
//! let mut file = std::fs::File::open("examples/minimal.mp4").unwrap();
//! let io = mp4parse::mp4parse_io { read: buf_read,
//!                                  userdata: &mut file as *mut _ as *mut std::os::raw::c_void };
//! unsafe {
//!     let parser = mp4parse::mp4parse_new(&io);
//!     let rv = mp4parse::mp4parse_read(parser);
//!     assert_eq!(rv, mp4parse::mp4parse_error::MP4PARSE_OK);
//!     mp4parse::mp4parse_free(parser);
//! }
//! ```

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std;
use std::io::Read;

// Symbols we need from our rust api.
use MediaContext;
use TrackType;
use read_mp4;
use Error;
use media_time_to_ms;
use track_time_to_ms;
use SampleEntry;

// rusty-cheddar's C enum generation doesn't namespace enum members by
// prefixing them, so we're forced to do it in our member names until
// https://github.com/Sean1708/rusty-cheddar/pull/35 is fixed.  Importing
// the members into the module namespace avoids doubling up on the
// namespacing on the Rust side.
use mp4parse_error::*;
use mp4parse_track_type::*;

#[repr(C)]
#[derive(PartialEq, Debug)]
pub enum mp4parse_error {
    MP4PARSE_OK = 0,
    MP4PARSE_ERROR_BADARG = 1,
    MP4PARSE_ERROR_INVALID = 2,
    MP4PARSE_ERROR_UNSUPPORTED = 3,
    MP4PARSE_ERROR_EOF = 4,
    MP4PARSE_ERROR_ASSERT = 5,
    MP4PARSE_ERROR_IO = 6,
}

#[repr(C)]
#[derive(PartialEq, Debug)]
pub enum mp4parse_track_type {
    MP4PARSE_TRACK_TYPE_VIDEO = 0,
    MP4PARSE_TRACK_TYPE_AUDIO = 1,
}

#[repr(C)]
pub struct mp4parse_track_info {
    pub track_type: mp4parse_track_type,
    pub track_id: u32,
    pub duration: u64,
    pub media_time: i64, // wants to be u64? understand how elst adjustment works
    // TODO(kinetik): include crypto guff
}

#[repr(C)]
pub struct mp4parse_track_audio_info {
    pub channels: u16,
    pub bit_depth: u16,
    pub sample_rate: u32,
    // TODO(kinetik):
    // int32_t profile;
    // int32_t extended_profile; // check types
    // extra_data
    // codec_specific_config
}

#[repr(C)]
pub struct mp4parse_track_video_info {
    pub display_width: u32,
    pub display_height: u32,
    pub image_width: u16,
    pub image_height: u16,
    // TODO(kinetik):
    // extra_data
    // codec_specific_config
}

// Even though mp4parse_parser is opaque to C, rusty-cheddar won't let us
// use more than one member, so we introduce *another* wrapper.
struct Wrap {
    context: MediaContext,
    io: mp4parse_io,
    poisoned: bool,
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct mp4parse_parser(Wrap);

impl mp4parse_parser {
    fn context(&self) -> &MediaContext {
        &self.0.context
    }

    fn context_mut(&mut self) -> &mut MediaContext {
        &mut self.0.context
    }

    fn io_mut(&mut self) -> &mut mp4parse_io {
        &mut self.0.io
    }

    fn poisoned(&self) -> bool {
        self.0.poisoned
    }

    fn set_poisoned(&mut self, poisoned: bool) {
        self.0.poisoned = poisoned;
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct mp4parse_io {
    pub read: extern fn(buffer: *mut u8, size: usize, userdata: *mut std::os::raw::c_void) -> isize,
    pub userdata: *mut std::os::raw::c_void,
}

// Required for the panic-catching thread in mp4parse_read because raw
// pointers don't impl Send by default.  This is *only* safe because we wait
// on the panic-catching thread to complete before returning from
// mp4parse_read and there's no concurrent access.
unsafe impl Send for mp4parse_io {}

impl Read for mp4parse_io {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.len() > isize::max_value() as usize {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "buf length overflow in mp4parse_io Read impl"));
        }
        let rv = (self.read)(buf.as_mut_ptr(), buf.len(), self.userdata);
        if rv >= 0 {
            Ok(rv as usize)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "I/O error in mp4parse_io Read impl"))
        }
    }
}

// C API wrapper functions.

/// Allocate an `mp4parse_parser*` to read from the supplied `mp4parse_io`.
#[no_mangle]
pub unsafe extern fn mp4parse_new(io: *const mp4parse_io) -> *mut mp4parse_parser {
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
    let parser = Box::new(mp4parse_parser(Wrap { context: MediaContext::new(), io: (*io).clone(), poisoned: false }));
    Box::into_raw(parser)
}

/// Free an `mp4parse_parser*` allocated by `mp4parse_new()`.
#[no_mangle]
pub unsafe extern fn mp4parse_free(parser: *mut mp4parse_parser) {
    assert!(!parser.is_null());
    let _ = Box::from_raw(parser);
}

/// Run the `mp4parse_parser*` allocated by `mp4parse_new()` until EOF or error.
#[no_mangle]
pub unsafe extern fn mp4parse_read(parser: *mut mp4parse_parser) -> mp4parse_error {
    // Validate arguments from C.
    if parser.is_null() || (*parser).poisoned() {
        return MP4PARSE_ERROR_BADARG;
    }

    let mut context = (*parser).context_mut();
    let mut io = (*parser).io_mut();

    let r = if cfg!(not(feature = "fuzz")) {
        // Parse in a subthread to catch any panics.
        let task = std::thread::spawn(move || read_mp4(io, context));
        // The task's JoinHandle will return an error result if the thread
        // panicked, and will wrap the closure's return'd result in an
        // Ok(..) otherwise, meaning we could see Ok(Err(Error::..))
        // here. So map thread failures back to an
        // mp4parse::Error::AssertCaught.
        task.join().unwrap_or_else(|_| Err(Error::AssertCaught))
    } else {
        read_mp4(io, context)
    };
    (*parser).set_poisoned(r.is_err());
    match r {
        Ok(_) => MP4PARSE_OK,
        Err(Error::NoMoov) | Err(Error::InvalidData(_)) => MP4PARSE_ERROR_INVALID,
        Err(Error::Unsupported(_)) => MP4PARSE_ERROR_UNSUPPORTED,
        Err(Error::AssertCaught) => MP4PARSE_ERROR_ASSERT,
        Err(Error::Io(UnexpectedEOF)) => MP4PARSE_ERROR_EOF,
        Err(Error::Io(e)) => MP4PARSE_ERROR_IO,
    }
}

/// Return the number of tracks parsed by previous `mp4parse_read()` call.
#[no_mangle]
pub unsafe extern fn mp4parse_get_track_count(parser: *const mp4parse_parser) -> u32 {
    // Validate argument from C.
    assert!(!parser.is_null() && !(*parser).poisoned());
    let context = (*parser).context();

    // Make sure the track count fits in a u32.
    assert!(context.tracks.len() < u32::max_value() as usize);
    context.tracks.len() as u32
}

/// Fill the supplied `mp4parse_track_info` with metadata for `track`.
#[no_mangle]
pub unsafe extern fn mp4parse_get_track_info(parser: *mut mp4parse_parser, track: u32, info: *mut mp4parse_track_info) -> mp4parse_error {
    if parser.is_null() || info.is_null() || (*parser).poisoned() {
        return MP4PARSE_ERROR_BADARG;
    }

    let context = (*parser).context_mut();
    let track_index: usize = track as usize;
    let info: &mut mp4parse_track_info = &mut *info;

    if track_index >= context.tracks.len() {
        return MP4PARSE_ERROR_BADARG;
    }

    info.track_type = match context.tracks[track_index].track_type {
        TrackType::Video => MP4PARSE_TRACK_TYPE_VIDEO,
        TrackType::Audio => MP4PARSE_TRACK_TYPE_AUDIO,
        TrackType::Unknown => return MP4PARSE_ERROR_UNSUPPORTED,
    };

    // Maybe context & track should just have a single simple is_valid() instead?
    if context.timescale.is_none() ||
       context.tracks[track_index].timescale.is_none() ||
       context.tracks[track_index].duration.is_none() ||
       context.tracks[track_index].track_id.is_none() {
        return MP4PARSE_ERROR_INVALID;
    }

    let track = &context.tracks[track_index];
    info.media_time = track.media_time.map_or(0, |media_time| {
        track_time_to_ms(media_time, track.timescale.unwrap()) as i64
    }) - track.empty_duration.map_or(0, |empty_duration| {
        media_time_to_ms(empty_duration, context.timescale.unwrap()) as i64
    });
    info.duration = track_time_to_ms(track.duration.unwrap(), track.timescale.unwrap());
    info.track_id = track.track_id.unwrap();

    MP4PARSE_OK
}

/// Fill the supplied `mp4parse_track_audio_info` with metadata for `track`.
#[no_mangle]
pub unsafe extern fn mp4parse_get_track_audio_info(parser: *mut mp4parse_parser, track: u32, info: *mut mp4parse_track_audio_info) -> mp4parse_error {
    if parser.is_null() || info.is_null() || (*parser).poisoned() {
        return MP4PARSE_ERROR_BADARG;
    }

    let context = (*parser).context_mut();

    if track as usize >= context.tracks.len() {
        return MP4PARSE_ERROR_BADARG;
    }

    let track = &context.tracks[track as usize];

    match track.track_type {
        TrackType::Audio => {}
        _ => return MP4PARSE_ERROR_INVALID,
    };

    let audio = match track.data {
        Some(ref data) => data,
        None => return MP4PARSE_ERROR_INVALID,
    };

    let audio = match *audio {
        SampleEntry::Audio(ref x) => x,
        _ => return MP4PARSE_ERROR_INVALID,
    };

    (*info).channels = audio.channelcount;
    (*info).bit_depth = audio.samplesize;
    (*info).sample_rate = audio.samplerate >> 16; // 16.16 fixed point

    MP4PARSE_OK
}

/// Fill the supplied `mp4parse_track_video_info` with metadata for `track`.
#[no_mangle]
pub unsafe extern fn mp4parse_get_track_video_info(parser: *mut mp4parse_parser, track: u32, info: *mut mp4parse_track_video_info) -> mp4parse_error {
    if parser.is_null() || info.is_null() || (*parser).poisoned() {
        return MP4PARSE_ERROR_BADARG;
    }

    let context = (*parser).context_mut();

    if track as usize >= context.tracks.len() {
        return MP4PARSE_ERROR_BADARG;
    }

    let track = &context.tracks[track as usize];

    match track.track_type {
        TrackType::Video => {}
        _ => return MP4PARSE_ERROR_INVALID,
    };

    let video = match track.data {
        Some(ref data) => data,
        None => return MP4PARSE_ERROR_INVALID,
    };

    let video = match *video {
        SampleEntry::Video(ref x) => x,
        _ => return MP4PARSE_ERROR_INVALID,
    };

    if let Some(ref tkhd) = track.tkhd {
        (*info).display_width = tkhd.width >> 16; // 16.16 fixed point
        (*info).display_height = tkhd.height >> 16; // 16.16 fixed point
    } else {
        return MP4PARSE_ERROR_INVALID;
    }
    (*info).image_width = video.width;
    (*info).image_height = video.height;

    MP4PARSE_OK
}

#[cfg(test)]
extern fn panic_read(_: *mut u8, _: usize, _: *mut std::os::raw::c_void) -> isize {
    panic!("panic_read shouldn't be called in these tests");
}

#[cfg(test)]
extern fn error_read(_: *mut u8, _: usize, _: *mut std::os::raw::c_void) -> isize {
    -1
}

#[cfg(test)]
extern fn valid_read(buf: *mut u8, size: usize, userdata: *mut std::os::raw::c_void) -> isize {
    let mut input: &mut std::fs::File = unsafe { &mut *(userdata as *mut _) };

    let mut buf = unsafe { std::slice::from_raw_parts_mut(buf, size) };
    match input.read(&mut buf) {
        Ok(n) => n as isize,
        Err(_) => -1,
    }
}

#[test]
fn new_parser() {
    let mut dummy_value: u32 = 42;
    let io = mp4parse_io { read: panic_read,
                           userdata: &mut dummy_value as *mut _ as *mut std::os::raw::c_void };
    unsafe {
        let parser = mp4parse_new(&io);
        assert!(!parser.is_null());
        mp4parse_free(parser);
    }
}

#[test]
#[should_panic(expected = "assertion failed")]
fn free_null_parser() {
    unsafe {
        mp4parse_free(std::ptr::null_mut());
    }
}

#[test]
#[should_panic(expected = "assertion failed")]
fn get_track_count_null_parser() {
    unsafe {
        let _ = mp4parse_get_track_count(std::ptr::null());
    }
}

#[test]
fn arg_validation() {
    unsafe {
        // Passing a null mp4parse_io is an error.
        let parser = mp4parse_new(std::ptr::null());
        assert!(parser.is_null());

        let null_mut: *mut std::os::raw::c_void = std::ptr::null_mut();

        // Passing an mp4parse_io with null members is an error.
        let io = mp4parse_io { read: std::mem::transmute(null_mut),
                               userdata: null_mut };
        let parser = mp4parse_new(&io);
        assert!(parser.is_null());

        let io = mp4parse_io { read: panic_read,
                               userdata: null_mut };
        let parser = mp4parse_new(&io);
        assert!(parser.is_null());

        let mut dummy_value = 42;
        let io = mp4parse_io { read: std::mem::transmute(null_mut),
                               userdata: &mut dummy_value as *mut _ as *mut std::os::raw::c_void };
        let parser = mp4parse_new(&io);
        assert!(parser.is_null());

        // Passing a null mp4parse_parser is an error.
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_read(std::ptr::null_mut()));

        let mut dummy_info = mp4parse_track_info { track_type: MP4PARSE_TRACK_TYPE_VIDEO,
                                                   track_id: 0,
                                                   duration: 0,
                                                   media_time: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_info(std::ptr::null_mut(), 0, &mut dummy_info));

        let mut dummy_video = mp4parse_track_video_info { display_width: 0,
                                                          display_height: 0,
                                                          image_width: 0,
                                                          image_height: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_video_info(std::ptr::null_mut(), 0, &mut dummy_video));

        let mut dummy_audio = mp4parse_track_audio_info { channels: 0,
                                                          bit_depth: 0,
                                                          sample_rate: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_audio_info(std::ptr::null_mut(), 0, &mut dummy_audio));
    }
}

#[test]
fn arg_validation_with_parser() {
    unsafe {
        let mut dummy_value = 42;
        let io = mp4parse_io { read: error_read,
                               userdata: &mut dummy_value as *mut _ as *mut std::os::raw::c_void };
        let parser = mp4parse_new(&io);
        assert!(!parser.is_null());

        // Our mp4parse_io read should simply fail with an error.
        assert_eq!(MP4PARSE_ERROR_IO, mp4parse_read(parser));

        // The parser is now poisoned and unusable.
        assert_eq!(MP4PARSE_ERROR_BADARG,  mp4parse_read(parser));

        // Null info pointers are an error.
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_info(parser, 0, std::ptr::null_mut()));
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_video_info(parser, 0, std::ptr::null_mut()));
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_audio_info(parser, 0, std::ptr::null_mut()));

        let mut dummy_info = mp4parse_track_info { track_type: MP4PARSE_TRACK_TYPE_VIDEO,
                                                   track_id: 0,
                                                   duration: 0,
                                                   media_time: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_info(parser, 0, &mut dummy_info));

        let mut dummy_video = mp4parse_track_video_info { display_width: 0,
                                                          display_height: 0,
                                                          image_width: 0,
                                                          image_height: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_video_info(parser, 0, &mut dummy_video));

        let mut dummy_audio = mp4parse_track_audio_info { channels: 0,
                                                          bit_depth: 0,
                                                          sample_rate: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_audio_info(parser, 0, &mut dummy_audio));

        mp4parse_free(parser);
    }
}

#[test]
#[should_panic(expected = "assertion failed")]
fn get_track_count_poisoned_parser() {
    unsafe {
        let mut dummy_value = 42;
        let io = mp4parse_io { read: error_read,
                               userdata: &mut dummy_value as *mut _ as *mut std::os::raw::c_void };
        let parser = mp4parse_new(&io);
        assert!(!parser.is_null());

        // Our mp4parse_io read should simply fail with an error.
        assert_eq!(MP4PARSE_ERROR_IO, mp4parse_read(parser));

        let _ = mp4parse_get_track_count(parser);
    }
}

#[test]
fn arg_validation_with_data() {
    unsafe {
        let mut file = std::fs::File::open("examples/minimal.mp4").unwrap();
        let io = mp4parse_io { read: valid_read,
                               userdata: &mut file as *mut _ as *mut std::os::raw::c_void };
        let parser = mp4parse_new(&io);
        assert!(!parser.is_null());

        assert_eq!(MP4PARSE_OK, mp4parse_read(parser));

        assert_eq!(2, mp4parse_get_track_count(parser));

        let mut info = mp4parse_track_info { track_type: MP4PARSE_TRACK_TYPE_VIDEO,
                                             track_id: 0,
                                             duration: 0,
                                             media_time: 0 };
        assert_eq!(MP4PARSE_OK, mp4parse_get_track_info(parser, 0, &mut info));
        assert_eq!(info.track_type, MP4PARSE_TRACK_TYPE_VIDEO);
        assert_eq!(info.track_id, 1);
        assert_eq!(info.duration, 40000);
        assert_eq!(info.media_time, 0);

        assert_eq!(MP4PARSE_OK, mp4parse_get_track_info(parser, 1, &mut info));
        assert_eq!(info.track_type, MP4PARSE_TRACK_TYPE_AUDIO);
        assert_eq!(info.track_id, 2);
        assert_eq!(info.duration, 61333);
        assert_eq!(info.media_time, 21333);

        let mut video = mp4parse_track_video_info { display_width: 0,
                                                    display_height: 0,
                                                    image_width: 0,
                                                    image_height: 0 };
        assert_eq!(MP4PARSE_OK, mp4parse_get_track_video_info(parser, 0, &mut video));
        assert_eq!(video.display_width, 320);
        assert_eq!(video.display_height, 240);
        assert_eq!(video.image_width, 320);
        assert_eq!(video.image_height, 240);

        let mut audio = mp4parse_track_audio_info { channels: 0,
                                                    bit_depth: 0,
                                                    sample_rate: 0 };
        assert_eq!(MP4PARSE_OK, mp4parse_get_track_audio_info(parser, 1, &mut audio));
        assert_eq!(audio.channels, 2);
        assert_eq!(audio.bit_depth, 16);
        assert_eq!(audio.sample_rate, 48000);

        // Test with an invalid track number.
        let mut info = mp4parse_track_info { track_type: MP4PARSE_TRACK_TYPE_VIDEO,
                                             track_id: 0,
                                             duration: 0,
                                             media_time: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_info(parser, 3, &mut info));
        assert_eq!(info.track_type, MP4PARSE_TRACK_TYPE_VIDEO);
        assert_eq!(info.track_id, 0);
        assert_eq!(info.duration, 0);
        assert_eq!(info.media_time, 0);

        let mut video = mp4parse_track_video_info { display_width: 0,
                                                    display_height: 0,
                                                    image_width: 0,
                                                    image_height: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_video_info(parser, 3, &mut video));
        assert_eq!(video.display_width, 0);
        assert_eq!(video.display_height, 0);
        assert_eq!(video.image_width, 0);
        assert_eq!(video.image_height, 0);

        let mut audio = mp4parse_track_audio_info { channels: 0,
                                                    bit_depth: 0,
                                                    sample_rate: 0 };
        assert_eq!(MP4PARSE_ERROR_BADARG, mp4parse_get_track_audio_info(parser, 3, &mut audio));
        assert_eq!(audio.channels, 0);
        assert_eq!(audio.bit_depth, 0);
        assert_eq!(audio.sample_rate, 0);

        mp4parse_free(parser);
    }
}
