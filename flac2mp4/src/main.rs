extern crate byteorder;
use byteorder::{
    ByteOrder,
    BigEndian,
    ReadBytesExt,
};

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
    Padding = 1,
    Application = 2,
    SeekTable = 3,
    VorbisComment = 4,
    Cuesheet = 5,
    Picture = 6,
    Reserved = 7,
    Unknown,
    Invalid = 127,
}

impl From<u8> for BlockType {
    fn from(v: u8) -> Self {
        match v {
            0 => BlockType::StreamInfo,
            1 => BlockType::Padding,
            2 => BlockType::Application,
            3 => BlockType::SeekTable,
            4 => BlockType::VorbisComment,
            5 => BlockType::Cuesheet,
            6 => BlockType::Picture,
            7...126 => BlockType::Reserved,
            127 => BlockType::Invalid,
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
    let length = BigEndian::read_uint(&buffer[1..4], 3) as u32;
    let mut data = vec![0; length as usize];
    try!(src.read_exact(data.as_mut_slice()));
    Ok(MetadataBlock {
        last: (buffer[0] & 0x80) > 0,
        block_type: BlockType::from(buffer[0] & 0x7f),
        data: data,
    })
}

#[derive(Debug)]
struct StreamInfo {
    block_min: u16,
    block_max: u16,
    frame_min: u32,
    frame_max: u32,
    sample_rate: u32,
    channel_count: u8,
    bit_depth: u8,
    total_samples: u64,
    md5: [u8; 16],
}

fn parse_stream_info<R: ReadBytesExt>(src: &mut R) -> Result<StreamInfo> {
    let block_min = try!(src.read_u16::<BigEndian>());
    let block_max = try!(src.read_u16::<BigEndian>());
    let frame_min = try!(src.read_uint::<BigEndian>(3)) as u32;
    let frame_max = try!(src.read_uint::<BigEndian>(3)) as u32;
    let mut buffer = [0u8; 8];
    try!(src.read_exact(&mut buffer));
    let sample_rate =
        (buffer[0] as u32) << 12 |
        (buffer[1] as u32) <<  4 |
        ((buffer[2] & 0xf0) as u32);
    if sample_rate == 0 || sample_rate > 655350 {
        return Err(Error::new(ErrorKind::InvalidData,
                              "StreamInfo sample rate invalid!"));
    }
    let channel_count = (buffer[2] & 0x0e) >> 1;
    if channel_count == 0 {
        return Err(Error::new(ErrorKind::InvalidData,
                              "StreamInfo channel count invalid!"));
    }
    let bit_depth = ((buffer[2] & 0x01) << 4 | (buffer[3] & 0xf0) >> 4) + 1;
    let total_samples =
        ((buffer[3] & 0x0f) as u64) << 32 |
        (buffer[4] as u64) << 24 |
        (buffer[5] as u64) << 16 |
        (buffer[6] as u64) << 8 |
        (buffer[7] as u64);
    let mut md5 = [0u8; 16];
    try!(src.read_exact(&mut md5));
    Ok(StreamInfo {
        block_min: block_min,
        block_max: block_max,
        frame_min: frame_min,
        frame_max: frame_max,
        sample_rate: sample_rate,
        channel_count: channel_count,
        bit_depth: bit_depth,
        total_samples: total_samples,
        md5: md5,
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
    loop {
        let block = try!(read_metadata(&mut file));
        println!("  {:?} block {} bytes", block.block_type, block.data.len());
        let last = block.last;
        metadata.push(block);
        if last {
            break;
        }
    }
    assert!(!metadata.is_empty(), "No metadata block found!");
    assert_eq!(metadata[0].block_type, BlockType::StreamInfo,
               "Invalid: first metadata block is not streaminfo!");
    let mut c = std::io::Cursor::new(&metadata[0].data);
    let stream_info = try!(parse_stream_info(&mut c));
    println!("  {:?}", stream_info);
    Ok(())
}

fn main() {
    for filename in std::env::args().nth(1) {
        convert(&filename).unwrap();
    }
}
