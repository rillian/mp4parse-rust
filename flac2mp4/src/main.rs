use std::io::{
    Read,
    Result,
    Error,
    ErrorKind,
};
use std::fs::File;

fn is_flac<R: Read>(src: &mut R) -> Result<()> {
    let mut magic = [0u8; 4];
    try!(src.read_exact(&mut magic));
    if magic != [0x66, 0x4C, 0x61, 0x43] {
        return Err(Error::new(ErrorKind::InvalidData,
                              "File doesn't have a FLAC stream marker"));
    }
    Ok(())
}

fn convert(filename: &str) -> Result<()> {
    let mut file = try!(File::open(filename));
    if is_flac(&mut file).is_err() {
        println!("Not a flac file: {}", filename);
        return Err(Error::new(ErrorKind::InvalidData, "Not a FLAC file"));
    }
    println!("Converting {}...", filename);
    // TODO: actual conversion.
    Ok(())
}

fn main() {
    for filename in std::env::args().nth(1) {
        convert(&filename).unwrap();
    }
}
