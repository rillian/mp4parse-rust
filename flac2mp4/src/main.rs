
use std::fs::File;
use std::io::{
    Read,
    Result,
    Error,
    ErrorKind,
};

fn is_flac<R: Read>(src: &mut R) -> Result<()> {
    let mut magic = [0u8; 4];
    try!(src.read_exact(&mut magic));
    if magic != [0x66, 0x4C, 0x61, 0x43] {
        return Err(Error::new(ErrorKind::InvalidData,
                              "File doesn't have a FLAC stream marker"));
    }
    Ok(())
}

#[derive(Debug,PartialEq)]
enum BlockType {
    StreamInfo = 0,
    Unknown,
}

impl From<u8> for BlockType {
    fn from(v: u8) -> Self {
        match v {
            0 => BlockType::StreamInfo,
            _ => BlockType::Unknown,
        }
    }
}

struct MetadataBlock {
    last: bool,
    block_type: BlockType,
    data: Vec<u8>,
}

fn read_metadata<R: Read>(src: &mut R) -> Result<MetadataBlock> {
    let mut buffer = [0u8; 4];
    try!(src.read_exact(&mut buffer));
    let length = 
        (buffer[1] as u32) << 16 |
        (buffer[2] as u32) <<  8 |
        (buffer[3] as u32);
    let mut data = Vec::with_capacity(length as usize);
    try!(src.read_exact(data.as_mut_slice()));
    Ok(MetadataBlock {
        last: (buffer[0] & 0x80) > 0,
        block_type: BlockType::from(buffer[0] & 0x7f),
        data: data,
    })
}


fn convert(filename: &str) -> Result<()> {
    let mut file = try!(File::open(filename));
    if is_flac(&mut file).is_err() {
        println!("Not a flac file: {}", filename);
        return Err(Error::new(ErrorKind::InvalidData, "Not a FLAC file"));
    }
    println!("Converting {}...", filename);
    let mut metadata = Vec::new();
    let block = try!(read_metadata(&mut file));
    assert_eq!(block.block_type, BlockType::StreamInfo,
               "Invalid: first metadata block is not streaminfo!");
    assert!(block.last, "Unhandled multiple metadata blocks!"); 
    metadata.push(block);
    Ok(())
}

fn main() {
    for filename in std::env::args().nth(1) {
        convert(&filename).unwrap();
    }
}
