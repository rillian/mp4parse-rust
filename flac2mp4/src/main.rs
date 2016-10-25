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
    Reserved,
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

fn read_metadata_block<R: Read>(src: &mut R) -> Result<MetadataBlock> {
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

fn read_metadata<R: Read>(src: &mut R) -> Result<Vec<MetadataBlock>> {
    let mut metadata = Vec::new();
    loop {
        let block = try!(read_metadata_block(src));
        println!("  {:?} block {} bytes", block.block_type, block.data.len());
        let last = block.last;
        metadata.push(block);
        if last {
            break;
        }
    }
    Ok(metadata)
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
        ((buffer[2] & 0xf0) as u32) >> 4;
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

#[derive(Debug)]
struct Frame {
    block_size: u32,
    sample_rate: u32,
}

struct BlockSizeTable {
    Some(u32),
    Lookup8Bit,
    Lookup16Bit,
}

struct SampleRateTable {
    /// Defined value in Hz.
    Some(u32),
    /// Tags for the presence of variable-length fields.
    Lookup8Bit,
    Lookup16Bit,
    Lookup16Bit10x,
}

fn read_frame<R: Read>(src: &mut R, info: &StreamInfo) -> Result<Frame> {
    let sync = try!(src.read_u16::<BigEndian>());
    if sync >> 2 != 0b11111111111110 {
        return Err(Error::new(ErrorKind::InvalidData,
                              "Lost sync reading Frame Header!"));
    }
    if sync & 0b10 != 0 {
        return Err(Error::new(ErrorKind::InvalidData,
                              "non-zero reserved bit 14 in Frame Header"));
    }
    let blocking_strategy = sync & 0b01;

    let temp = try!(src.read_u16::<BigEndian>());
    let block_size = match temp >> 12 {
        0b0000 => return Err(Error::new(ErrorKind::InvalidData,
                                        "reserved block size in Frame Header")),
        0b0001 => Some(192),
        n @ 0b0010...0b0101 => Some(576 * 2u32.pow(n - 2)),
        0b0110 => Lookup8Bit,
        0b0111 => Lookup16Bit,
        n @ 0b1000...0b1111 => Some(256 * 2u32.pow(n - 8)),
    };
    let sample_rate = (match temp >> 8) & 0x000f {
        0b0000 => Some(info.sample_rate),
        0b0001 => Some( 88 200), // Hz
        0b0010 => Some(176 400),
        0b0011 => Some(192 000),
        0b0100 => Some(  8 000),
        0b0101 => Some( 16 000),
        0b0110 => Some( 22 050),
        0b0111 => Some( 24 000),
        0b1000 => Some( 32 000),
        0b1001 => Some( 44 100),
        0b1010 => Some( 48 000),
        0b1011 => Some( 96 000),
        0b1100 => Lookup8Bit,
        0b1101 => Lookup16Bit,
        0b1110 => Lookup16Bit10x,
        0b1111 => return Err(Error::new(ErrorKind::InvalidData,
                      "Invalid sample rate in Frame Header!")),
    };
    let channel_assignment = (temp >> 4) & 0x000f;
    let sample_size = match (temp >> 1) & 0x0007 {
        0b000 => info.bit_depth,
        0b001 => 8,
        0b010 => 12,
        0b100 => 16,
        0b101 => 20,
        0b110 => 24,
        0b011 | 0b111 => return Err(Error::new(ErrorKind::InvalidData,
                             "Invalid sample size in Frame Header!")),
    };

    if temp & 0x0001 {
        return Err(Error::new(ErrorKind::InvalidData,
                              "non-zero reserved bit 32 in Frame Header"));
    }

    // Read variable-length fields, if any.
    match blocking_strategy {
        0 => try!(src.read_varint(31)),
        1 => try!(src.read_varint(36)),
    }
    let block_size = match block_size {
        Some(v) => v,
        Lookup8Bit => try!(src.read_u8()),
        Lookup16 => try!(src.read_u16::<BigEndian>()),
    };
    let sample_rate = match sample_rate {
        Some(v) => v,
        Lookup8Bit => try!(src.read_u8()),
        Lookup16Bit => try!(src.read_u16::<BigEndian>()),
        Lookup16Bit10x => 10 * try!(src.read_u16::<BigEndian>()),
    };

    Ok(Frame {
        block_size = block_size,
        sample_rate = sample_rate,
    })
}

fn convert(filename: &str) -> Result<()> {
    let mut file = try!(File::open(filename));
    if is_flac(&mut file).is_err() {
        println!("Not a flac file: {}", filename);
        return Err(Error::new(ErrorKind::InvalidData, "Not a FLAC file"));
    }
    println!("Converting {}...", filename);
    let metadata = try!(read_metadata(&mut file));
    assert!(!metadata.is_empty(), "No metadata block found!");
    assert_eq!(metadata[0].block_type, BlockType::StreamInfo,
               "Invalid: first metadata block is not streaminfo!");
    let mut c = std::io::Cursor::new(&metadata[0].data);
    let stream_info = try!(parse_stream_info(&mut c));
    println!("  {:?}", stream_info);
    let frame = try!(read_frame(&mut file));
    println!("    {:?}", frame);
    Ok(())
}

fn main() {
    for filename in std::env::args().nth(1) {
        convert(&filename).unwrap();
    }
}
